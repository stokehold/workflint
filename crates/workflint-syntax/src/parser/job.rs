//! Jobs, as the reference's `parseJobs` and `parseJob` at
//! `parse.go:1328-1496`, with the job-level productions `parseRunsOn`,
//! `parseOutputs`, `parseTimeoutMinutes`, and `parseSnapshot`.
//!
//! Job IDs are case-insensitive and jobs keep their source order. Job keys
//! are case-sensitive. A job that calls a reusable workflow with `uses` may
//! not have the keys that only a job with steps may have, and a job without
//! `uses` needs `runs-on` and `steps` and may not have `with` or `secrets`.

use std::sync::Arc;

use workflint_diagnostics::quote::quote;
use workflint_yaml::{Node, NodeKind, Tag};

use super::mapping::Key;
use super::shared::mapping_field;
use super::{KeySpec, Label, MapEntry, Parser, Section, kind_name};
use crate::rules::{
    EMPTY_SECTION, INVALID_REUSABLE_CALL_KEY, MISSING_REQUIRED_KEY, NON_POSITIVE_NUMBER,
    REUSABLE_CALL_KEY_WITHOUT_USES, UNEXPECTED_NODE_KIND,
};
use crate::strconv::format_float;
use crate::{
    Field, Job, Keyed, KeyedEntry, Runner, Snapshot, Str, Template, Value, WorkflowCall,
    WorkflowCallSecrets,
};

/// A job while its keys are parsed, with what the checks after the keys
/// need.
#[derive(Default)]
struct JobBody<'a> {
    job: Job<'a>,
    /// `uses`, `with`, and `secrets`. It becomes the job's
    /// [`workflow_call`](Job::workflow_call) if the job may call a reusable
    /// workflow.
    call: WorkflowCall<'a>,
    /// The keys written that only a job with steps may have, in source
    /// order.
    steps_only: Vec<Key<'a>>,
    /// The keys written that only a job that calls a reusable workflow may
    /// have, in source order.
    call_only: Vec<Key<'a>>,
}

impl Section for JobBody<'static> {
    type Ast<'a> = JobBody<'a>;
}

/// The keys of a job, in the reference's order.
///
/// `runs-on`, `environment`, `outputs`, `env`, `defaults`, `steps`,
/// `timeout-minutes`, `continue-on-error`, `container`, and `services` are
/// only for a job with steps. The reference does not count `services`, so
/// workflint reports it but keeps the call
/// (`docs/differences.md#reusable-call-services`). `uses`, `with`, and
/// `secrets` are only for a job that calls a reusable workflow.
static JOB_KEYS: [KeySpec<JobBody<'static>>; 20] = [
    KeySpec {
        name: "name",
        parse: |p, b, e| b.job.name = p.parse_template(e.key.string, e.value, true),
    },
    KeySpec {
        name: "needs",
        parse: |p, b, e| {
            b.job.needs =
                p.parse_template_or_template_sequence(e.key.string, "needs", e.value, false, false);
        },
    },
    KeySpec {
        name: "runs-on",
        parse: |p, b, e| {
            b.job.runs_on = p.parse_runs_on(e);
            b.steps_only.push(e.key);
        },
    },
    KeySpec {
        name: "permissions",
        parse: |p, b, e| b.job.permissions = p.parse_permissions(e.key, e.value).into(),
    },
    KeySpec {
        name: "environment",
        parse: |p, b, e| {
            b.job.environment = p.parse_environment(e.key, e.value).into();
            b.steps_only.push(e.key);
        },
    },
    KeySpec {
        name: "concurrency",
        parse: |p, b, e| b.job.concurrency = p.parse_concurrency(e.key, e.value).into(),
    },
    KeySpec {
        name: "outputs",
        parse: |p, b, e| {
            b.job.outputs = p.parse_outputs(e).into();
            b.steps_only.push(e.key);
        },
    },
    KeySpec {
        name: "env",
        parse: |p, b, e| {
            b.job.env = p.parse_env(e.key, e.value).into();
            b.steps_only.push(e.key);
        },
    },
    KeySpec {
        name: "defaults",
        parse: |p, b, e| {
            b.job.defaults = p.parse_defaults(e.key, e.value, true).into();
            b.steps_only.push(e.key);
        },
    },
    KeySpec {
        name: "if",
        parse: |p, b, e| b.job.condition = p.parse_condition(e.key.string, e.value).into(),
    },
    KeySpec {
        name: "steps",
        parse: |p, b, e| {
            b.job.steps = p.parse_steps(e);
            b.steps_only.push(e.key);
        },
    },
    KeySpec {
        name: "timeout-minutes",
        parse: |p, b, e| {
            b.job.timeout_minutes = p.parse_timeout_minutes(e).into();
            b.steps_only.push(e.key);
        },
    },
    KeySpec {
        name: "strategy",
        parse: |p, b, e| b.job.strategy = p.parse_strategy(e).into(),
    },
    KeySpec {
        name: "continue-on-error",
        parse: |p, b, e| {
            b.job.continue_on_error = p.parse_bool(e.key.string, e.value).into();
            b.steps_only.push(e.key);
        },
    },
    KeySpec {
        name: "container",
        parse: |p, b, e| {
            b.job.container = p.parse_job_container(e).into();
            b.steps_only.push(e.key);
        },
    },
    KeySpec {
        name: "services",
        parse: |p, b, e| {
            b.job.services = p.parse_services(e).into();
            b.steps_only.push(e.key);
        },
    },
    KeySpec {
        name: "uses",
        parse: |p, b, e| {
            b.call.uses = p.parse_job_uses(e);
            b.call_only.push(e.key);
        },
    },
    KeySpec {
        name: "with",
        parse: |p, b, e| {
            b.call.inputs = p.parse_call_inputs(e);
            b.call_only.push(e.key);
        },
    },
    KeySpec {
        name: "secrets",
        parse: |p, b, e| {
            b.call.secrets = p.parse_call_secrets(e);
            b.call_only.push(e.key);
        },
    },
    KeySpec {
        name: "snapshot",
        parse: |p, b, e| b.job.snapshot = p.parse_snapshot(e).into(),
    },
];

