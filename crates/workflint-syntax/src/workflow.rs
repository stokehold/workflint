//! The workflow root and the sections that workflows and jobs share.

use std::sync::Arc;

use workflint_diagnostics::Span;

use crate::{Event, Field, Job, Keyed, Str, Value, WorkflowCallEvent};

/// A workflow file.
///
/// The reference's `Workflow` type. Its sections keep the order they are
/// written in, and its jobs are a [`Keyed`] collection in source order.
#[derive(Debug, Clone, PartialEq)]
pub struct Workflow<'a> {
    /// From the start of the YAML document to the end of the root node. The
    /// document starts at its first directive, its `---`, or else its root
    /// node. The reference reports a missing `on` or `jobs` section at this
    /// start.
    pub span: Span,
    /// `name`. It may be empty.
    pub name: Field<'a, Str<'a>>,
    /// `run-name`.
    pub run_name: Field<'a, Str<'a>>,
    /// `on`: the events that trigger the workflow, in source order.
    ///
    /// The list is empty when `on` is written but names no valid event,
    /// such as `on:` with a null value. `on` is invalid when its value is
    /// an alias that stays unresolved, and the reference then reports it as
    /// missing.
    pub on: Field<'a, Arc<[Event<'a>]>>,
    /// `permissions`. The reference's `Permissions.Pos` is this field's key.
    pub permissions: Field<'a, Permissions<'a>>,
    /// `env`.
    pub env: Value<'a, Env<'a>>,
    /// `defaults`. The reference's `Defaults.Pos` is this field's key.
    pub defaults: Field<'a, Defaults<'a>>,
    /// `concurrency`. The reference's `Concurrency.Pos` is this field's key.
    pub concurrency: Field<'a, Concurrency<'a>>,
    /// `jobs`, keyed by job ID, in source order. A null `jobs:` has no
    /// entries. A job whose ID is not a scalar is left out after the parser
    /// reports it.
    pub jobs: Field<'a, Keyed<'a, Job<'a>>>,
}

impl<'a> Workflow<'a> {
    /// Returns the first `workflow_call` event in `on`, the reference's
    /// `FindWorkflowCallEvent`.
    #[must_use]
    pub fn workflow_call_event(&self) -> Option<&WorkflowCallEvent<'a>> {
        self.on.value()?.iter().find_map(|event| match event {
            Event::WorkflowCall(event) => Some(event),
            _ => None,
        })
    }
}

/// `env` of a workflow, job, step, or container: the variables keyed by
/// name.
///
/// A variable's value is `None` when it is not a scalar, which the parser
/// reports. The reference's `EnvVar.Name` is the entry's key. Its
/// `Env.Expression` is [`Value::Expression`] on the field.
pub type Env<'a> = Keyed<'a, Option<Str<'a>>>;

/// `permissions` of a workflow or job.
#[derive(Debug, Clone, PartialEq)]
pub enum Permissions<'a> {
    /// The string form, such as `read-all`. The reference's
    /// `Permissions.All`.
    ///
    /// It may be empty, as in `permissions:` with no value. The parser
    /// reports an empty string and keeps it, and the `permissions` rule
    /// reports it as an invalid value too, as in the reference.
    All(Str<'a>),
    /// The mapping form, keyed by scope name. A scope's value is `None`
    /// when it is not a non-empty string, which the parser reports. The
    /// reference keeps such a value as `""` and its `permissions` rule
    /// reports it at the entry's span. The reference's
    /// `PermissionScope.Name` is the entry's key.
    Scopes(Keyed<'a, Option<Str<'a>>>),
}

/// `defaults` of a workflow or job.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Defaults<'a> {
    /// `run`. The reference's `DefaultsRun.Pos` is this field's key.
    pub run: Field<'a, DefaultsRun<'a>>,
}

/// `defaults.run`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DefaultsRun<'a> {
    /// `shell`.
    pub shell: Field<'a, Str<'a>>,
    /// `working-directory`.
    pub working_directory: Field<'a, Str<'a>>,
}

