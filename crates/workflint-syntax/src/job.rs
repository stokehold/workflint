//! Jobs and their sections.

use std::sync::Arc;

use crate::{
    Concurrency, Defaults, Env, Field, JobUses, Keyed, Matrix, Permissions, Step, Str, Template,
    Value,
};

/// One job of `jobs`.
///
/// The job ID is the key of the job's entry in
/// [`Workflow::jobs`](crate::Workflow::jobs). The key's span is the
/// reference's `Job.Pos`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Job<'a> {
    /// `name`. It may be empty.
    pub name: Field<'a, Template<'a>>,
    /// `needs`: job IDs, in source order. A string becomes a list of one.
    pub needs: Field<'a, Arc<[Template<'a>]>>,
    /// `runs-on`.
    pub runs_on: Field<'a, Runner<'a>>,
    /// `permissions`. The reference's `Permissions.Pos` is this field's key.
    pub permissions: Field<'a, Permissions<'a>>,
    /// `environment`. The reference's `Environment.Pos` is this field's key.
    pub environment: Field<'a, Environment<'a>>,
    /// `concurrency`. The reference's `Concurrency.Pos` is this field's key.
    pub concurrency: Field<'a, Concurrency<'a>>,
    /// `outputs`, keyed by output name. An output's value is `None` when it
    /// is not a scalar, which the parser reports. The reference's
    /// `Output.Name` is the entry's key.
    pub outputs: Field<'a, Keyed<'a, Option<Template<'a>>>>,
    /// `env`.
    pub env: Value<'a, Env<'a>>,
    /// `defaults`. The reference's `Defaults.Pos` is this field's key.
    pub defaults: Field<'a, Defaults<'a>>,
    /// `if`.
    pub condition: Field<'a, Template<'a>>,
    /// `steps`, in source order.
    pub steps: Field<'a, Arc<[Step<'a>]>>,
    /// `timeout-minutes`. A literal that is zero or less stays a literal
    /// after the parser reports it.
    pub timeout_minutes: Value<'a, f64>,
    /// `strategy`. The reference's `Strategy.Pos` is this field's key.
    pub strategy: Field<'a, Strategy<'a>>,
    /// `continue-on-error`.
    pub continue_on_error: Value<'a, bool>,
    /// `container`. The reference's `Container.Pos` is this field's key.
    pub container: Field<'a, Container<'a>>,
    /// `services`. The reference's `Services.Pos` is the span of this
    /// field's value.
    pub services: Value<'a, Services<'a>>,
    /// The call of a reusable workflow, from `uses`, `with`, and `secrets`.
    ///
    /// It is set only when `uses` is written, valid or not, and no key that
    /// only a job with steps may have is written, such as `runs-on` or
    /// `services`. The reference does not count `services`
    /// (`docs/differences.md#reusable-call-services`).
    pub workflow_call: Option<WorkflowCall<'a>>,
    /// `snapshot`. The reference's `snapshot` position is this field's key.
    pub snapshot: Field<'a, Snapshot<'a>>,
}

/// `runs-on`: the runner a job runs on.
///
/// The string and sequence forms give only `labels`, with no key. A whole
/// `runs-on: ${{ }}` is [`Value::Expression`] on `labels`, as in the
/// reference, where it is `LabelsExpr`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Runner<'a> {
    /// `labels`, in source order. A string becomes a list of one. Labels
    /// that contain an expression among other text stay labels.
    ///
    /// An empty label is kept after the parser reports it, as in the
    /// reference, where the `runner-label` rule reports it as an unknown
    /// label too. A label that is not a scalar is reported and left out.
    pub labels: Value<'a, Arc<[Template<'a>]>>,
    /// `group`.
    pub group: Field<'a, Template<'a>>,
}

/// `environment` of a job.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Environment<'a> {
    /// `name`. In the string form, `environment: production`, the whole
    /// value is the name and the field has no key.
    pub name: Field<'a, Template<'a>>,
    /// `url`.
    pub url: Field<'a, Template<'a>>,
    /// `deployment`.
    pub deployment: Value<'a, bool>,
}

/// `strategy` of a job.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Strategy<'a> {
    /// `matrix`.
    ///
    /// The reference's `Matrix.Pos` is this field's key for a mapping and
    /// the span of the value for an expression.
    pub matrix: Value<'a, Matrix<'a>>,
    /// `fail-fast`.
    pub fail_fast: Value<'a, bool>,
    /// `max-parallel`.
    pub max_parallel: Value<'a, i64>,
}

/// `container` of a job, or one service of `services`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Container<'a> {
    /// `image`. In the string form, `container: node:24`, the whole value
    /// is the image and the field has no key.
    pub image: Field<'a, Template<'a>>,
    /// `credentials`. The reference's `Credentials.Pos` is this field's key.
    pub credentials: Value<'a, Credentials<'a>>,
    /// `env`.
    pub env: Value<'a, Env<'a>>,
    /// `ports`, in source order.
    pub ports: Field<'a, Arc<[Template<'a>]>>,
    /// `volumes`, in source order. The reference stores them in `Ports`
    /// (`docs/differences.md#container-volumes`).
    pub volumes: Field<'a, Arc<[Template<'a>]>>,
    /// `options`.
    pub options: Field<'a, Template<'a>>,
    /// `command`. Only a service may have it.
    pub command: Field<'a, Template<'a>>,
    /// `entrypoint`. Only a service may have it.
    pub entrypoint: Field<'a, Template<'a>>,
}

/// `credentials` of a container.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Credentials<'a> {
    /// `username`.
    pub username: Field<'a, Template<'a>>,
    /// `password`.
    pub password: Field<'a, Template<'a>>,
}

/// `services` of a job, keyed by service ID.
///
/// The reference's `Service.Name` is the entry's key, and its
/// `Service.Container.Pos` is the key's span.
pub type Services<'a> = Keyed<'a, Container<'a>>;

/// The call of a reusable workflow in a job.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorkflowCall<'a> {
    /// `uses`. An empty value or one that is not a scalar is invalid, and
    /// the reference does not classify it either.
    pub uses: Field<'a, JobUses<'a>>,
    /// `with`, keyed by input name. An input's value is `None` when it is
    /// not a scalar, which the parser reports. The reference's
    /// `WorkflowCallInput.Name` is the entry's key.
    pub inputs: Field<'a, Keyed<'a, Option<Template<'a>>>>,
    /// `secrets`.
    pub secrets: Field<'a, WorkflowCallSecrets<'a>>,
}

/// `secrets` of a job that calls a reusable workflow.
#[derive(Debug, Clone, PartialEq)]
pub enum WorkflowCallSecrets<'a> {
    /// `secrets: inherit`, the reference's `InheritSecrets`.
    Inherit(Str<'a>),
    /// The secrets keyed by name. A secret's value is `None` when it is not
    /// a scalar, which the parser reports. The reference's
    /// `WorkflowCallSecret.Name` is the entry's key.
    Map(Keyed<'a, Option<Template<'a>>>),
}

/// `snapshot` of a job.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot<'a> {
    /// `image-name`. In the string form the whole value is the image name
    /// and the field has no key.
    pub image_name: Field<'a, Str<'a>>,
    /// `version`.
    pub version: Field<'a, Str<'a>>,
    /// `if`.
    pub condition: Field<'a, Template<'a>>,
}
