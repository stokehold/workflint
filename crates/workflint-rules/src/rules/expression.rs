//! The `expression` rule: type-checks each `${{ }}` at its location in the workflow.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `RuleExpression` in `rule_expression.go`.
//! Each location has a workflow key, which decides the contexts and special functions that its
//! expressions may use, and some locations need a type, such as an object for a whole `env:`
//! expression (`docs/research/02-expressions.md` section 7.2).
//!
//! The parser has parsed every placeholder and reported its parse errors, so the rule skips a
//! placeholder that holds a parse error, and a value that the parser marked invalid. It checks
//! every placeholder of a string, where the reference stops at the first one with an error
//! (`docs/differences.md#all-expressions-in-string`), and reports each error at its exact
//! position in the scalar (`docs/differences.md#positions-inside-scalars`).

use std::cell::OnceCell;
use std::collections::BTreeMap;
use std::sync::Arc;

use workflint_data::{UNTRUSTED_INPUTS, popular_action};
use workflint_diagnostics::quote::quote;
use workflint_diagnostics::{Category, RuleMeta, Severity};
use workflint_expr::{MAX_TYPE_DEPTH, ObjectTy, Rest, Ty, TypeEnv};
use workflint_syntax::{
    Concurrency, Container, Credentials, Defaults, Env, Event, Exec, Expr, Field, Job, JobUses,
    JobUsesKind, Keyed, KeyedEntry, Matrix, MatrixCombination, MatrixRow, Placeholder, RawValue,
    ReusableWorkflowMetadata, Runner, Snapshot, Step, StepUses, Str, StrSpans, Template, Value,
    Workflow, WorkflowCall, WorkflowCallEvent, WorkflowCallInputType, WorkflowCallSecrets,
    WorkflowDispatchEvent, WorkflowDispatchInputType, contains_expression,
};
use workflint_yaml::{Tag, simple_lowercase};

use crate::local::{Lookup, is_local_action};
use crate::rules::workflow_call::report_lookup_error;
use crate::{Context, Rule};

/// A template evaluates to an object, an array, or null, which GitHub cannot put in a string.
pub(crate) static OBJECT_IN_TEMPLATE: RuleMeta = RuleMeta {
    id: "object-in-template",
    description: "A `${{ }}` in a string evaluates to an object, an array, or null",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/object-in-template.md",
};

/// A value written as one `${{ }}` has a type that its key does not accept, such as a string
/// for `continue-on-error`.
pub(crate) static WRONG_EXPRESSION_TYPE: RuleMeta = RuleMeta {
    id: "wrong-expression-type",
    description: "A value written as one `${{ }}` has a type that its key does not accept",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/wrong-expression-type.md",
};

/// A value that must be one `${{ }}` holds another number of them. The parser accepts such a
/// value only with one, so the rule never reports it.
pub(crate) static SINGLE_EXPRESSION_REQUIRED: RuleMeta = RuleMeta {
    id: "single-expression-required",
    description: "A value that must be one `${{ }}` holds another number of them",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/single-expression-required.md",
};

/// A `boolean` or `number` input of `workflow_call` has a default written as one `${{ }}` of
/// another type.
pub(crate) static WRONG_INPUT_DEFAULT_TYPE: RuleMeta = RuleMeta {
    id: "wrong-input-default-type",
    description: "A `boolean` or `number` input of `workflow_call` has a default of another type",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/wrong-input-default-type.md",
};

/// A value in `with:` of a call to a local reusable workflow has a type that the callee's
/// input does not accept.
pub(crate) static WRONG_WORKFLOW_CALL_INPUT_TYPE: RuleMeta = RuleMeta {
    id: "wrong-workflow-call-input-type",
    description: "A `with:` value has a type that the input of the local reusable workflow does not accept",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/wrong-workflow-call-input-type.md",
};

/// The `expression` rule.
#[derive(Debug, Default)]
pub(crate) struct Expression<'a> {
    /// The jobs of the workflow, which `needs` reads its dependencies from.
    jobs: Option<&'a Keyed<'a, Job<'a>>>,
    /// The contexts of the workflow, which every job starts from.
    workflow: TypeEnv,
    /// The contexts of the job being visited, from its job-pre hook to its job-post hook.
    job: Option<TypeEnv>,
}

impl Expression<'_> {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Returns the contexts of the job being visited, or of the workflow outside a job.
    fn env(&self) -> &TypeEnv {
        self.job.as_ref().unwrap_or(&self.workflow)
    }

    /// Returns a checker for the location with the workflow key `key`.
    fn at<'e>(&'e self, key: &'e str) -> Checker<'e> {
        Checker::new(self.env(), key)
    }
}

