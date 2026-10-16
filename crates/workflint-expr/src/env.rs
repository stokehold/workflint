//! The types of the contexts that an expression can read.

use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock};

use workflint_data::{Availability, GLOBAL_CONTEXTS, UntrustedInput, workflow_key};

use crate::types::{ObjectTy, Rest, Ty};

/// The global contexts converted to [`Ty`] once, sorted by name.
static GLOBALS: LazyLock<Vec<(&'static str, Ty)>> = LazyLock::new(|| {
    GLOBAL_CONTEXTS
        .iter()
        .map(|context| (context.name, Ty::from(&context.ty)))
        .collect()
});

/// The secrets that every workflow can read, besides the ones it declares.
const BUILT_IN_SECRETS: [&str; 3] = ["actions_runner_debug", "actions_step_debug", "github_token"];

/// The types of the contexts at one place in a workflow, which [`check`](Self::check) checks
/// expressions against.
///
/// A new environment has the global contexts of `workflint-data`. The setters replace the
/// contexts that depend on the workflow, the job, or the step, so a caller builds one
/// environment for the workflow, clones it for each job and step, and changes the clone. A
/// clone is cheap, because types share their parts. Checking an expression changes nothing
/// in the environment, so one environment can check many expressions.
///
/// This replaces the reference's `Update*` methods, which change the checker's shared map of
/// contexts.
///
/// ```
/// use workflint_expr::{ObjectTy, Rest, Ty, TypeEnv, parse_expression};
///
/// let mut env = TypeEnv::new();
/// env.set_matrix(ObjectTy::new([("os", Ty::String)], Rest::Strict));
///
/// let (expr, _) = parse_expression("matrix.os }}").unwrap();
/// let (ty, errors) = env.check(&expr);
/// assert_eq!(ty, Ty::String);
/// assert!(errors.is_empty());
///
/// let (expr, _) = parse_expression("matrix.arch }}").unwrap();
/// let (_, errors) = env.check(&expr);
/// assert_eq!(errors[0].message, r#"property "arch" is not defined in object type {os: string}"#);
/// ```
#[derive(Debug, Clone, Default)]
pub struct TypeEnv {
    matrix: Option<Ty>,
    steps: Option<Ty>,
    needs: Option<Ty>,
    secrets: Option<Ty>,
    inputs: Option<Ty>,
    jobs: Option<Ty>,
    /// The type of `github.event.inputs`. `github` itself is never copied.
    event_inputs: Option<Ty>,
    /// The configured variable names. `None` turns the check of `vars.NAME` against the list
    /// off.
    config_vars: Option<Arc<[String]>>,
    /// The contexts and special functions that expressions may use. `None` allows all of them.
    availability: Option<Availability>,
    /// The untrusted input paths that [`check`](Self::check) reports. `None` turns the check
    /// off.
    untrusted_inputs: Option<&'static [UntrustedInput]>,
}

impl TypeEnv {
    /// An environment with only the global contexts.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the type of `matrix`. This is the reference's `UpdateMatrix`.
    pub fn set_matrix(&mut self, ty: impl Into<Arc<ObjectTy>>) {
        self.matrix = Some(Ty::Object(ty.into()));
    }

    /// Sets the type of `steps`. This is the reference's `UpdateSteps`.
    pub fn set_steps(&mut self, ty: impl Into<Arc<ObjectTy>>) {
        self.steps = Some(Ty::Object(ty.into()));
    }

    /// Sets the type of `needs`. This is the reference's `UpdateNeeds`.
    pub fn set_needs(&mut self, ty: impl Into<Arc<ObjectTy>>) {
        self.needs = Some(Ty::Object(ty.into()));
    }

    /// Sets the type of `jobs`, which exists only where the environment sets it. This is the
    /// reference's `UpdateJobs`.
    pub fn set_jobs(&mut self, ty: impl Into<Arc<ObjectTy>>) {
        self.jobs = Some(Ty::Object(ty.into()));
    }

    /// Sets `secrets` to a strict object of the declared secrets and the three that every
    /// workflow can read: `github_token`, `actions_step_debug`, and `actions_runner_debug`. A
    /// declared secret replaces a built-in one of the same name. This is the reference's
    /// `UpdateSecrets`.
    pub fn set_secrets(&mut self, declared: impl Into<Arc<ObjectTy>>) {
        let declared = declared.into();
        let mut props: BTreeMap<String, Ty> = BUILT_IN_SECRETS
            .iter()
            .map(|name| ((*name).to_owned(), Ty::String))
            .collect();
        for (name, ty) in declared.props() {
            props.insert(name.clone(), ty.clone());
        }
        self.secrets = Some(ObjectTy::new(props, Rest::Strict).into());
    }

    /// Adds the inputs of a trigger to `inputs`. The first call replaces the empty default.
    /// Later calls merge, so a workflow with `workflow_call` and `workflow_dispatch` reads the
    /// inputs of both. This is the reference's `UpdateInputs`.
    pub fn add_inputs(&mut self, ty: impl Into<Arc<ObjectTy>>) {
        let ty = Ty::Object(ty.into());
        let merged = match &self.inputs {
            Some(current) if !is_empty_strict(current) => current.merge(&ty),
            _ => ty,
        };
        self.inputs = Some(merged);
    }