/// `concurrency` of a workflow or job.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Concurrency<'a> {
    /// `group`. In the string form, `concurrency: ci`, the whole value is
    /// the group and the field has no key.
    pub group: Field<'a, Str<'a>>,
    /// `cancel-in-progress`.
    pub cancel_in_progress: Value<'a, bool>,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use std::sync::Arc;

    use workflint_diagnostics::{FileId, LineCol, SourceFile};
    use workflint_yaml::Document;

    use super::*;
    use crate::testing::{document, key_at, node_at, str_at};
    use crate::{
        Exec, ExecRun, KeyedEntry, Matrix, MatrixRow, RawValue, Step, Strategy, WebhookEvent,
        WorkflowCallEventInput,
    };

    const WORKFLOW: &str = "\
on:
  push:
  workflow_call:
    inputs:
      Version:
        required: true
jobs:
  Build:
    steps: &steps
      - run: make
  test:
    steps: *steps
    strategy:
      matrix:
        python: [1, '1']
";

    fn valid<'a, T>(document: &'a Document, path: &[&str], value: T) -> Field<'a, T> {
        Field::Valid {
            key: Some(key_at(document, path)),
            span: node_at(document, path).span,
            value,
        }
    }

    fn entry<'a, T>(document: &'a Document, path: &[&str], value: T) -> KeyedEntry<'a, T> {
        KeyedEntry::new(key_at(document, path), node_at(document, path).span, value)
    }

    /// Builds the events of `WORKFLOW`.
    fn events(document: &Document) -> Arc<[Event<'_>]> {
        let input = WorkflowCallEventInput {
            required: Value::Literal {
                key: Some(key_at(
                    document,
                    &["on", "workflow_call", "inputs", "Version", "required"],
                )),
                span: node_at(
                    document,
                    &["on", "workflow_call", "inputs", "Version", "required"],
                )
                .span,
                value: true,
            },
            ..WorkflowCallEventInput::default()
        };
        let inputs = Keyed::new(vec![entry(
            document,
            &["on", "workflow_call", "inputs", "Version"],
            input,
        )]);
        Arc::new([
            Event::Webhook(WebhookEvent {
                name: key_at(document, &["on", "push"]),
                body: Some(node_at(document, &["on", "push"]).span),
                types: Field::Absent,
                branches: Field::Absent,
                branches_ignore: Field::Absent,
                tags: Field::Absent,
                tags_ignore: Field::Absent,
                paths: Field::Absent,
                paths_ignore: Field::Absent,
                workflows: Field::Absent,
            }),
            Event::WorkflowCall(WorkflowCallEvent {
                name: key_at(document, &["on", "workflow_call"]),
                body: Some(node_at(document, &["on", "workflow_call"]).span),
                inputs: valid(document, &["on", "workflow_call", "inputs"], inputs),
                secrets: Field::Absent,
                outputs: Field::Absent,
            }),
        ])
    }

    /// Builds the AST of `WORKFLOW` the way the parser will, sharing the
    /// anchored step list between both jobs.
    fn workflow(document: &Document) -> Workflow<'_> {
        let steps: Arc<[Step<'_>]> = Arc::new([Step {
            span: node_at(document, &["jobs", "Build", "steps", "0"]).span,
            id: Field::Absent,
            condition: Field::Absent,
            name: Field::Absent,
            exec: Some(Exec::Run(ExecRun {
                run: valid(
                    document,
                    &["jobs", "Build", "steps", "0", "run"],
                    str_at(document, &["jobs", "Build", "steps", "0", "run"]),
                ),
                ..ExecRun::default()
            })),
            env: Value::Absent,
            continue_on_error: Value::Absent,
            timeout_minutes: Value::Absent,
        }]);
        let build = Job {
            steps: valid(document, &["jobs", "Build", "steps"], Arc::clone(&steps)),
            ..Job::default()
        };

        let python = ["jobs", "test", "strategy", "matrix", "python"];
        let values: Arc<[RawValue<'_>]> = (0..2)
            .map(|i| RawValue::Scalar(str_at(document, &[&python[..], &[&i.to_string()]].concat())))
            .collect();
        let matrix = Matrix {
            rows: Keyed::new(vec![entry(document, &python, MatrixRow::Values(values))]),
            ..Matrix::default()
        };
        let test = Job {
            steps: valid(document, &["jobs", "test", "steps"], steps),
            strategy: valid(
                document,
                &["jobs", "test", "strategy"],
                Strategy {
                    matrix: Value::Literal {
                        key: Some(key_at(document, &python[..4])),
                        span: node_at(document, &python[..4]).span,
                        value: matrix,
                    },
                    ..Strategy::default()
                },
            ),
            ..Job::default()
        };
        let jobs = Keyed::new(vec![
            entry(document, &["jobs", "Build"], build),
            entry(document, &["jobs", "test"], test),
        ]);

        Workflow {
            span: node_at(document, &[]).span,
            name: Field::Absent,
            run_name: Field::Absent,
            on: valid(document, &["on"], events(document)),
            permissions: Field::Absent,
            env: Value::Absent,
            defaults: Field::Absent,
            concurrency: Field::Absent,
            jobs: valid(document, &["jobs"], jobs),
        }
    }

    #[test]
    fn jobs_keep_source_order_and_are_found_by_any_case() {
        let document = document(WORKFLOW);
        let workflow = workflow(&document);
        let jobs = workflow.jobs.value().unwrap();

        let ids: Vec<_> = jobs.iter().map(|job| job.key.value()).collect();
        assert_eq!(ids, ["Build", "test"]);
        let build = jobs.get("build").unwrap();
        assert_eq!(build.key.value(), "Build");
        // The reference reports a job at its ID, line 8, column 3.
        let file = SourceFile::new(FileId::new(0), "test.yml", WORKFLOW).unwrap();
        let LineCol { line, column } = file.line_col(build.key.span().start);
        assert_eq!((line, column), (8, 3));
    }

    #[test]
    fn the_workflow_call_event_is_found_after_other_events() {
        let document = document(WORKFLOW);
        let workflow = workflow(&document);

        let call = workflow.workflow_call_event().unwrap();
        assert_eq!(call.name.value(), "workflow_call");
        let names: Vec<_> = workflow
            .on
            .value()
            .unwrap()
            .iter()
            .map(|e| e.name().value())
            .collect();
        assert_eq!(names, ["push", "workflow_call"]);
        let inputs = call.inputs.value().unwrap();
        assert_eq!(inputs.entries()[0].folded(), "version");
        assert!(inputs.get("VERSION").unwrap().value.is_required());
    }

    #[test]
    fn an_aliased_step_list_is_shared_between_jobs() {
        let document = document(WORKFLOW);
        let workflow = workflow(&document);
        let jobs = workflow.jobs.value().unwrap();

        let steps = |id| jobs.get(id).unwrap().value.steps.value().unwrap();
        assert!(Arc::ptr_eq(steps("build"), steps("test")));
    }

    #[test]
    fn matrix_values_keep_their_tag() {
        let document = document(WORKFLOW);
        let workflow = workflow(&document);
        let test = &workflow.jobs.value().unwrap().get("test").unwrap().value;
        let matrix = test.strategy.value().unwrap().matrix.literal().unwrap();

        let MatrixRow::Values(values) = &matrix.rows.get("PYTHON").unwrap().value else {
            panic!("not a list of values");
        };
        let tags: Vec<_> = values
            .iter()
            .map(|value| match value {
                RawValue::Scalar(scalar) => (scalar.value(), scalar.tag().as_str()),
                _ => panic!("not a scalar"),
            })
            .collect();
        assert_eq!(tags, [("1", "!!int"), ("1", "!!str")]);
    }
}