impl<'a> Rule<'a> for Expression<'a> {
    fn workflow_pre(&mut self, workflow: &'a Workflow<'a>, cx: &mut Context<'_>) {
        // The events add their inputs to the contexts of the workflow, in the order of `on`.
        self.workflow = TypeEnv::new();
        self.jobs = workflow.jobs.value();
        // `name` and `defaults` see the contexts from before the events add theirs.
        let before_events = TypeEnv::new();
        let none = Checker::new(&before_events, "");
        none.string(cx, &workflow.name);
        for event in workflow
            .on
            .value()
            .into_iter()
            .flat_map(|events| events.iter().flatten())
        {
            self.event(event, cx);
        }
        if let Some(secrets) = declared_secrets(workflow) {
            self.workflow.set_secrets(secrets);
        }
        self.at("run-name").string(cx, &workflow.run_name);
        self.env_vars(cx, &workflow.env, "env");
        defaults(cx, &none, &workflow.defaults);
        concurrency(cx, &self.at("concurrency"), &workflow.concurrency);
    }

    fn job_pre(&mut self, entry: &'a KeyedEntry<'a, Job<'a>>, cx: &mut Context<'_>) {
        let job = &entry.value;
        // `needs` is typed before `matrix`, because the matrix may read `needs`. The type of
        // `matrix` is set before the other keys are checked, because they all see it.
        let mut env = self.workflow.clone();
        env.set_needs(self.needs_ty(entry, cx));
        // `steps` starts as the empty strict object of the global contexts, and each step with
        // an `id` adds itself once its own keys are checked.
        self.job = Some(env);
        let strategy = job.strategy.value();
        if let Some(strategy) = strategy {
            let mut matrix = self
                .at("jobs.<job_id>.strategy")
                .matrix(cx, &strategy.matrix);
            // An `${{ insert }}` expression in `strategy` may hold the whole matrix.
            if !strategy.inserts.is_empty() && !strategy.matrix.is_present() {
                matrix = Some((*loose()).clone());
            }
            if let (Some(matrix), Some(env)) = (matrix, &mut self.job) {
                env.set_matrix(matrix);
            }
        }

        self.at("jobs.<job_id>.name").string(cx, &job.name);
        self.at("").strings(cx, &job.needs);
        if let Some(Runner {
            labels,
            group,
            inserts,
        }) = job.runs_on.value()
        {
            let runs_on = self.at("jobs.<job_id>.runs-on");
            match labels {
                Value::Expression { expr, .. } => runs_on.runs_on(cx, expr),
                Value::Literal { value, .. } => {
                    for label in value.iter().flatten() {
                        runs_on.template(cx, label);
                    }
                }
                Value::Absent | Value::Invalid { .. } => {}
            }
            runs_on.string(cx, group);
            for expr in inserts.iter() {
                runs_on.object(cx, expr, "runs-on");
            }
        }
        concurrency(cx, &self.at("jobs.<job_id>.concurrency"), &job.concurrency);
        self.env_vars(cx, &job.env, "jobs.<job_id>.env");
        defaults(cx, &self.at("jobs.<job_id>.defaults.run"), &job.defaults);
        self.at("jobs.<job_id>.if").condition(cx, &job.condition);
        if let Some(strategy) = strategy {
            let strategy_env = self.at("jobs.<job_id>.strategy");
            strategy_env.bool(cx, &strategy.fail_fast);
            strategy_env.number(cx, &strategy.max_parallel, "integer value");
            for expr in strategy.inserts.iter() {
                strategy_env.object(cx, expr, "strategy");
            }
        }
        self.at("jobs.<job_id>.continue-on-error")
            .bool(cx, &job.continue_on_error);
        self.at("jobs.<job_id>.timeout-minutes").number(
            cx,
            &job.timeout_minutes,
            "float number value",
        );
        if let Some(container) = job.container.value() {
            self.container(
                cx,
                container,
                "container",
                "jobs.<job_id>.container",
                "jobs.<job_id>.container",
            );
        }
        let services = self.at("jobs.<job_id>.services");
        if let Value::Expression { expr, .. } = &job.services {
            services.object(cx, expr, "services");
        }
        for expr in job.services.literal().into_iter().flat_map(Keyed::inserts) {
            services.object(cx, expr, "services");
        }
        for service in job.services.literal().into_iter().flatten() {
            self.container(
                cx,
                &service.value,
                "services",
                "jobs.<job_id>.services",
                "jobs.<job_id>.services.<service_id>",
            );
        }
        if let Some(call) = &job.workflow_call {
            self.workflow_call(cx, call);
        }
        if let Some(Snapshot { condition, .. }) = job.snapshot.value() {
            self.at("jobs.<job_id>.snapshot.if")
                .condition(cx, condition);
        }
    }

    fn step(&mut self, step: &'a Step<'a>, cx: &mut Context<'_>) {
        self.at("jobs.<job_id>.steps.name").string(cx, &step.name);
        self.at("jobs.<job_id>.steps.if")
            .condition(cx, &step.condition);
        match &step.exec {
            Some(Exec::Run(run)) => {
                self.at("jobs.<job_id>.steps.run").script(cx, &run.run);
                self.at("").string(cx, &run.shell);
                self.at("jobs.<job_id>.steps.working-directory")
                    .string(cx, &run.working_directory);
            }
            Some(Exec::Action(action)) => {
                let uses = action.uses.value();
                if let Some(uses) = uses {
                    self.at("").template(cx, &uses.template);
                }
                let with = self.at("jobs.<job_id>.steps.with");
                let github_script =
                    uses.is_some_and(|uses| uses.value().starts_with("actions/github-script@"));
                for input in action.inputs.value().into_iter().flatten() {
                    let Some(value) = &input.value else {
                        continue;
                    };
                    if github_script && input.folded() == "script" {
                        with.script_template(cx, value);
                    } else {
                        with.template(cx, value);
                    }
                }
                for expr in action.inputs.value().into_iter().flat_map(Keyed::inserts) {
                    with.object(cx, expr, "with");
                }
                with.string(cx, &action.entrypoint);
                with.string(cx, &action.args);
            }
            None => {}
        }
        self.env_vars(cx, &step.env, "jobs.<job_id>.steps.env");
        self.at("jobs.<job_id>.steps.continue-on-error")
            .bool(cx, &step.continue_on_error);
        self.at("jobs.<job_id>.steps.timeout-minutes").number(
            cx,
            &step.timeout_minutes,
            "float number value",
        );
        // An `id` is checked only if it contains an expression: the parser stores no
        // placeholder otherwise.
        self.at("").string(cx, &step.id);
        self.add_step(step, cx);
    }

    fn job_post(&mut self, entry: &'a KeyedEntry<'a, Job<'a>>, cx: &mut Context<'_>) {
        let job = &entry.value;
        // GitHub evaluates `environment` and `outputs` after the steps.
        if let Some(environment) = job.environment.value() {
            let env = self.at("jobs.<job_id>.environment");
            env.string(cx, &environment.name);
            self.at("jobs.<job_id>.environment.url")
                .string(cx, &environment.url);
            env.bool(cx, &environment.deployment);
            for expr in environment.inserts.iter() {
                env.object(cx, expr, "environment");
            }
        }
        let outputs = self.at("jobs.<job_id>.outputs.<output_id>");
        for output in job.outputs.value().into_iter().flatten() {
            if let Some(value) = &output.value {
                outputs.template(cx, value);
            }
        }
        self.job = None;
    }

    fn workflow_post(&mut self, workflow: &'a Workflow<'a>, cx: &mut Context<'_>) {
        let Some(call) = workflow.workflow_call_event() else {
            return;
        };
        let has_jobs = workflow.jobs.value().is_some_and(|jobs| !jobs.is_empty());
        let Some(outputs) = call.outputs.value().filter(|o| has_jobs && !o.is_empty()) else {
            return;
        };
        let mut env = self.workflow.clone();
        env.set_jobs(jobs_ty(workflow.jobs.value().into_iter().flatten()));
        let checker = Checker::new(&env, "on.workflow_call.outputs.<output_id>.value");
        for output in outputs {
            checker.string(cx, &output.value.value);
        }
    }
}

