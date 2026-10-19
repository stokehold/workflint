//! The `on` section and its events, as the reference's `parseEvents` and the
//! event parsers at `parse.go:381-736`.
//!
//! Event names are case-sensitive and the parser accepts any name: a name
//! that is not one of the five events with their own syntax is a webhook
//! event, and the `events` rule checks it. Input, secret, and output names
//! are case-insensitive.

use std::sync::Arc;

use workflint_diagnostics::Span;
use workflint_diagnostics::quote::quote;
use workflint_yaml::{Node, NodeKind, Tag};

use super::{Key, KeySpec, Label, MapEntry, Parser, Section, kind_name};
use crate::rules::{
    INVALID_DISPATCH_INPUT_TYPE, INVALID_WORKFLOW_CALL_INPUT_TYPE, MISSING_REQUIRED_KEY,
    SCHEDULE_WITHOUT_CONFIG, UNEXPECTED_NODE_KIND,
};
use crate::{
    Event, Field, ImageVersionEvent, Keyed, KeyedEntry, RepositoryDispatchEvent, ScheduleEntry,
    ScheduleEvent, Str, Template, WebhookEvent, WorkflowCallEvent, WorkflowCallEventInput,
    WorkflowCallEventOutput, WorkflowCallEventSecret, WorkflowCallInputType, WorkflowDispatchEvent,
    WorkflowDispatchInput, WorkflowDispatchInputType,
};

/// A string or a sequence of strings, such as a webhook filter.
type Strings<'a> = Field<'a, Arc<[Option<Template<'a>>]>>;

/// The filters of a webhook event, parsed before the event is built so that
/// the body of an event whose key is not a scalar is still checked.
#[derive(Default)]
struct Filters<'a> {
    types: Strings<'a>,
    branches: Strings<'a>,
    branches_ignore: Strings<'a>,
    tags: Strings<'a>,
    tags_ignore: Strings<'a>,
    paths: Strings<'a>,
    paths_ignore: Strings<'a>,
    workflows: Strings<'a>,
}

impl Section for Filters<'static> {
    type Ast<'a> = Filters<'a>;
}

static WEBHOOK_KEYS: [KeySpec<Filters<'static>>; 8] = [
    KeySpec {
        name: "types",
        parse: |p, f, e| f.types = p.parse_filter(e),
    },
    KeySpec {
        name: "branches",
        parse: |p, f, e| f.branches = p.parse_filter(e),
    },
    KeySpec {
        name: "branches-ignore",
        parse: |p, f, e| f.branches_ignore = p.parse_filter(e),
    },
    KeySpec {
        name: "tags",
        parse: |p, f, e| f.tags = p.parse_filter(e),
    },
    KeySpec {
        name: "tags-ignore",
        parse: |p, f, e| f.tags_ignore = p.parse_filter(e),
    },
    KeySpec {
        name: "paths",
        parse: |p, f, e| f.paths = p.parse_filter(e),
    },
    KeySpec {
        name: "paths-ignore",
        parse: |p, f, e| f.paths_ignore = p.parse_filter(e),
    },
    KeySpec {
        name: "workflows",
        parse: |p, f, e| f.workflows = p.parse_filter(e),
    },
];

impl Section for ScheduleEntry<'static> {
    type Ast<'a> = ScheduleEntry<'a>;
}

static SCHEDULE_KEYS: [KeySpec<ScheduleEntry<'static>>; 2] = [
    KeySpec {
        name: "cron",
        parse: |p, s, e| s.cron = p.parse_template(e.key.string, e.value, false),
    },
    KeySpec {
        name: "timezone",
        parse: |p, s, e| s.timezone = p.parse_template(e.key.string, e.value, false),
    },
];

impl Section for WorkflowDispatchEvent<'static> {
    type Ast<'a> = WorkflowDispatchEvent<'a>;
}

static DISPATCH_KEYS: [KeySpec<WorkflowDispatchEvent<'static>>; 1] = [KeySpec {
    name: "inputs",
    parse: |p, d, e| d.inputs = p.parse_keyed(e, "inputs", Parser::parse_dispatch_input),
}];

impl Section for WorkflowDispatchInput<'static> {
    type Ast<'a> = WorkflowDispatchInput<'a>;
}

