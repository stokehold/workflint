//! Steps, as the reference's `parseSteps`, `parseStep`,
//! `parseStepExecRun`, and `parseStepExecAction` at `parse.go:1120-1267`.
//!
//! Step keys are case-sensitive, and the keys of `with` are not. The later
//! of `run` and `uses` in source order decides whether a step runs a script
//! or an action. The keys of the other kind, including the key that lost,
//! are then unexpected. A step with neither key is reported, and its other
//! keys are not checked.

use std::sync::Arc;

use workflint_yaml::Node;

use super::mapping::Mapping;
use super::shared::mapping_field;
use super::{Inserts, KeySpec, Label, MapEntry, Parser, Section};
use crate::rules::STEP_WITHOUT_RUN_OR_USES;
use crate::{Exec, ExecAction, ExecRun, Field, Step, Str, Template, Value, contains_expression};

/// What a step runs, from the later of its `run` and `uses` keys.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// Neither key is written.
    Unknown,
    /// `uses`, with a value that does not start with `docker://`.
    Action,
    /// `uses`, with a value that starts with `docker://`.
    Docker,
    /// `run`.
    Run,
}

/// A key that both kinds of step have. The first pass over the step's keys
/// parses it, so the second pass skips it.
const fn common<S: Section>(name: &'static str) -> KeySpec<S> {
    KeySpec {
        name,
        parse: |_, _, _| {},
    }
}

impl Section for ExecRun<'static> {
    type Ast<'a> = ExecRun<'a>;
}

/// The keys of a step that runs a script, in the reference's order.
static RUN_STEP_KEYS: [KeySpec<ExecRun<'static>>; 9] = [
    common("id"),
    common("if"),
    common("name"),
    common("env"),
    common("continue-on-error"),
    common("timeout-minutes"),
    KeySpec {
        name: "run",
        parse: |p, r, e| r.run = p.parse_template(e.key.string, e.value, false),
    },
    KeySpec {
        name: "shell",
        parse: |p, r, e| r.shell = p.parse_template(e.key.string, e.value, false).into(),
    },
    KeySpec {
        name: "working-directory",
        parse: |p, r, e| {
            r.working_directory = p.parse_template(e.key.string, e.value, false).into();
        },
    },
];

/// A step that runs an action while its keys are parsed.
struct ActionBody<'a> {
    action: ExecAction<'a>,
    /// Whether the action is a Docker image, which decides where
    /// `with.entrypoint` and `with.args` go.
    docker: bool,
}

impl Section for ActionBody<'static> {
    type Ast<'a> = ActionBody<'a>;
}

/// The keys of a step that runs an action, in the reference's order.
static ACTION_STEP_KEYS: [KeySpec<ActionBody<'static>>; 8] = [
    common("id"),
    common("if"),
    common("name"),
    common("env"),
    common("continue-on-error"),
    common("timeout-minutes"),
    KeySpec {
        name: "uses",
        parse: |p, b, e| b.action.uses = p.parse_step_uses(e),
    },
    KeySpec {
        name: "with",
        parse: |p, b, e| p.parse_step_inputs(b, e),
    },
];