impl Expression<'_> {
    /// Returns the type of `needs` in the job `entry`, the reference's `calcNeedsType`: each job
    /// that `entry` lists in `needs` with its `outputs` and its `result`. The jobs that those
    /// jobs need are left out, as on GitHub (rhysd/actionlint#151). A job that calls a
    /// reusable workflow has the outputs of the callee.
    fn needs_ty(&self, entry: &KeyedEntry<'_, Job<'_>>, cx: &mut Context<'_>) -> ObjectTy {
        let mut needs = BTreeMap::new();
        let ids = entry
            .value
            .needs
            .value()
            .into_iter()
            .flat_map(|ids| ids.iter().flatten());
        for id in ids {
            let id = simple_lowercase(id.value());
            // A job that needs itself is a cycle, which the `job-needs` rule reports.
            if id == entry.folded() || needs.contains_key(id.as_ref()) {
                continue;
            }
            let Some(needed) = self.jobs.and_then(|jobs| jobs.get(&id)) else {
                continue;
            };
            let job = &needed.value;
            let outputs = if let Some(call) = &job.workflow_call {
                workflow_call_outputs(call, cx)
            } else {
                let outputs = job.outputs.value().into_iter().flatten();
                Arc::new(ObjectTy::new(
                    outputs.map(|output| (output.folded().to_owned(), Ty::String)),
                    Rest::Strict,
                ))
            };
            let ty = Ty::strict([("outputs", Ty::Object(outputs)), ("result", Ty::String)]);
            needs.insert(id.into_owned(), ty);
        }
        ObjectTy::new(needs, Rest::Strict)
    }

    /// Adds `step` to `steps` if it has an `id`, so that the steps after it and the keys of the
    /// job-post hook see it. A step does not see itself. An `id` that contains an expression
    /// makes `steps` accept any step, because the value of the `id` is not known.
    fn add_step(&mut self, step: &Step<'_>, cx: &Context<'_>) {
        let Some(id) = step.id.value() else {
            return;
        };
        let uses = match &step.exec {
            Some(Exec::Action(action)) => action.uses.value(),
            Some(Exec::Run(_)) | None => None,
        };
        let ty = Ty::strict([
            ("outputs", Ty::Object(action_outputs(uses, cx))),
            ("conclusion", Ty::String),
            ("outcome", Ty::String),
        ]);
        if let Some(env) = &mut self.job {
            env.add_step(
                simple_lowercase(id.value()),
                ty,
                contains_expression(id.value()),
            );
        }
    }

    /// Checks the expressions of `event`, and adds its inputs to the contexts of the workflow.
    fn event(&mut self, event: &Event<'_>, cx: &mut Context<'_>) {
        // The whole event sees the contexts from before it adds its inputs.
        let before_event = self.workflow.clone();
        let none = Checker::new(&before_event, "");
        match event {
            Event::Webhook(webhook) => {
                for filter in [
                    &webhook.types,
                    &webhook.branches,
                    &webhook.branches_ignore,
                    &webhook.tags,
                    &webhook.tags_ignore,
                    &webhook.paths,
                    &webhook.paths_ignore,
                    &webhook.workflows,
                ] {
                    none.strings(cx, filter);
                }
            }
            Event::Schedule(schedule) => {
                for entry in schedule.schedules.iter() {
                    none.string(cx, &entry.cron);
                    none.string(cx, &entry.timezone);
                }
            }
            Event::WorkflowDispatch(dispatch) => {
                for input in dispatch.inputs.value().into_iter().flatten() {
                    none.string(cx, &input.value.description);
                    none.string(cx, &input.value.default);
                    none.bool(cx, &input.value.required);
                    none.strings(cx, &input.value.options);
                }
                self.workflow
                    .add_dispatch_inputs(dispatch_inputs_ty(dispatch));
            }
            Event::RepositoryDispatch(dispatch) => none.strings(cx, &dispatch.types),
            Event::WorkflowCall(call) => {
                let inputs = self.workflow_call_inputs(cx, call);
                self.workflow
                    .add_inputs(ObjectTy::new(inputs, Rest::Strict));
                for secret in call.secrets.value().into_iter().flatten() {
                    none.string(cx, &secret.value.description);
                }
                // The values of the outputs are checked in the workflow-post hook.
                for output in call.outputs.value().into_iter().flatten() {
                    none.string(cx, &output.value.description);
                }
            }
            Event::ImageVersion(image) => {
                none.strings(cx, &image.names);
                none.strings(cx, &image.versions);
            }
        }
    }

    /// Checks the description and the default of each input of `call`, and returns the types of
    /// the inputs, keyed by their lower-case names.
    ///
    /// A default sees only the inputs declared before it, so a default that reads its own input
    /// or a later one is reported. It also sees the inputs of a `workflow_dispatch` written
    /// before `workflow_call` under `on`, as in the reference.
    fn workflow_call_inputs(
        &self,
        cx: &mut Context<'_>,
        call: &WorkflowCallEvent<'_>,
    ) -> BTreeMap<String, Ty> {
        let none = self.at("");
        let mut inputs = BTreeMap::new();
        for input in call.inputs.value().into_iter().flatten() {
            none.string(cx, &input.value.description);
            let mut env = self.workflow.clone();
            env.add_inputs(ObjectTy::new(inputs.clone(), Rest::Strict));
            let default = input.value.default.value();
            let types = default
                .map(|default| {
                    Checker::new(&env, "on.workflow_call.inputs.<inputs_id>.default")
                        .template(cx, default)
                })
                .unwrap_or_default();
            let ty = match input.value.input_type.value() {
                Some(WorkflowCallInputType::String) => Ty::String,
                Some(WorkflowCallInputType::Boolean) => Ty::Bool,
                Some(WorkflowCallInputType::Number) => Ty::Number,
                None => Ty::Any,
            };
            if let (Some(default), [found]) = (default, types.as_slice())
                && matches!(ty, Ty::Bool | Ty::Number)
                && is_one_expression(default.value())
                && !matches!(found, Ty::Any)
                && *found != ty
            {
                let message = format!(
                    "type of input {} must be {ty} but found type {found}",
                    quote(input.key.value())
                );
                cx.report_in(
                    default.source,
                    &WRONG_INPUT_DEFAULT_TYPE,
                    default.span(),
                    message,
                );
            }
            inputs.insert(input.folded().to_owned(), ty);
        }
        inputs
    }

    /// Checks `env` of a workflow, job, step, or container with the key `key`: each name and
    /// value, or the whole expression, which must be an object.
    fn env_vars(&self, cx: &mut Context<'_>, env: &Value<'_, Env<'_>>, key: &str) {
        let checker = self.at(key);
        match env {
            Value::Expression { expr, .. } => checker.object(cx, expr, "env"),
            Value::Literal { value, .. } => {
                for var in value {
                    checker.template(cx, &var.value.name);
                    if let Some(value) = &var.value.value {
                        checker.template(cx, value);
                    }
                }
                for expr in value.inserts() {
                    checker.object(cx, expr, "env");
                }
            }
            Value::Absent | Value::Invalid { .. } => {}
        }
    }

    /// Checks a container or a service. `section` names the container's mapping in messages,
    /// `key` is the workflow key of the container's strings, and `child` the prefix of the keys
    /// of its `credentials` and `env`.
    fn container(
        &self,
        cx: &mut Context<'_>,
        container: &Container<'_>,
        section: &str,
        key: &str,
        child: &str,
    ) {
        let checker = self.at(key);
        checker.string(cx, &container.image);
        let credentials_key = format!("{child}.credentials");
        let credentials = self.at(&credentials_key);
        match &container.credentials {
            Value::Expression { expr, .. } => credentials.object(cx, expr, "credentials"),
            Value::Literal {
                value:
                    Credentials {
                        username,
                        password,
                        inserts,
                    },
                ..
            } => {
                credentials.string(cx, username);
                credentials.string(cx, password);
                for expr in inserts.iter() {
                    credentials.object(cx, expr, "credentials");
                }
            }
            Value::Absent | Value::Invalid { .. } => {}
        }
        self.env_vars(cx, &container.env, &format!("{child}.env.<env_id>"));
        checker.strings(cx, &container.ports);
        checker.strings(cx, &container.volumes);
        checker.string(cx, &container.options);
        checker.string(cx, &container.command);
        checker.string(cx, &container.entrypoint);
        for expr in container.inserts.iter() {
            checker.object(cx, expr, section);
        }
    }

    /// Checks the call of a reusable workflow, the reference's `checkWorkflowCall`. Each value
    /// of `with:` in a call to a local reusable workflow must have a type that the callee's
    /// input accepts.
    fn workflow_call(&self, cx: &mut Context<'_>, call: &WorkflowCall<'_>) {
        let uses = call.uses.value();
        if let Some(uses) = uses {
            self.at("").template(cx, &uses.template);
        }
        let callee = uses.and_then(|uses| local_callee(uses, cx).map(|callee| (uses, callee)));
        let with = self.at("jobs.<job_id>.with.<with_id>");
        for entry in call.inputs.value().into_iter().flatten() {
            let Some(value) = &entry.value else {
                continue;
            };
            let types = with.template(cx, value);
            let Some((uses, metadata)) = &callee else {
                continue;
            };
            let Some(input) = metadata.inputs.get(entry.folded()) else {
                continue;
            };
            let want = match input.input_type {
                Some(WorkflowCallInputType::String) => Ty::String,
                Some(WorkflowCallInputType::Boolean) => Ty::Bool,
                Some(WorkflowCallInputType::Number) => Ty::Number,
                None => continue,
            };
            let Some(found) = with_value_ty(value, &types) else {
                continue;
            };
            if !want.accepts(&found) {
                let message = format!(
                    "input {} is typed as {want} by reusable workflow {}. {found} value cannot be assigned",
                    quote(&input.name),
                    quote(uses.value()),
                );
                cx.report_in(
                    value.source,
                    &WRONG_WORKFLOW_CALL_INPUT_TYPE,
                    value.span(),
                    message,
                );
            }
        }
        if let Some(WorkflowCallSecrets::Map(secrets)) = call.secrets.value() {
            keyed(
                cx,
                &self.at("jobs.<job_id>.secrets.<secrets_id>"),
                Some(secrets),
            );
        }
    }
}

