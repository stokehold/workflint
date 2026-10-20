//! Steps.

use workflint_diagnostics::Span;

use crate::{Env, Field, Keyed, Rare, StepUses, Template, Value};

/// One step of `steps`.
#[derive(Debug, Clone, PartialEq)]
pub struct Step<'a> {
    /// The span of the step's mapping, the reference's `Step.Pos`.
    pub span: Span,
    /// `id`. Its placeholders are parsed only if it
    /// [contains an expression](crate::contains_expression), as in the
    /// reference.
    pub id: Rare<Field<'a, Template<'a>>>,
    /// `if`.
    pub condition: Rare<Field<'a, Template<'a>>>,
    /// `name`. It may be empty.
    pub name: Field<'a, Template<'a>>,
    /// What the step runs, or `None` when it has neither `run` nor `uses`.
    pub exec: Option<Exec<'a>>,
    /// `env`.
    pub env: Rare<Value<'a, Env<'a>>>,
    /// `continue-on-error`.
    pub continue_on_error: Rare<Value<'a, bool>>,
    /// `timeout-minutes`.
    pub timeout_minutes: Rare<Value<'a, f64>>,
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
    pub run: Field<'a, Template<'a>>,
    /// `shell`.
    pub shell: Rare<Field<'a, Template<'a>>>,
    /// `working-directory`.
    pub working_directory: Rare<Field<'a, Template<'a>>>,
}

/// A step that runs an action.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExecAction<'a> {
    /// `uses`. An empty value or one that is not a scalar is invalid. The
    /// reference reads both as an empty string and classifies it as an
    /// action whose ref is missing.
    pub uses: Field<'a, StepUses<'a>>,
    /// `with`, keyed by input name. An input's value is `None` when it is
    /// not a scalar, which the parser reports. The reference's `Input.Name`
    /// is the entry's key.
    ///
    /// For a `docker://` action, `entrypoint` and `args` are moved out of
    /// it into the fields below, as in the reference.
    pub inputs: Field<'a, Keyed<'a, Option<Template<'a>>>>,
    /// `with.entrypoint` of a `docker://` action.
    pub entrypoint: Rare<Field<'a, Template<'a>>>,
    /// `with.args` of a `docker://` action.
    pub args: Rare<Field<'a, Template<'a>>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // A large workflow has hundreds of steps, and most of their keys are
    // absent, so those keys are `Rare` (#240).
    #[test]
    fn a_step_fits_in_360_bytes() {
        assert!(size_of::<Step<'_>>() <= 360, "{}", size_of::<Step<'_>>());
    }
}