    /// Adds the inputs of `workflow_dispatch`: to `inputs` as [`add_inputs`](Self::add_inputs)
    /// does, and to `github.event.inputs` as a strict object whose properties are all strings,
    /// as GitHub passes them in the event. This is the reference's `UpdateDispatchInputs`.
    pub fn add_dispatch_inputs(&mut self, ty: impl Into<Arc<ObjectTy>>) {
        let ty = ty.into();
        let event_inputs = ty.props().keys().map(|name| (name.clone(), Ty::String));
        self.event_inputs = Some(Ty::strict(event_inputs));
        self.add_inputs(ty);
    }

    /// Sets the names of the configuration variables that `vars.NAME` may read. Names are
    /// compared without regard to case. Without a call, any name is allowed.
    pub fn set_config_vars<S: Into<String>>(&mut self, names: impl IntoIterator<Item = S>) {
        self.config_vars = Some(names.into_iter().map(Into::into).collect());
    }

    /// Allows the contexts and special functions of the workflow key `key`, such as
    /// `jobs.<job_id>.steps.if`. A key that `workflint-data` does not list, or an empty key,
    /// allows no context and no special function, as in the reference. Without a call to this
    /// or [`set_availability`](Self::set_availability), every context and special function is
    /// allowed.
    ///
    /// ```
    /// use workflint_expr::{TypeEnv, parse_expression};
    ///
    /// let mut env = TypeEnv::new();
    /// env.set_workflow_key("jobs.<job_id>.if");
    ///
    /// let (expr, _) = parse_expression("success() && github.ref == 'main' }}").unwrap();
    /// assert!(env.check(&expr).1.is_empty());
    ///
    /// let (expr, _) = parse_expression("hashFiles('*.lock') }}").unwrap();
    /// let (_, errors) = env.check(&expr);
    /// assert!(errors[0].message.starts_with(r#"calling function "hashFiles" is not allowed here."#));
    /// ```
    pub fn set_workflow_key(&mut self, key: &str) {
        let availability = workflow_key(key).copied().unwrap_or(Availability::NONE);
        self.set_availability(availability);
    }

    /// Allows only the contexts and special functions in `availability`. Messages list the
    /// allowed contexts in its order. This is the reference's `SetContextAvailability` and
    /// `SetSpecialFunctionAvailability`.
    pub fn set_availability(&mut self, availability: Availability) {
        self.availability = Some(availability);
    }

    /// Returns the contexts and special functions that expressions may use, or `None` if all
    /// of them are allowed.
    pub(crate) fn availability(&self) -> Option<&Availability> {
        self.availability.as_ref()
    }

    /// Turns on the check for untrusted input with the paths in `roots`, as for an expression in
    /// a script. [`check`](Self::check) then reports each untrusted value that the expression
    /// reads, as [`find_untrusted_inputs`](crate::find_untrusted_inputs) does, after the
    /// semantic errors. The check is off in a new environment.
    ///
    /// `roots` is usually [`workflint_data::UNTRUSTED_INPUTS`]. This is the reference's
    /// `NewExprSemanticsChecker` with `checkUntrustedInput` set, plus the custom roots of
    /// `NewUntrustedInputChecker`.
    ///
    /// ```
    /// use workflint_data::UNTRUSTED_INPUTS;
    /// use workflint_expr::{TypeEnv, parse_expression};
    ///
    /// let mut env = TypeEnv::new();
    /// env.set_untrusted_inputs(UNTRUSTED_INPUTS);
    /// let (expr, _) = parse_expression("github.head_ref }}").unwrap();
    /// let (_, errors) = env.check(&expr);
    /// assert_eq!(errors[0].rule.id, "untrusted-input");
    /// ```
    pub fn set_untrusted_inputs(&mut self, roots: &'static [UntrustedInput]) {
        self.untrusted_inputs = Some(roots);
    }

    /// Returns the untrusted input paths, if the check is on.
    pub(crate) fn untrusted_inputs(&self) -> Option<&'static [UntrustedInput]> {
        self.untrusted_inputs
    }

    /// Returns the type of the context `name`, which must be lower case.
    pub(crate) fn variable(&self, name: &str) -> Option<&Ty> {
        let local = match name {
            "matrix" => &self.matrix,
            "steps" => &self.steps,
            "needs" => &self.needs,
            "secrets" => &self.secrets,
            "inputs" => &self.inputs,
            "jobs" => return self.jobs.as_ref(),
            _ => &None,
        };
        local.as_ref().or_else(|| global(name))
    }

    /// Returns the names of the contexts, for the message about an undefined variable.
    pub(crate) fn variable_names(&self) -> impl Iterator<Item = &'static str> {
        let jobs = self.jobs.as_ref().map(|_| "jobs");
        GLOBALS.iter().map(|(name, _)| *name).chain(jobs)
    }

    /// Returns the type of `github.event.inputs`, if `workflow_dispatch` set it.
    pub(crate) fn event_inputs(&self) -> Option<&Ty> {
        self.event_inputs.as_ref()
    }

    /// Returns the configured variable names, if the configuration lists them.
    pub(crate) fn config_vars(&self) -> Option<&[String]> {
        self.config_vars.as_deref()
    }
}

fn global(name: &str) -> Option<&'static Ty> {
    let index = GLOBALS
        .binary_search_by(|(other, _)| (*other).cmp(name))
        .ok()?;
    GLOBALS.get(index).map(|(_, ty)| ty)
}

fn is_empty_strict(ty: &Ty) -> bool {
    matches!(ty, Ty::Object(object) if object.props().is_empty() && object.is_strict())
}