/// Looks up the local reusable workflow that `uses` calls, and reports the error of the lookup
/// if it is the first. Returns `None` for a call to a workflow in another repository, or one
/// with no metadata. Only a valid `./` path is looked up: the `workflow-call` rule reports an
/// invalid one.
fn local_callee(uses: &JobUses<'_>, cx: &mut Context<'_>) -> Option<Arc<ReusableWorkflowMetadata>> {
    if !matches!(uses.kind, JobUsesKind::Local { .. }) {
        return None;
    }
    match cx.local_metadata().reusable_workflow(uses.value()) {
        Lookup::Found { metadata, .. } => Some(metadata),
        Lookup::None => None,
        Lookup::Error(err) => {
            report_lookup_error(cx, uses, &err);
            None
        }
    }
}

/// Returns the type of `needs.<job_id>.outputs` for a job that runs `call`, the reference's
/// `getWorkflowCallOutputsType`: the outputs of a local callee, whose names are in lower case,
/// or outputs of any name, which are strings.
fn workflow_call_outputs(call: &WorkflowCall<'_>, cx: &mut Context<'_>) -> Arc<ObjectTy> {
    let callee = call.uses.value().and_then(|uses| local_callee(uses, cx));
    let outputs = match &callee {
        Some(metadata) => ObjectTy::new(
            metadata
                .outputs
                .keys()
                .map(|name| (name.clone(), Ty::String)),
            Rest::Strict,
        ),
        None => ObjectTy::new([] as [(&str, Ty); 0], Rest::Map(Ty::String)),
    };
    Arc::new(outputs)
}

/// Returns the type of the `with:` value `value`, whose placeholders without errors have the
/// types `types`, or `None` if a placeholder has errors.
///
/// A value written as one `${{ }}` has the type of the expression, and a value with text
/// around its placeholders is a string. A literal has the type of its YAML tag, so a quoted
/// value is a string, where the reference types the text
/// (`docs/differences.md#quoted-scalar-types`).
fn with_value_ty(value: &Template<'_>, types: &[Ty]) -> Option<Ty> {
    if types.len() != value.placeholders().len() {
        return None;
    }
    if let [ty] = types
        && is_one_expression(value.value())
    {
        return Some(ty.clone());
    }
    if !types.is_empty() {
        return Some(Ty::String);
    }
    Some(tag_ty(value.source.tag()))
}

/// Returns the type of a literal scalar with the tag `tag`.
fn tag_ty(tag: &Tag) -> Ty {
    match tag {
        Tag::Bool => Ty::Bool,
        Tag::Null => Ty::Null,
        Tag::Int | Tag::Float => Ty::Number,
        Tag::Str | Tag::Map | Tag::Seq | Tag::Other(_) => Ty::String,
    }
}