impl Section for Runner<'static> {
    type Ast<'a> = Runner<'a>;
}

static RUNS_ON_KEYS: [KeySpec<Runner<'static>>; 2] = [
    KeySpec {
        name: "labels",
        parse: |p, r, e| {
            r.labels = match p.may_parse_expression(e.value) {
                Some(expr) => Value::Expression {
                    key: e.key.string,
                    expr,
                },
                None => p.parse_labels(e.key.string, "labels", e.value),
            };
        },
    },
    KeySpec {
        name: "group",
        parse: |p, r, e| r.group = p.parse_template(e.key.string, e.value, false),
    },
];

impl Section for Snapshot<'static> {
    type Ast<'a> = Snapshot<'a>;
}

static SNAPSHOT_KEYS: [KeySpec<Snapshot<'static>>; 3] = [
    KeySpec {
        name: "image-name",
        parse: |p, s, e| s.image_name = p.parse_string(e.key.string, e.value, false),
    },
    KeySpec {
        name: "version",
        parse: |p, s, e| s.version = p.parse_string(e.key.string, e.value, false),
    },
    KeySpec {
        name: "if",
        parse: |p, s, e| s.condition = p.parse_condition(e.key.string, e.value),
    },
];

impl<'a> Parser<'a, '_> {
    /// Parses `jobs`, the reference's `parseJobs`: a case-insensitive
    /// mapping of job IDs to jobs, in source order, that must have entries.
    ///
    /// A job whose ID is not a scalar is parsed under the ID `""`, as in the
    /// reference, so its errors are reported, and then left out, because a
    /// [`Keyed`] collection needs a string key.
    pub(super) fn parse_jobs(
        &mut self,
        key: Key<'a>,
        node: &'a Node,
    ) -> Field<'a, Keyed<'a, Job<'a>>> {
        let entries = self.parse_mapping(Label::Section("jobs"), node, false, false);
        let mut jobs = Vec::with_capacity(entries.len());
        for entry in entries {
            let job = self.within(entry.path, |p| p.parse_job(entry.key, entry.value));
            if let Some(id) = entry.key.string {
                jobs.push(KeyedEntry::new(id, entry.value.span, job));
            }
        }
        let span = node.span;
        let key = key.string;
        match &node.kind {
            NodeKind::Scalar(scalar) if scalar.tag != Tag::Null => Field::Invalid { key, span },
            NodeKind::Sequence(_) | NodeKind::Alias(_) => Field::Invalid { key, span },
            NodeKind::Scalar(_) | NodeKind::Mapping(_) => Field::Valid {
                key,
                span,
                value: Keyed::new(jobs),
            },
        }
    }

    /// Parses the job `id`, the reference's `parseJob`.
    ///
    /// After the keys come the checks of research section 3.6. With `uses`,
    /// each key that only a job with steps may have is reported, and the
    /// job then has no [`workflow_call`](Job::workflow_call), unless that
    /// key is `services` alone. Without
    /// `uses`, a missing `runs-on` or `steps` is reported at the job ID, and
    /// each of `with` and `secrets` is reported at its key. An invalid
    /// `steps` counts as missing, as in the reference. The reference
    /// reports only the last such key
    /// (`docs/differences.md#reusable-call-keys`).
    fn parse_job(&mut self, id: Key<'a>, node: &'a Node) -> Job<'a> {
        let name = quote(id.value());
        let label = format!("{name} job");
        let entries = self.parse_mapping(Label::At(&label), node, false, true);
        let mut body = JobBody::default();
        self.parse_keys(entries, "job", &JOB_KEYS, &mut body);
        let JobBody {
            mut job,
            call,
            steps_only,
            call_only,
        } = body;
        if call.uses.is_present() {
            for key in &steps_only {
                self.error(
                    &INVALID_REUSABLE_CALL_KEY,
                    key.span,
                    format!(
                        "when a reusable workflow is called with \"uses\", {} is not available. \
                         only following keys are allowed: \"name\", \"uses\", \"with\", \
                         \"secrets\", \"needs\", \"if\", and \"permissions\" in job {name}",
                        quote(key.value()),
                    ),
                );
            }
            // The reference keeps the call when `services` is the only such
            // key, because it does not count `services`.
            if steps_only.iter().all(|key| key.value() == "services") {
                job.workflow_call = Some(Box::new(call));
            }
            return job;
        }
        if !job.steps.is_valid() {
            self.error(
                &MISSING_REQUIRED_KEY,
                id.span,
                format!("\"steps\" section is missing in job {name}"),
            );
        }
        if !job.runs_on.is_present() {
            self.error(
                &MISSING_REQUIRED_KEY,
                id.span,
                format!("\"runs-on\" section is missing in job {name}"),
            );
        }
        for key in &call_only {
            self.error(
                &REUSABLE_CALL_KEY_WITHOUT_USES,
                key.span,
                format!(
                    "{} is only available for a reusable workflow call with \"uses\" but \"uses\" \
                     is not found in job {name}",
                    quote(key.value()),
                ),
            );
        }
        job
    }

    /// Parses `runs-on`, the reference's `parseRunsOn`.
    ///
    /// A `!!str` scalar that is a single `${{ }}` expression is the labels
    /// as an expression. Any other scalar is one label, and a sequence is a
    /// list of labels. Anything else is a mapping with `labels`, which takes
    /// the same three forms, `group`, and `${{ insert }}` keys.
    fn parse_runs_on(&mut self, entry: &MapEntry<'a>) -> Field<'a, Runner<'a>> {
        let key = entry.key.string;
        let node = entry.value;
        let span = node.span;
        if let Some(expr) = self.may_parse_expression(node) {
            let runner = Runner {
                labels: Value::Expression { key: None, expr },
                ..Runner::default()
            };
            return Field::Valid {
                key,
                span,
                value: runner,
            };
        }
        if let NodeKind::Scalar(_) | NodeKind::Sequence(_) = node.kind {
            let runner = Runner {
                labels: self.parse_labels(None, "runs-on", node),
                ..Runner::default()
            };
            return Field::Valid {
                key,
                span,
                value: runner,
            };
        }
        let mut runner = Runner::default();
        runner.inserts = self.parse_section_keys(node, "runs-on", &RUNS_ON_KEYS, &mut runner);
        mapping_field(entry.key, node, runner)
    }

    /// Parses `node`, the value of `key`, as a label or a non-empty sequence
    /// of labels, as the reference's `parseStringOrStringSequence` with
    /// `allowEmpty` and `allowElemEmpty` false.
    ///
    /// A label that is empty or not a scalar is reported and kept as
    /// `None`. The reference keeps it as `""`, and its `runner-label` rule
    /// then reports it again as an unknown label.
    fn parse_labels(
        &mut self,
        key: Option<Str<'a>>,
        section: &str,
        node: &'a Node,
    ) -> Value<'a, Arc<[Option<Template<'a>>]>> {
        match self.parse_template_or_template_sequence(key, section, node, false, false) {
            Field::Valid { key, span, value } => Value::Literal { key, span, value },
            Field::Invalid { key, span } => Value::Invalid { key, span },
            Field::Absent => Value::Absent,
        }
    }

    /// Parses `outputs` of a job, the reference's `parseOutputs`: a
    /// case-insensitive mapping of output names to strings that may be
    /// empty.
    ///
    /// It must have entries. Without any, it reports `"outputs" section
    /// should not be empty` besides the error of the mapping walker, as the
    /// reference does.
    fn parse_outputs(
        &mut self,
        entry: &MapEntry<'a>,
    ) -> Field<'a, Keyed<'a, Option<Template<'a>>>> {
        let node = entry.value;
        let entries = self.parse_mapping(Label::Section("outputs"), node, false, false);
        if entries.is_empty() {
            self.error(
                &EMPTY_SECTION,
                node.span,
                "\"outputs\" section should not be empty".to_owned(),
            );
        }
        let outputs = self.keyed_templates(entries, Vec::new(), true);
        mapping_field(entry.key, node, outputs)
    }

    /// Parses `timeout-minutes` of a job or a step, the reference's
    /// `parseTimeoutMinutes`: a float, or a single expression. A literal
    /// that is zero or less is reported and kept.
    pub(super) fn parse_timeout_minutes(&mut self, entry: &MapEntry<'a>) -> Value<'a, f64> {
        let value = self.parse_float(entry.key.string, entry.value);
        if let Value::Literal { value: minutes, .. } = value
            && minutes <= 0.0
        {
            self.non_positive("timeout-minutes", entry.value, &format_float(minutes));
        }
        value
    }

    /// Reports that the literal `value` of `node`, the value of `key`, is
    /// not greater than zero. `value` is the number as Go's `%v` prints it.
    pub(super) fn non_positive(&mut self, key: &str, node: &Node, value: &str) {
        self.error(
            &NON_POSITIVE_NUMBER,
            node.span,
            format!("value at {} must be greater than zero: {value}", quote(key)),
        );
    }

    /// Parses `with` of a job that calls a reusable workflow: a
    /// case-insensitive mapping of input names to strings that may be
    /// empty, and must have entries.
    fn parse_call_inputs(
        &mut self,
        entry: &MapEntry<'a>,
    ) -> Field<'a, Keyed<'a, Option<Template<'a>>>> {
        let node = entry.value;
        let entries = self.parse_mapping(Label::Section("with"), node, false, false);
        let inputs = self.keyed_templates(entries, Vec::new(), true);
        mapping_field(entry.key, node, inputs)
    }

    /// Parses `secrets` of a job that calls a reusable workflow: the scalar
    /// `inherit`, or a case-insensitive mapping of secret names to strings
    /// that may be empty, and must have entries.
    ///
    /// Any other scalar, such as `Inherit` or a null `secrets:`, reports
    /// `expected mapping node for secrets or "inherit" string node`.
    fn parse_call_secrets(&mut self, entry: &MapEntry<'a>) -> Field<'a, WorkflowCallSecrets<'a>> {
        let node = entry.value;
        let Some(scalar) = self.string(node) else {
            let entries = self.parse_mapping(Label::Section("secrets"), node, false, false);
            let secrets = self.keyed_templates(entries, Vec::new(), true);
            return mapping_field(entry.key, node, WorkflowCallSecrets::Map(secrets));
        };
        let key = entry.key.string;
        let span = node.span;
        if scalar.value() == "inherit" {
            return Field::Valid {
                key,
                span,
                value: WorkflowCallSecrets::Inherit(scalar),
            };
        }
        self.error(
            &UNEXPECTED_NODE_KIND,
            span,
            format!(
                "expected mapping node for secrets or \"inherit\" string node but found {} node",
                quote(scalar.value())
            ),
        );
        Field::Invalid { key, span }
    }

    /// Parses `snapshot` of a job, the reference's `parseSnapshot`.
    ///
    /// A scalar is the image name, with no key. A mapping has `image-name`,
    /// `version`, and `if`. Without an `image-name` key it reports
    /// `"snapshot" section must have "image-name" configuration` at the
    /// section's key. The mapping's errors name the `snapshot` section,
    /// where the reference's name `on`
    /// (`docs/differences.md#snapshot-section-name`).
    fn parse_snapshot(&mut self, entry: &MapEntry<'a>) -> Field<'a, Snapshot<'a>> {
        let key = entry.key;
        let node = entry.value;
        let span = node.span;
        match node.kind {
            NodeKind::Scalar(_) => {
                let snapshot = Snapshot {
                    image_name: self.parse_string(None, node, false),
                    ..Snapshot::default()
                };
                Field::Valid {
                    key: key.string,
                    span,
                    value: snapshot,
                }
            }
            NodeKind::Mapping(_) => {
                let mut snapshot = Snapshot::default();
                let entries = self.parse_mapping(Label::Section("snapshot"), node, false, true);
                self.parse_keys(entries, "snapshot", &SNAPSHOT_KEYS, &mut snapshot);
                if !snapshot.image_name.is_present() {
                    self.error(
                        &MISSING_REQUIRED_KEY,
                        key.span,
                        "\"snapshot\" section must have \"image-name\" configuration".to_owned(),
                    );
                }
                Field::Valid {
                    key: key.string,
                    span,
                    value: snapshot,
                }
            }
            NodeKind::Sequence(_) | NodeKind::Alias(_) => {
                self.error(
                    &UNEXPECTED_NODE_KIND,
                    span,
                    format!(
                        "\"snapshot\" section value must be string or mapping but found {} node",
                        kind_name(node)
                    ),
                );
                Field::Invalid {
                    key: key.string,
                    span,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
