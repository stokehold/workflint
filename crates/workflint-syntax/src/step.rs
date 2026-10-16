//! Steps.

use workflint_diagnostics::Span;

use crate::{Env, Field, Keyed, Str, Value};

/// One step of `steps`.
#[derive(Debug, Clone, PartialEq)]
pub struct Step<'a> {
    /// The span of the step's mapping, the reference's `Step.Pos`.
    pub span: Span,
    /// `id`.
    pub id: Field<'a, Str<'a>>,
    /// `if`.
    pub condition: Field<'a, Str<'a>>,
    /// `name`. It may be empty.
    pub name: Field<'a, Str<'a>>,
    /// What the step runs, or `None` when it has neither `run` nor `uses`.
    pub exec: Option<Exec<'a>>,
    /// `env`.
    pub env: Value<'a, Env<'a>>,
    /// `continue-on-error`.
    pub continue_on_error: Value<'a, bool>,
    /// `timeout-minutes`.
    pub timeout_minutes: Value<'a, f64>,
}

/// What a step runs: a script or an action.
#[derive(Debug, Clone, PartialEq)]
pub enum Exec<'a> {
    /// A `run` script.
    Run(ExecRun<'a>),
    /// A `uses` action.
    Action(ExecAction<'a>),
}

/// A step that runs a script.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExecRun<'a> {
    /// `run`. The key's span is the reference's `RunPos`.
    pub run: Field<'a, Str<'a>>,
    /// `shell`.
    pub shell: Field<'a, Str<'a>>,
    /// `working-directory`.
    pub working_directory: Field<'a, Str<'a>>,
}

/// A step that runs an action.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExecAction<'a> {
    /// `uses`.
    // TODO(#72): replace with the typed `uses` value.
    pub uses: Field<'a, Str<'a>>,
    /// `with`, keyed by input name. An input's value is `None` when it is
    /// not a scalar, which the parser reports. The reference's `Input.Name`
    /// is the entry's key.
    ///
    /// For a `docker://` action, `entrypoint` and `args` are moved out of
    /// it into the fields below, as in the reference.
    pub inputs: Field<'a, Keyed<'a, Option<Str<'a>>>>,
    /// `with.entrypoint` of a `docker://` action.
    pub entrypoint: Field<'a, Str<'a>>,
    /// `with.args` of a `docker://` action.
    pub args: Field<'a, Str<'a>>,
}