impl<'a> Parser<'a, '_> {
    /// Parses `steps` of a job, the reference's `parseSteps`: a sequence of
    /// steps that must have items.
    ///
    /// Any other value is reported and gives an invalid field, which the job
    /// then reports as missing, as in the reference.
    pub(super) fn parse_steps(&mut self, entry: &MapEntry<'a>) -> Field<'a, Arc<[Step<'a>]>> {
        let key = entry.key.string;
        let node = entry.value;
        let span = node.span;
        let Some(items) = self.check_sequence("steps", node, false) else {
            return Field::Invalid { key, span };
        };
        let mut steps = Vec::with_capacity(items.len());
        for item in items {
            steps.push(self.within(item.path, |p| p.parse_step(item.node)));
        }
        Field::Valid {
            key,
            span,
            value: steps.into(),
        }
    }

    /// Parses one step, the reference's `parseStep`.
    ///
    /// The first pass parses the keys that both kinds of step have and finds
    /// the kind. The second pass parses the keys of that kind and reports
    /// the others. A step that is not a mapping is reported by the mapping
    /// walker and as a step with neither `run` nor `uses`.
    fn parse_step(&mut self, node: &'a Node) -> Step<'a> {
        let entries =
            self.parse_mapping(Label::At("element of \"steps\" section"), node, false, true);
        let mut step = Step {
            span: node.span,
            id: Field::Absent.into(),
            condition: Field::Absent.into(),
            name: Field::Absent,
            exec: None,
            env: Value::Absent.into(),
            continue_on_error: Value::Absent.into(),
            timeout_minutes: Value::Absent.into(),
        };
        let mut kind = Kind::Unknown;
        for entry in &entries {
            let key = entry.key.string;
            self.within(entry.path, |p| match &*entry.id {
                "id" => step.id = p.parse_step_id(key, entry.value).into(),
                "if" => step.condition = p.parse_condition(key, entry.value).into(),
                "name" => step.name = p.parse_template(key, entry.value, true),
                "env" => step.env = p.parse_env(entry.key, entry.value).into(),
                "continue-on-error" => {
                    step.continue_on_error = p.parse_bool(key, entry.value).into();
                }
                "timeout-minutes" => step.timeout_minutes = p.parse_timeout_minutes(entry).into(),
                // The reference reads the value without checking its kind,
                // so a value that is not a scalar makes an action step.
                "uses" => {
                    let docker = Str::new(entry.value)
                        .is_some_and(|uses| uses.value().starts_with("docker://"));
                    kind = if docker { Kind::Docker } else { Kind::Action };
                }
                "run" => kind = Kind::Run,
                _ => {}
            });
        }
        step.exec = match kind {
            Kind::Action | Kind::Docker => {
                let mut body = ActionBody {
                    action: ExecAction::default(),
                    docker: kind == Kind::Docker,
                };
                self.parse_keys(
                    entries,
                    "step to execute action",
                    &ACTION_STEP_KEYS,
                    &mut body,
                );
                Some(Exec::Action(body.action))
            }
            Kind::Run => {
                let mut run = ExecRun::default();
                self.parse_keys(
                    entries,
                    "step to run shell command",
                    &RUN_STEP_KEYS,
                    &mut run,
                );
                Some(Exec::Run(run))
            }
            Kind::Unknown => {
                self.error(
                    &STEP_WITHOUT_RUN_OR_USES,
                    node.span,
                    "step must run script with \"run\" section or run action with \"uses\" section"
                        .to_owned(),
                );
                None
            }
        };
        step
    }

    /// Parses `with` of a step that runs an action: a case-insensitive
    /// mapping of input names to strings that may be empty, and must have
    /// entries or an `${{ insert }}` key, whose value must be one
    /// expression.
    ///
    /// For a Docker image, `entrypoint` and `args` go to their own fields,
    /// as in the reference. `entrypoint` must not be empty, and `args` may
    /// be.
    fn parse_step_inputs(&mut self, body: &mut ActionBody<'a>, entry: &MapEntry<'a>) {
        let node = entry.value;
        let Mapping {
            mut entries,
            inserts,
        } = self.parse_mapping_with(
            Label::Section("with"),
            node,
            false,
            false,
            Inserts::Expression,
        );
        if body.docker {
            entries.retain(|input| match &*input.id {
                "entrypoint" => {
                    body.action.entrypoint = self
                        .within(input.path, |p| {
                            p.parse_template(input.key.string, input.value, false)
                        })
                        .into();
                    false
                }
                "args" => {
                    body.action.args = self
                        .within(input.path, |p| {
                            p.parse_template(input.key.string, input.value, true)
                        })
                        .into();
                    false
                }
                _ => true,
            });
        }
        let inputs = self.keyed_templates(entries, inserts, true);
        body.action.inputs = mapping_field(entry.key, node, inputs);
    }

    /// Parses `id` of a step. Its placeholders are parsed only if it
    /// [contains an expression](contains_expression), because only then
    /// does the reference check them.
    fn parse_step_id(&mut self, key: Option<Str<'a>>, node: &'a Node) -> Field<'a, Template<'a>> {
        self.parse_string(key, node, false).map(|id| {
            if contains_expression(id.value()) {
                self.template(id)
            } else {
                Template::plain(id)
            }
        })
    }
}

#[cfg(test)]
mod tests;