/// Returns the type of `steps.<step_id>.outputs` for a step that runs `uses`, or a script if
/// `uses` is `None`. This is the reference's `getActionOutputsType`.
///
/// The outputs of a local action come from its metadata, and those of a popular action from
/// `workflint-data`. Their names are in lower case. `actions/github-script` can set any output
/// with `core.setOutput`, and so can a popular action whose data skips its outputs. Any other
/// step has outputs of any name, which are strings.
///
/// Broken metadata of a local action gets outputs of any name, and this rule reports nothing.
/// The `action` rule runs before this one on each step, so it looks the action up first and
/// reports the error.
fn action_outputs(uses: Option<&StepUses<'_>>, cx: &Context<'_>) -> Arc<ObjectTy> {
    let any_string = || Arc::new(ObjectTy::new([] as [(&str, Ty); 0], Rest::Map(Ty::String)));
    let Some(spec) = uses.map(StepUses::value) else {
        return any_string();
    };
    if is_local_action(spec) {
        return match cx.local_metadata().action(spec) {
            Lookup::Found { metadata, .. } => Arc::new(ObjectTy::new(
                metadata
                    .metadata
                    .outputs
                    .keys()
                    .map(|name| (name.clone(), Ty::String)),
                Rest::Strict,
            )),
            Lookup::None | Lookup::Error(_) => any_string(),
        };
    }
    if spec.starts_with("actions/github-script@") {
        return loose();
    }
    match popular_action(spec) {
        Some(action) if action.skip_outputs => loose(),
        Some(action) => Arc::new(ObjectTy::new(
            action.outputs.iter().map(|output| (output.id, Ty::String)),
            Rest::Strict,
        )),
        None => any_string(),
    }
}

/// Returns the types of the inputs of `dispatch`, keyed by their lower-case names. An input
/// without a valid `type` has type `any`.
fn dispatch_inputs_ty(dispatch: &WorkflowDispatchEvent<'_>) -> ObjectTy {
    let inputs = dispatch.inputs.value().into_iter().flatten().map(|input| {
        let ty = match input.value.input_type.value() {
            Some(WorkflowDispatchInputType::Boolean) => Ty::Bool,
            Some(WorkflowDispatchInputType::Number) => Ty::Number,
            Some(
                WorkflowDispatchInputType::String
                | WorkflowDispatchInputType::Choice
                | WorkflowDispatchInputType::Environment,
            ) => Ty::String,
            None => Ty::Any,
        };
        (input.folded().to_owned(), ty)
    });
    ObjectTy::new(inputs, Rest::Strict)
}

/// Returns the secrets that `on.workflow_call.secrets` declares, if `workflow_call` is the only
/// trigger of `workflow` and the key is written. `secrets` then has only these and the
/// built-in secrets. Otherwise it accepts any name: a caller may pass every secret with
/// `secrets: inherit`, and another trigger runs the workflow with every repository secret
/// (`docs/differences.md#secrets-context-mixed-triggers`). A value that is not a mapping
/// declares no secret, as in the reference.
fn declared_secrets(workflow: &Workflow<'_>) -> Option<ObjectTy> {
    let events = workflow.on.value()?;
    let [Some(Event::WorkflowCall(call))] = &events[..] else {
        return None;
    };
    let secrets = match &call.secrets {
        Field::Absent => return None,
        Field::Invalid { .. } => Vec::new(),
        Field::Valid { value, .. } => value
            .iter()
            .map(|secret| (secret.folded().to_owned(), Ty::String))
            .collect(),
    };
    Some(ObjectTy::new(secrets, Rest::Strict))
}

/// Returns the type of `jobs` in `on.workflow_call.outputs.<output_id>.value`: each job with
/// its `outputs` and its `result`. The reference has no `result`
/// (`docs/differences.md#jobs-context-result`). A job that calls a reusable workflow has
/// outputs of any name, because its outputs are not in this file.
fn jobs_ty<'j, 'a: 'j>(jobs: impl Iterator<Item = &'j KeyedEntry<'a, Job<'a>>>) -> ObjectTy {
    let jobs = jobs.map(|entry| {
        let job = &entry.value;
        let outputs = if job.workflow_call.is_some() {
            loose()
        } else {
            let outputs = job.outputs.value().into_iter().flatten();
            Arc::new(ObjectTy::new(
                outputs.map(|output| (output.folded().to_owned(), Ty::String)),
                Rest::Strict,
            ))
        };
        let ty = Ty::strict([("outputs", Ty::Object(outputs)), ("result", Ty::String)]);
        (entry.folded().to_owned(), ty)
    });
    ObjectTy::new(jobs, Rest::Strict)
}

/// Returns whether `value` is one `${{ }}` with only whitespace around it, the reference's
/// `isExprAssigned`.
fn is_one_expression(value: &str) -> bool {
    let value = value.trim();
    value.starts_with("${{") && value.ends_with("}}") && value.matches("${{").count() == 1
}

/// Returns the properties of the object type `ty`, or none if it is not an object.
fn object_props(ty: &Ty) -> BTreeMap<String, Ty> {
    match ty {
        Ty::Object(object) => object.props().clone(),
        _ => BTreeMap::new(),
    }
}

/// Returns the object type `ty` with the properties `props`.
fn with_props(ty: &Ty, props: BTreeMap<String, Ty>) -> Ty {
    let rest = match ty {
        Ty::Object(object) => object.rest().clone(),
        _ => Rest::Strict,
    };
    ObjectTy::new(props, rest).into()
}

/// An object type with no known property, which accepts any property.
fn loose() -> Arc<ObjectTy> {
    Arc::new(ObjectTy::new([] as [(&str, Ty); 0], Rest::Loose))
}

fn defaults(cx: &mut Context<'_>, checker: &Checker, defaults: &Field<'_, Defaults<'_>>) {
    if let Some(run) = defaults.value().and_then(|defaults| defaults.run.value()) {
        checker.string(cx, &run.shell);
        checker.string(cx, &run.working_directory);
        for expr in run.inserts.iter() {
            checker.object(cx, expr, "run");
        }
    }
}

fn concurrency(cx: &mut Context<'_>, checker: &Checker, concurrency: &Field<'_, Concurrency<'_>>) {
    if let Some(concurrency) = concurrency.value() {
        checker.string(cx, &concurrency.group);
        checker.bool(cx, &concurrency.cancel_in_progress);
        for expr in concurrency.inserts.iter() {
            checker.object(cx, expr, "concurrency");
        }
    }
}

fn keyed(cx: &mut Context<'_>, checker: &Checker, keyed: Option<&Keyed<'_, Option<Template<'_>>>>) {
    for entry in keyed.into_iter().flatten() {
        if let Some(value) = &entry.value {
            checker.template(cx, value);
        }
    }
}