/// The keys of a `workflow_dispatch` input. The reference lists only
/// `default`, `description`, and `required` when it reports an unexpected
/// key (`docs/differences.md#dispatch-input-key-list`).
static DISPATCH_INPUT_KEYS: [KeySpec<WorkflowDispatchInput<'static>>; 5] = [
    KeySpec {
        name: "description",
        parse: |p, i, e| i.description = p.parse_template(e.key.string, e.value, true),
    },
    KeySpec {
        name: "required",
        parse: |p, i, e| i.required = p.parse_bool(e.key.string, e.value),
    },
    KeySpec {
        name: "default",
        parse: |p, i, e| i.default = p.parse_template(e.key.string, e.value, true),
    },
    KeySpec {
        name: "type",
        parse: |p, i, e| i.input_type = p.parse_dispatch_input_type(e),
    },
    KeySpec {
        name: "options",
        parse: |p, i, e| {
            i.options = p.parse_template_sequence(e.key.string, "options", e.value, false, false);
        },
    },
];

impl Section for RepositoryDispatchEvent<'static> {
    type Ast<'a> = RepositoryDispatchEvent<'a>;
}

static REPOSITORY_DISPATCH_KEYS: [KeySpec<RepositoryDispatchEvent<'static>>; 1] = [KeySpec {
    name: "types",
    parse: |p, r, e| r.types = p.parse_filter(e),
}];

impl Section for WorkflowCallEvent<'static> {
    type Ast<'a> = WorkflowCallEvent<'a>;
}

static WORKFLOW_CALL_KEYS: [KeySpec<WorkflowCallEvent<'static>>; 3] = [
    KeySpec {
        name: "inputs",
        parse: |p, c, e| c.inputs = p.parse_keyed(e, "inputs", Parser::parse_call_input),
    },
    KeySpec {
        name: "secrets",
        parse: |p, c, e| c.secrets = p.parse_keyed(e, "secrets", Parser::parse_call_secret),
    },
    KeySpec {
        name: "outputs",
        parse: |p, c, e| c.outputs = p.parse_keyed(e, "outputs", Parser::parse_call_output),
    },
];

impl Section for WorkflowCallEventInput<'static> {
    type Ast<'a> = WorkflowCallEventInput<'a>;
}

static CALL_INPUT_KEYS: [KeySpec<WorkflowCallEventInput<'static>>; 4] = [
    KeySpec {
        name: "description",
        parse: |p, i, e| i.description = p.parse_template(e.key.string, e.value, true),
    },
    KeySpec {
        name: "required",
        parse: |p, i, e| i.required = p.parse_bool(e.key.string, e.value),
    },
    KeySpec {
        name: "default",
        parse: |p, i, e| i.default = p.parse_template(e.key.string, e.value, true),
    },
    KeySpec {
        name: "type",
        parse: |p, i, e| i.input_type = p.parse_call_input_type(e),
    },
];

impl Section for WorkflowCallEventSecret<'static> {
    type Ast<'a> = WorkflowCallEventSecret<'a>;
}

static CALL_SECRET_KEYS: [KeySpec<WorkflowCallEventSecret<'static>>; 2] = [
    KeySpec {
        name: "description",
        parse: |p, s, e| s.description = p.parse_template(e.key.string, e.value, true),
    },
    KeySpec {
        name: "required",
        parse: |p, s, e| s.required = p.parse_bool(e.key.string, e.value),
    },
];

impl Section for WorkflowCallEventOutput<'static> {
    type Ast<'a> = WorkflowCallEventOutput<'a>;
}

static CALL_OUTPUT_KEYS: [KeySpec<WorkflowCallEventOutput<'static>>; 2] = [
    KeySpec {
        name: "description",
        parse: |p, o, e| o.description = p.parse_template(e.key.string, e.value, true),
    },
    KeySpec {
        name: "value",
        parse: |p, o, e| o.value = p.parse_template(e.key.string, e.value, false),
    },
];

impl Section for ImageVersionEvent<'static> {
    type Ast<'a> = ImageVersionEvent<'a>;
}

static IMAGE_VERSION_KEYS: [KeySpec<ImageVersionEvent<'static>>; 2] = [
    KeySpec {
        name: "names",
        parse: |p, i, e| {
            i.names = p.parse_template_sequence(e.key.string, "names", e.value, false, false);
        },
    },
    KeySpec {
        name: "versions",
        parse: |p, i, e| {
            i.versions = p.parse_template_sequence(e.key.string, "versions", e.value, false, false);
        },
    },
];

impl<'a> Parser<'a, '_> {
    /// Parses `node`, the value of the `on` key `key`. The reference's
    /// `parseEvents`.
    ///
    /// A scalar names one event and a sequence names one per item, each
    /// with no configuration. A mapping names one per key, with its
    /// configuration under the key. An event that is reported and not
    /// built is kept as `None`. Any other node, which can only be an alias
    /// that stays unresolved, is invalid, and the reference then also
    /// reports `on` as missing.
    pub(super) fn parse_events(
        &mut self,
        key: Option<Str<'a>>,
        node: &'a Node,
    ) -> Field<'a, Arc<[Option<Event<'a>>]>> {
        let span = node.span;
        let mut events = Vec::new();
        match &node.kind {
            NodeKind::Scalar(_) => events.push(self.parse_event_with_no_config(node)),
            NodeKind::Sequence(_) => {
                for item in self.check_sequence("on", node, false).unwrap_or_default() {
                    events
                        .push(self.within(item.path, |p| p.parse_event_with_no_config(item.node)));
                }
            }
            NodeKind::Mapping(_) => {
                for entry in self.parse_mapping(Label::Section("on"), node, false, true) {
                    events.push(self.within(entry.path, |p| {
                        p.parse_event(&entry.id, entry.key, Some(entry.value))
                    }));
                }
            }
            NodeKind::Alias(_) => {
                self.error(
                    &UNEXPECTED_NODE_KIND,
                    span,
                    format!(
                        "\"on\" section value is expected to be mapping or sequence but found {} node",
                        kind_name(node)
                    ),
                );
                return Field::Invalid { key, span };
            }
        }
        Field::Valid {
            key,
            span,
            value: events.into(),
        }
    }

    /// Parses the scalar or sequence item `node` as the name of an event
    /// with no configuration. The reference's `parseEventWithNoConfig`.
    fn parse_event_with_no_config(&mut self, node: &'a Node) -> Option<Event<'a>> {
        let name = self.check_string(node, false)?;
        let key = Key {
            string: Some(name),
            span: node.span,
        };
        self.parse_event(name.value(), key, None)
    }

    /// Builds the event `id`, named by `name`, and parses `body`, the value
    /// under its key in the mapping form.
    ///
    /// Returns `None`, after reporting, for an event that is not built.
    /// `schedule` with no body is an error, and `schedule` with a body that
    /// is not a sequence with items is not built, as in the reference. An
    /// event whose key is not a scalar is a webhook event with the name
    /// `""` in the reference, which its `events` rule reports again as an
    /// unknown event. Its body is checked, but the event is not built.
    fn parse_event(
        &mut self,
        id: &str,
        name: Key<'a>,
        body: Option<&'a Node>,
    ) -> Option<Event<'a>> {
        let body_span = body.map(|node| node.span);
        let event = match id {
            "schedule" => {
                let Some(body) = body else {
                    self.error(
                        &SCHEDULE_WITHOUT_CONFIG,
                        name.span,
                        "schedule event must be configured with mapping".to_owned(),
                    );
                    return None;
                };
                Event::Schedule(self.parse_schedule(name.string?, body)?)
            }
            "workflow_dispatch" => {
                let mut event = WorkflowDispatchEvent {
                    name: name.string?,
                    body: body_span,
                    inputs: Field::Absent,
                };
                self.parse_body(body, "workflow_dispatch", &DISPATCH_KEYS, &mut event);
                Event::WorkflowDispatch(event)
            }
            "repository_dispatch" => {
                let mut event = RepositoryDispatchEvent {
                    name: name.string?,
                    body: body_span,
                    types: Field::Absent,
                };
                self.parse_body(
                    body,
                    "repository_dispatch",
                    &REPOSITORY_DISPATCH_KEYS,
                    &mut event,
                );
                Event::RepositoryDispatch(event)
            }
            "workflow_call" => {
                let mut event = WorkflowCallEvent {
                    name: name.string?,
                    body: body_span,
                    inputs: Field::Absent,
                    secrets: Field::Absent,
                    outputs: Field::Absent,
                };
                self.parse_body(body, "workflow_call", &WORKFLOW_CALL_KEYS, &mut event);
                Event::WorkflowCall(event)
            }
            "image_version" => {
                let mut event = ImageVersionEvent {
                    name: name.string?,
                    body: body_span,
                    names: Field::Absent,
                    versions: Field::Absent,
                };
                self.parse_body(body, "image_version", &IMAGE_VERSION_KEYS, &mut event);
                Event::ImageVersion(event)
            }
            _ => {
                let mut filters = Filters::default();
                self.parse_body(body, name.value(), &WEBHOOK_KEYS, &mut filters);
                Event::Webhook(WebhookEvent {
                    name: name.string?,
                    body: body_span,
                    types: filters.types,
                    branches: filters.branches,
                    branches_ignore: filters.branches_ignore,
                    tags: filters.tags,
                    tags_ignore: filters.tags_ignore,
                    paths: filters.paths,
                    paths_ignore: filters.paths_ignore,
                    workflows: filters.workflows,
                })
            }
        };
        Some(event)
    }

    /// Parses `body`, if any, as the case-sensitive mapping `section`, which
    /// may be empty, with the keys `keys`.
    fn parse_body<S: Section>(
        &mut self,
        body: Option<&'a Node>,
        section: &str,
        keys: &[KeySpec<S>],
        ast: &mut S::Ast<'a>,
    ) {
        if let Some(body) = body {
            let entries = self.parse_mapping(Label::Section(section), body, true, true);
            self.parse_keys(entries, section, keys, ast);
        }
    }

    /// Parses `node` as a sequence of `schedule` entries. The reference's
    /// `parseScheduleEvent`.
    ///
    /// The event is not built if `node` is not a sequence with items. An
    /// entry is dropped unless its `cron` is a valid string, as in the
    /// reference, so no rule sees it.
    fn parse_schedule(&mut self, name: Str<'a>, node: &'a Node) -> Option<ScheduleEvent<'a>> {
        const ELEMENT: &str = "element of \"schedule\" section";
        let items = self.check_sequence("schedule", node, false)?;
        let mut schedules = Vec::with_capacity(items.len());
        for item in items {
            let mut entry = ScheduleEntry {
                span: item.node.span,
                cron: Field::Absent,
                timezone: Field::Absent,
            };
            self.within(item.path, |p| {
                let entries = p.parse_mapping(Label::At(ELEMENT), item.node, false, true);
                p.parse_keys(entries, ELEMENT, &SCHEDULE_KEYS, &mut entry);
            });
            if entry.cron.is_valid() {
                schedules.push(entry);
            }
        }
        Some(ScheduleEvent {
            name,
            body: node.span,
            schedules: schedules.into(),
        })
    }

    /// Parses the value of `entry`, a webhook filter or the `types` of
    /// `repository_dispatch`, as a string or a non-empty sequence of
    /// non-empty strings. The reference's `parseWebhookEventFilter`.
    fn parse_filter(&mut self, entry: &MapEntry<'a>) -> Strings<'a> {
        self.parse_template_or_template_sequence(
            entry.key.string,
            entry.key.value(),
            entry.value,
            false,
            false,
        )
    }

    /// Parses the value of `entry` as the mapping `section` from
    /// case-insensitive names to items that `item` parses.
    ///
    /// A null value is an empty mapping. Any other value that is not a
    /// mapping is invalid, where the reference has an empty mapping. An
    /// item whose name is not a scalar is parsed and reported but not kept.
    fn parse_keyed<T>(
        &mut self,
        entry: &MapEntry<'a>,
        section: &str,
        item: fn(&mut Self, Key<'a>, &'a Node) -> T,
    ) -> Field<'a, Keyed<'a, T>> {
        let key = entry.key.string;
        let node = entry.value;
        let span = node.span;
        let mut items = Vec::new();
        for entry in self.parse_mapping(Label::Section(section), node, true, false) {
            let value = self.within(entry.path, |p| item(p, entry.key, entry.value));
            if let Some(name) = entry.key.string {
                items.push(KeyedEntry::new(name, entry.value.span, value));
            }
        }
        match &node.kind {
            NodeKind::Mapping(_) => {}
            NodeKind::Scalar(scalar) if scalar.tag == Tag::Null => {}
            NodeKind::Scalar(_) | NodeKind::Sequence(_) | NodeKind::Alias(_) => {
                return Field::Invalid { key, span };
            }
        }
        Field::Valid {
            key,
            span,
            value: Keyed::new(items),
        }
    }

    /// Parses `node`, the value of a `workflow_dispatch` input. The
    /// reference's `parseWorkflowDispatchEventInput`.
    fn parse_dispatch_input(&mut self, _: Key<'a>, node: &'a Node) -> WorkflowDispatchInput<'a> {
        let mut input = WorkflowDispatchInput::default();
        let label = Label::At("input settings of workflow_dispatch event");
        let entries = self.parse_mapping(label, node, true, true);
        self.parse_keys(entries, "inputs", &DISPATCH_INPUT_KEYS, &mut input);
        input
    }

    /// Parses the value of `entry` as the `type` of a `workflow_dispatch`
    /// input.
    fn parse_dispatch_input_type(
        &mut self,
        entry: &MapEntry<'a>,
    ) -> Field<'a, WorkflowDispatchInputType> {
        let parsed = self.parse_input_type(entry, |value| match value {
            "string" => Some(WorkflowDispatchInputType::String),
            "number" => Some(WorkflowDispatchInputType::Number),
            "boolean" => Some(WorkflowDispatchInputType::Boolean),
            "choice" => Some(WorkflowDispatchInputType::Choice),
            "environment" => Some(WorkflowDispatchInputType::Environment),
            _ => None,
        });
        parsed.unwrap_or_else(|(span, value)| {
            self.error(
                &INVALID_DISPATCH_INPUT_TYPE,
                span,
                format!(
                    "input type of workflow_dispatch event must be one of \"string\", \"number\", \"boolean\", \"choice\", \"environment\" but got {}",
                    quote(value)
                ),
            );
            Field::Invalid {
                key: entry.key.string,
                span,
            }
        })
    }

    /// Parses `node`, the value of the `workflow_call` input `name`, and
    /// reports a missing `type` at `name`. The reference's
    /// `parseWorkflowCallEventInput`.
    fn parse_call_input(&mut self, name: Key<'a>, node: &'a Node) -> WorkflowCallEventInput<'a> {
        let mut input = WorkflowCallEventInput::default();
        let label = Label::At("input of workflow_call event");
        let entries = self.parse_mapping(label, node, true, true);
        self.parse_keys(
            entries,
            "inputs at workflow_call event",
            &CALL_INPUT_KEYS,
            &mut input,
        );
        if !input.input_type.is_present() {
            self.error(
                &MISSING_REQUIRED_KEY,
                name.span,
                format!(
                    "\"type\" is missing at {} input of workflow_call event",
                    quote(name.value())
                ),
            );
        }
        input
    }

    /// Parses the value of `entry` as the `type` of a `workflow_call` input.
    fn parse_call_input_type(&mut self, entry: &MapEntry<'a>) -> Field<'a, WorkflowCallInputType> {
        let parsed = self.parse_input_type(entry, |value| match value {
            "boolean" => Some(WorkflowCallInputType::Boolean),
            "number" => Some(WorkflowCallInputType::Number),
            "string" => Some(WorkflowCallInputType::String),
            _ => None,
        });
        parsed.unwrap_or_else(|(span, value)| {
            self.error(
                &INVALID_WORKFLOW_CALL_INPUT_TYPE,
                span,
                format!(
                    "invalid value {} for input type of workflow_call event. it must be one of \"boolean\", \"number\", or \"string\"",
                    quote(value)
                ),
            );
            Field::Invalid {
                key: entry.key.string,
                span,
            }
        })
    }

    /// Parses the value of `entry` as a non-empty string that `parse` turns
    /// into an input type. Returns the span and the string when `parse`
    /// does not know it, for the caller to report.
    fn parse_input_type<T>(
        &mut self,
        entry: &MapEntry<'a>,
        parse: fn(&str) -> Option<T>,
    ) -> Result<Field<'a, T>, (Span, &'a str)> {
        let key = entry.key.string;
        let span = entry.value.span;
        let Some(string) = self.check_string(entry.value, false) else {
            return Ok(Field::Invalid { key, span });
        };
        match parse(string.value()) {
            Some(value) => Ok(Field::Valid { key, span, value }),
            None => Err((span, string.value())),
        }
    }

    /// Parses `node`, the value of a `workflow_call` secret. The
    /// reference's `parseWorkflowCallEventSecret`.
    fn parse_call_secret(&mut self, _: Key<'a>, node: &'a Node) -> WorkflowCallEventSecret<'a> {
        let mut secret = WorkflowCallEventSecret::default();
        let label = Label::At("secret of workflow_call event");
        let entries = self.parse_mapping(label, node, true, true);
        self.parse_keys(entries, "secrets", &CALL_SECRET_KEYS, &mut secret);
        secret
    }

    /// Parses `node`, the value of the `workflow_call` output `name`, and
    /// reports a missing `value` at `name`. The reference's
    /// `parseWorkflowCallEventOutput`.
    fn parse_call_output(&mut self, name: Key<'a>, node: &'a Node) -> WorkflowCallEventOutput<'a> {
        let mut output = WorkflowCallEventOutput::default();
        let label = Label::At("output of workflow_call event");
        let entries = self.parse_mapping(label, node, true, true);
        self.parse_keys(
            entries,
            "outputs at workflow_call event",
            &CALL_OUTPUT_KEYS,
            &mut output,
        );
        if !output.value.is_present() {
            self.error(
                &MISSING_REQUIRED_KEY,
                name.span,
                format!(
                    "\"value\" is missing at {} output of workflow_call event",
                    quote(name.value())
                ),
            );
        }
        output
    }
}

#[cfg(test)]
mod tests;