/// Checks the expressions of one location against the contexts that its workflow key allows.
///
/// Most locations hold no expression, so the checker builds the environment of its location,
/// a clone of `base` with the availability of `key`, when it checks the first one.
struct Checker<'e> {
    base: &'e TypeEnv,
    key: &'e str,
    env: OnceCell<TypeEnv>,
}

impl<'e> Checker<'e> {
    fn new(base: &'e TypeEnv, key: &'e str) -> Self {
        Self {
            base,
            key,
            env: OnceCell::new(),
        }
    }

    fn env(&self) -> &TypeEnv {
        self.env.get_or_init(|| {
            let mut env = self.base.clone();
            env.set_workflow_key(self.key);
            env
        })
    }

    /// Type-checks `placeholder` of the string of `spans` and reports its errors. Returns its
    /// type, or `None` if it holds a parse error, which the parser has reported, or has errors.
    fn placeholder(
        &self,
        cx: &mut Context<'_>,
        spans: &StrSpans<'_, '_>,
        placeholder: &Placeholder,
    ) -> Option<Ty> {
        let expr = placeholder.expr.as_ref().ok()?;
        let (ty, errors) = self.env().check(expr);
        if errors.is_empty() {
            return Some(ty);
        }
        for error in errors {
            let span = spans.span(placeholder.to_value(error.range()));
            cx.report_in_with_help(spans.source(), error.rule, span, error.message, error.help);
        }
        None
    }

    /// Checks each placeholder of `template`, and reports a placeholder that evaluates to an
    /// object, an array, or null, which GitHub cannot put in a string. Returns the types of the
    /// placeholders without errors.
    fn template(&self, cx: &mut Context<'_>, template: &Template<'_>) -> Vec<Ty> {
        let mut types = Vec::new();
        let spans = template.source.spans(cx.file());
        for placeholder in template.placeholders() {
            let Some(ty) = self.placeholder(cx, &spans, placeholder) else {
                continue;
            };
            if !placeholder.is_implicit() && matches!(ty, Ty::Object(_) | Ty::Array(_) | Ty::Null) {
                let start = usize::try_from(placeholder.start).unwrap_or(usize::MAX);
                let span = spans.span(start..start + "${{".len());
                cx.report_in(
                    template.source,
                    &OBJECT_IN_TEMPLATE,
                    span,
                    format!(
                        "object, array, and null values should not be evaluated in template with ${{{{ }}}} but evaluating the value of type {ty}"
                    ),
                );
            }
            types.push(ty);
        }
        types
    }

    fn string(&self, cx: &mut Context<'_>, field: &Field<'_, Template<'_>>) {
        if let Some(template) = field.value() {
            self.template(cx, template);
        }
    }

    fn strings(&self, cx: &mut Context<'_>, field: &Field<'_, Arc<[Option<Template<'_>>]>>) {
        for template in field.value().into_iter().flat_map(|t| t.iter().flatten()) {
            self.template(cx, template);
        }
    }

    /// Checks an `if` condition. The parser stored a condition written without `${{ }}` as one
    /// implicit placeholder, which is not a template. A condition may have any type
    /// (`docs/differences.md#bool-assignability`).
    fn condition(&self, cx: &mut Context<'_>, field: &Field<'_, Template<'_>>) {
        self.string(cx, field);
    }

    /// Checks a script, whose placeholders must not read untrusted input.
    fn script(&self, cx: &mut Context<'_>, field: &Field<'_, Template<'_>>) {
        if let Some(template) = field.value() {
            self.script_template(cx, template);
        }
    }

    fn script_template(&self, cx: &mut Context<'_>, template: &Template<'_>) {
        if template.placeholders().is_empty() {
            return;
        }
        let mut env = self.base.clone();
        env.set_untrusted_inputs(UNTRUSTED_INPUTS);
        Checker::new(&env, self.key).template(cx, template);
    }

    /// Checks a value written as one `${{ }}` and returns its type, or `None` if it has errors.
    fn one(&self, cx: &mut Context<'_>, expr: &Expr<'_>, what: &str) -> Option<Ty> {
        one_expression(
            cx,
            expr.source,
            std::slice::from_ref(expr.placeholder()),
            what,
        )?;
        self.placeholder(cx, &expr.source.spans(cx.file()), expr.placeholder())
    }

    fn bool(&self, cx: &mut Context<'_>, value: &Value<'_, bool>) {
        let Some(expr) = value.expression() else {
            return;
        };
        let Some(ty) = self.one(cx, expr, "bool value") else {
            return;
        };
        if !matches!(ty, Ty::Bool | Ty::Any) {
            let message = format!("type of expression must be bool but found type {ty}");
            cx.report_in(expr.source, &WRONG_EXPRESSION_TYPE, expr.span(), message);
        }
    }

    fn number<T>(&self, cx: &mut Context<'_>, value: &Value<'_, T>, what: &str) {
        let Some(expr) = value.expression() else {
            return;
        };
        let Some(ty) = self.one(cx, expr, what) else {
            return;
        };
        if !Want::Number.accepts(&ty) {
            wrong_type(cx, expr, what, Want::Number, &ty);
        }
    }

    fn object(&self, cx: &mut Context<'_>, expr: &Expr<'_>, what: &str) {
        self.typed(cx, expr, what, Want::Object);
    }

    /// Checks the expressions of a matrix and returns the type of `matrix`, the reference's
    /// `checkMatrix`, or `None` if the matrix is absent or invalid. A value written as one
    /// `${{ }}` must be an object for the whole matrix and for an element of `include` or
    /// `exclude`, an array for a row and for `include`, and an array of objects for `exclude`.
    ///
    /// Each row has the merged type of its values, and `include` adds the properties of its
    /// elements or merges them into the rows. An `${{ insert }}` expression in the matrix or in
    /// an element of `include` must be an object, and makes the matrix accept any property. `exclude` is checked but adds nothing. The
    /// reference does not check an element of `include` written as one `${{ }}`, or merge its
    /// type (`docs/differences.md#matrix-include-expressions`).
    fn matrix(&self, cx: &mut Context<'_>, matrix: &Value<'_, Matrix<'_>>) -> Option<ObjectTy> {
        let matrix = match matrix {
            Value::Expression { expr, .. } => return Some(self.matrix_expression(cx, expr)),
            Value::Literal { value, .. } => value,
            Value::Absent | Value::Invalid { .. } => return None,
        };
        match &matrix.exclude {
            Value::Expression { expr, .. } => {
                if let Some(Ty::Array(array)) = self.typed(cx, expr, "exclude", Want::Array)
                    && !Want::Object.accepts(array.elem())
                {
                    wrong_type(cx, expr, "exclude", Want::Object, array.elem());
                }
            }
            Value::Literal { value, .. } => {
                for combination in value.iter() {
                    match combination {
                        MatrixCombination::Expression(expr) => self.object(cx, expr, "exclude"),
                        MatrixCombination::Assigns { assigns, .. } => {
                            for assign in assigns {
                                self.raw_value(cx, &assign.value);
                            }
                            for expr in assigns.inserts() {
                                self.object(cx, expr, "exclude");
                            }
                        }
                        MatrixCombination::Invalid(_) => {}
                    }
                }
            }
            Value::Absent | Value::Invalid { .. } => {}
        }

        let rows = matrix
            .rows
            .iter()
            .map(|row| (row.folded().to_owned(), self.matrix_row(cx, &row.value)));
        let mut ty = Ty::strict(rows);
        if !matrix.rows.inserts().is_empty() {
            for expr in matrix.rows.inserts() {
                self.object(cx, expr, "matrix");
            }
            ty = Ty::loose(object_props(&ty));
        }
        match &matrix.include {
            Value::Expression { expr, .. } => {
                // The merge of an object with anything but an object is `any`.
                return Some(match self.one(cx, expr, "include") {
                    Some(Ty::Array(array)) => match ty.merge(array.elem()) {
                        Ty::Object(object) => (*object).clone(),
                        _ => ObjectTy::new([] as [(&str, Ty); 0], Rest::Loose),
                    },
                    _ => ObjectTy::new([] as [(&str, Ty); 0], Rest::Loose),
                });
            }
            Value::Literal { value, .. } => {
                for combination in value.iter() {
                    match combination {
                        MatrixCombination::Expression(expr) => {
                            let what = "matrix combination at element of include section";
                            let Some(element) = self.one(cx, expr, what) else {
                                continue;
                            };
                            if !Want::Object.accepts(&element) {
                                wrong_type(cx, expr, what, Want::Object, &element);
                            }
                            ty = match ty.merge(&element) {
                                Ty::Object(object) => Ty::Object(object),
                                _ => Ty::loose(object_props(&ty)),
                            };
                        }
                        MatrixCombination::Assigns { assigns, .. } => {
                            let mut props = object_props(&ty);
                            for assign in assigns {
                                let value = self.raw_value(cx, &assign.value);
                                let merged = match props.get(assign.folded()) {
                                    Some(row) => row.merge(&value),
                                    None => value,
                                };
                                props.insert(assign.folded().to_owned(), merged);
                            }
                            ty = if assigns.inserts().is_empty() {
                                with_props(&ty, props)
                            } else {
                                for expr in assigns.inserts() {
                                    self.object(cx, expr, "include");
                                }
                                Ty::loose(props)
                            };
                        }
                        MatrixCombination::Invalid(_) => {}
                    }
                }
            }
            Value::Absent | Value::Invalid { .. } => {}
        }
        match ty {
            Ty::Object(object) => Some((*object).clone()),
            _ => None,
        }
    }

    /// Checks a whole `matrix: ${{ }}` and returns the type of `matrix`, the reference's
    /// `checkMatrixExpression`: the object type of the expression with the properties of the
    /// elements of its `include` merged in, and without `include` and `exclude`. A value of
    /// another type, or with errors, gives an object with any property.
    fn matrix_expression(&self, cx: &mut Context<'_>, expr: &Expr<'_>) -> ObjectTy {
        let Some(Ty::Object(object)) = self.typed(cx, expr, "matrix", Want::Object) else {
            return ObjectTy::new([] as [(&str, Ty); 0], Rest::Loose);
        };
        let mut props = object.props().clone();
        if let Some(Ty::Array(include)) = props.remove("include")
            && let Ty::Object(element) = include.elem()
        {
            for (name, ty) in element.props() {
                let merged = match props.get(name) {
                    Some(row) => row.merge(ty),
                    None => ty.clone(),
                };
                props.insert(name.clone(), merged);
            }
        }
        props.remove("exclude");
        ObjectTy::new(props, object.rest().clone())
    }

    /// Checks a matrix row and returns the type of its values, the reference's
    /// `checkMatrixRow`. A row without values, or whose expression has errors, has type `any`.
    fn matrix_row(&self, cx: &mut Context<'_>, row: &MatrixRow<'_>) -> Ty {
        match row {
            MatrixRow::Values(values) => values
                .iter()
                .map(|value| self.raw_value(cx, value))
                .reduce(|merged, ty| merged.merge(&ty))
                .unwrap_or(Ty::Any),
            MatrixRow::Expression(expr) => match self.typed(cx, expr, "matrix row", Want::Array) {
                Some(Ty::Array(array)) => array.elem().clone(),
                _ => Ty::Any,
            },
            MatrixRow::Invalid => Ty::Any,
        }
    }

    /// Checks the scalars in a matrix value and returns its type, the reference's
    /// `checkRawYAMLValue`: a mapping is a strict object, and a sequence an array of the merged
    /// type of its items. A matrix value may nest as deep as the YAML layer allows, so the walk
    /// keeps a stack of its own, and an object or array nested deeper than [`MAX_TYPE_DEPTH`]
    /// has type `any`.
    fn raw_value(&self, cx: &mut Context<'_>, value: &RawValue<'_>) -> Ty {
        /// A step of the walk.
        enum Task<'v, 'a> {
            /// Type `value`, at the depth `depth`, and push its type.
            Visit(&'v RawValue<'a>, usize),
            /// Pop the types of the `len` items of a sequence and push the sequence's type.
            Sequence { len: usize, typed: bool },
            /// Pop the types of the values of `entries` and push the mapping's type.
            Mapping {
                entries: &'v Keyed<'a, RawValue<'a>>,
                typed: bool,
            },
        }

        let mut tasks = vec![Task::Visit(value, 0)];
        // The types of the values typed so far whose sequence or mapping is not yet typed.
        let mut done: Vec<Ty> = Vec::new();
        while let Some(task) = tasks.pop() {
            match task {
                Task::Visit(RawValue::Scalar(scalar), _) => done.push(self.raw_scalar(cx, scalar)),
                Task::Visit(RawValue::Sequence { items, .. }, depth) => {
                    let typed = depth < MAX_TYPE_DEPTH;
                    tasks.push(Task::Sequence {
                        len: items.len(),
                        typed,
                    });
                    tasks.extend(items.iter().rev().map(|item| Task::Visit(item, depth + 1)));
                }
                Task::Visit(RawValue::Mapping { entries, .. }, depth) => {
                    let typed = depth < MAX_TYPE_DEPTH;
                    tasks.push(Task::Mapping { entries, typed });
                    tasks.extend(
                        entries
                            .iter()
                            .rev()
                            .map(|entry| Task::Visit(&entry.value, depth + 1)),
                    );
                }
                Task::Sequence { len, typed } => {
                    let items = done.split_off(done.len().saturating_sub(len));
                    done.push(if typed {
                        let elem = items.into_iter().reduce(|merged, ty| merged.merge(&ty));
                        Ty::array(elem.unwrap_or(Ty::Any))
                    } else {
                        Ty::Any
                    });
                }
                Task::Mapping { entries, typed } => {
                    let values = done.split_off(done.len().saturating_sub(entries.len()));
                    done.push(if typed {
                        let names = entries.iter().map(|entry| entry.folded().to_owned());
                        Ty::strict(names.zip(values))
                    } else {
                        Ty::Any
                    });
                }
            }
        }
        done.pop().unwrap_or(Ty::Any)
    }

    /// Checks the placeholders of a scalar in a matrix value and returns its type, the
    /// reference's `checkRawYAMLString`. A scalar written as one `${{ }}` has the type of the
    /// expression, or `any` if it has errors. Any other scalar has the type of its tag, so a
    /// quoted or block scalar is a string, where the reference types its text
    /// (`docs/differences.md#quoted-scalar-types`). A scalar is not a template: its value
    /// becomes a matrix value, which may be an object.
    fn raw_scalar(&self, cx: &mut Context<'_>, scalar: &Template<'_>) -> Ty {
        let mut types = Vec::new();
        let mut ok = true;
        let spans = scalar.source.spans(cx.file());
        for placeholder in scalar.placeholders() {
            match self.placeholder(cx, &spans, placeholder) {
                Some(ty) => types.push(ty),
                None => ok = false,
            }
        }
        if is_one_expression(scalar.value()) {
            return match (ok, types.pop()) {
                (true, Some(ty)) if types.is_empty() => ty,
                _ => Ty::Any,
            };
        }
        tag_ty(scalar.source.tag())
    }

    /// Checks a value written as one `${{ }}` whose type must be `want`. Returns its type if
    /// it has that type or `any`.
    fn typed(&self, cx: &mut Context<'_>, expr: &Expr<'_>, what: &str, want: Want) -> Option<Ty> {
        let ty = self.one(cx, expr, what)?;
        if want.accepts(&ty) {
            Some(ty)
        } else {
            wrong_type(cx, expr, what, want, &ty);
            None
        }
    }

    /// Checks a whole `runs-on: ${{ }}`, which must be a string or an array. The reference
    /// quotes the type in this message only.
    fn runs_on(&self, cx: &mut Context<'_>, expr: &Expr<'_>) {
        let what = "runner label at \"runs-on\" section";
        let Some(ty) = self.one(cx, expr, what) else {
            return;
        };
        if !matches!(ty, Ty::String | Ty::Array(_) | Ty::Any) {
            let message = format!(
                "type of expression at \"runs-on\" must be string or array but found type {}",
                quote(&ty.to_string())
            );
            cx.report_in(expr.source, &WRONG_EXPRESSION_TYPE, expr.span(), message);
        }
    }
}

/// Reports a value written as one `${{ }}` that has `placeholders` but not exactly one, the
/// reference's `checkOneExpression`. Returns `None` after reporting it. The parser stores such
/// a value only with one placeholder, so this never reports in a linted workflow.
fn one_expression(
    cx: &mut Context<'_>,
    source: Str<'_>,
    placeholders: &[Placeholder],
    what: &str,
) -> Option<()> {
    if placeholders.len() == 1 {
        return Some(());
    }
    let message = format!(
        "one ${{{{ }}}} expression should be included in {} value but got {} expressions",
        quote(what),
        placeholders.len()
    );
    cx.report_in(source, &SINGLE_EXPRESSION_REQUIRED, source.span(), message);
    None
}

/// The type that a value written as one `${{ }}` must have.
#[derive(Debug, Clone, Copy)]
enum Want {
    Number,
    Object,
    Array,
}

impl Want {
    /// Returns whether a value of type `ty` has this type. `any` has every type.
    fn accepts(self, ty: &Ty) -> bool {
        match self {
            Self::Number => matches!(ty, Ty::Number | Ty::Any),
            Self::Object => matches!(ty, Ty::Object(_) | Ty::Any),
            Self::Array => matches!(ty, Ty::Array(_) | Ty::Any),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Number => "number",
            Self::Object => "object",
            Self::Array => "array",
        }
    }
}

fn wrong_type(cx: &mut Context<'_>, expr: &Expr<'_>, what: &str, want: Want, ty: &Ty) {
    let message = format!(
        "type of expression at {} must be {} but found type {ty}",
        quote(what),
        want.name()
    );
    cx.report_in(expr.source, &WRONG_EXPRESSION_TYPE, expr.span(), message);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use workflint_diagnostics::{FileId, SourceFile};
    use workflint_syntax::Template;

    use super::*;

    /// Returns the messages that [`one_expression`] reports for the value `value`.
    fn one_expression_messages(value: &str) -> Vec<String> {
        let file = SourceFile::new(FileId::new(0), "test.yaml", value).unwrap();
        let document = workflint_yaml::parse(&file).document.unwrap();
        let node = document.node(document.root().unwrap()).unwrap();
        let template = Template::new(Str::new(node).unwrap());
        let mut cx = Context::new(&file);
        one_expression(
            &mut cx,
            template.source,
            template.placeholders(),
            "bool value",
        );
        cx.into_diagnostics()
            .into_iter()
            .map(|d| format!("{} [{}]", d.message, d.rule.id))
            .collect()
    }

    #[test]
    fn a_value_with_one_expression_is_not_reported() {
        assert_eq!(one_expression_messages("${{ true }}"), Vec::<String>::new());
    }

    #[test]
    fn a_value_with_no_expression_or_with_two_is_reported() {
        assert_eq!(
            one_expression_messages("true"),
            [
                r#"one ${{ }} expression should be included in "bool value" value but got 0 expressions [single-expression-required]"#
            ]
        );
        assert_eq!(
            one_expression_messages("${{ true }} ${{ false }}"),
            [
                r#"one ${{ }} expression should be included in "bool value" value but got 2 expressions [single-expression-required]"#
            ]
        );
    }
}
