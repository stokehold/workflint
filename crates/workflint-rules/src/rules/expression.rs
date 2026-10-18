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

use std::collections::BTreeMap;
use std::sync::Arc;

use workflint_data::UNTRUSTED_INPUTS;
use workflint_diagnostics::quote::quote;
use workflint_diagnostics::{Category, RuleMeta, Severity, Span};
use workflint_expr::{ObjectTy, Rest, Ty, TypeEnv};
use workflint_syntax::{
    Concurrency, Container, Credentials, Defaults, Env, Event, Exec, Expr, Field, Job, Keyed,
    KeyedEntry, Matrix, MatrixCombination, MatrixRow, Placeholder, RawValue, Runner, Snapshot,
    Step, Str, Template, Value, Workflow, WorkflowCall, WorkflowCallEvent, WorkflowCallInputType,
    WorkflowCallSecrets, WorkflowDispatchEvent, WorkflowDispatchInputType,
};

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

/// The `expression` rule.
#[derive(Debug, Default)]
pub(crate) struct Expression {
    /// The contexts of the workflow, which every job starts from.
    workflow: TypeEnv,
    /// The contexts of the job being visited, from its job-pre hook to its job-post hook.
    job: Option<TypeEnv>,
}

impl Expression {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Returns the contexts of the job being visited, or of the workflow outside a job.
    fn env(&self) -> &TypeEnv {
        self.job.as_ref().unwrap_or(&self.workflow)
    }

    /// Returns a checker for the location with the workflow key `key`.
    fn at(&self, key: &str) -> Checker {
        let mut env = self.env().clone();
        env.set_workflow_key(key);
        Checker { env }
    }
}

impl<'a> Rule<'a> for Expression {
    fn workflow_pre(&mut self, workflow: &'a Workflow<'a>, cx: &mut Context<'_>) {
        // The events add their inputs to the contexts of the workflow, in the order of `on`.
        self.workflow = TypeEnv::new();
        let none = self.at("");
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
        self.job = Some(job_env(&self.workflow, job));

        self.at("jobs.<job_id>.name").string(cx, &job.name);
        self.at("").strings(cx, &job.needs);
        if let Some(Runner { labels, group }) = job.runs_on.value() {
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
        }
        concurrency(cx, &self.at("jobs.<job_id>.concurrency"), &job.concurrency);
        self.env_vars(cx, &job.env, "jobs.<job_id>.env");
        defaults(cx, &self.at("jobs.<job_id>.defaults.run"), &job.defaults);
        self.at("jobs.<job_id>.if").condition(cx, &job.condition);
        if let Some(strategy) = job.strategy.value() {
            let strategy_env = self.at("jobs.<job_id>.strategy");
            strategy_env.matrix(cx, &strategy.matrix);
            strategy_env.bool(cx, &strategy.fail_fast);
            strategy_env.number(cx, &strategy.max_parallel, "integer value");
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
                "jobs.<job_id>.container",
                "jobs.<job_id>.container",
            );
        }
        let services = self.at("jobs.<job_id>.services");
        if let Value::Expression { expr, .. } = &job.services {
            services.object(cx, expr, "services");
        }
        for service in job.services.literal().into_iter().flatten() {
            self.container(
                cx,
                &service.value,
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

        // The steps of the job and the keys of its job-post hook see the steps before them.
        // TODO(#146): type `steps` step by step from the outputs of each step with an `id`.
        if let Some(env) = &mut self.job {
            env.set_steps(loose());
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
        env.set_workflow_key("on.workflow_call.outputs.<output_id>.value");
        let checker = Checker { env };
        for output in outputs {
            checker.string(cx, &output.value.value);
        }
    }
}

impl Expression {
    /// Checks the expressions of `event`, and adds its inputs to the contexts of the workflow.
    fn event(&mut self, event: &Event<'_>, cx: &mut Context<'_>) {
        let none = self.at("");
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
            env.set_workflow_key("on.workflow_call.inputs.<inputs_id>.default");
            let default = input.value.default.value();
            let types = default
                .map(|default| Checker { env }.template(cx, default))
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
                cx.report(&WRONG_INPUT_DEFAULT_TYPE, default.span(), message);
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
            }
            Value::Absent | Value::Invalid { .. } => {}
        }
    }

    /// Checks a container or a service. `key` is the workflow key of the container's strings,
    /// and `child` the prefix of the keys of its `credentials` and `env`.
    fn container(&self, cx: &mut Context<'_>, container: &Container<'_>, key: &str, child: &str) {
        let checker = self.at(key);
        checker.string(cx, &container.image);
        let credentials = self.at(&format!("{child}.credentials"));
        match &container.credentials {
            Value::Expression { expr, .. } => credentials.object(cx, expr, "credentials"),
            Value::Literal {
                value: Credentials { username, password },
                ..
            } => {
                credentials.string(cx, username);
                credentials.string(cx, password);
            }
            Value::Absent | Value::Invalid { .. } => {}
        }
        self.env_vars(cx, &container.env, &format!("{child}.env.<env_id>"));
        checker.strings(cx, &container.ports);
        checker.strings(cx, &container.volumes);
        checker.string(cx, &container.options);
        checker.string(cx, &container.command);
        checker.string(cx, &container.entrypoint);
    }

    /// Checks the call of a reusable workflow.
    fn workflow_call(&self, cx: &mut Context<'_>, call: &WorkflowCall<'_>) {
        if let Some(uses) = call.uses.value() {
            self.at("").template(cx, &uses.template);
        }
        // TODO(#147): check the type of each value against the input of a local reusable
        // workflow.
        keyed(
            cx,
            &self.at("jobs.<job_id>.with.<with_id>"),
            call.inputs.value(),
        );
        if let Some(WorkflowCallSecrets::Map(secrets)) = call.secrets.value() {
            keyed(
                cx,
                &self.at("jobs.<job_id>.secrets.<secrets_id>"),
                Some(secrets),
            );
        }
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

/// Returns the contexts of `job`, which its keys and steps start from.
fn job_env(workflow: &TypeEnv, job: &Job<'_>) -> TypeEnv {
    let mut env = workflow.clone();
    // TODO(#147): type `needs` from the direct dependencies of the job and their outputs.
    env.set_needs(loose());
    if job
        .strategy
        .value()
        .is_some_and(|strategy| strategy.matrix.is_present())
    {
        // TODO(#145): type `matrix` from the rows, `include`, and a whole-matrix expression.
        env.set_matrix(loose());
    }
    env
}

/// An object type with no known property, which accepts any property.
fn loose() -> Arc<ObjectTy> {
    Arc::new(ObjectTy::new([] as [(&str, Ty); 0], Rest::Loose))
}

fn defaults(cx: &mut Context<'_>, checker: &Checker, defaults: &Field<'_, Defaults<'_>>) {
    if let Some(run) = defaults.value().and_then(|defaults| defaults.run.value()) {
        checker.string(cx, &run.shell);
        checker.string(cx, &run.working_directory);
    }
}

fn concurrency(cx: &mut Context<'_>, checker: &Checker, concurrency: &Field<'_, Concurrency<'_>>) {
    if let Some(concurrency) = concurrency.value() {
        checker.string(cx, &concurrency.group);
        checker.bool(cx, &concurrency.cancel_in_progress);
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
struct Checker {
    env: TypeEnv,
}

impl Checker {
    /// Type-checks `placeholder` of `source` and reports its errors. Returns its type, or `None`
    /// if it holds a parse error, which the parser has reported, or has errors.
    fn placeholder(
        &self,
        cx: &mut Context<'_>,
        source: Str<'_>,
        placeholder: &Placeholder,
    ) -> Option<Ty> {
        let expr = placeholder.expr.as_ref().ok()?;
        let (ty, errors) = self.env.check(expr);
        if errors.is_empty() {
            return Some(ty);
        }
        for error in errors {
            let span = source_span(cx, source, placeholder.to_value(error.range()));
            cx.report(error.rule, span, error.message);
        }
        None
    }

    /// Checks each placeholder of `template`, and reports a placeholder that evaluates to an
    /// object, an array, or null, which GitHub cannot put in a string. Returns the types of the
    /// placeholders without errors.
    fn template(&self, cx: &mut Context<'_>, template: &Template<'_>) -> Vec<Ty> {
        let mut types = Vec::new();
        for placeholder in template.placeholders() {
            let Some(ty) = self.placeholder(cx, template.source, placeholder) else {
                continue;
            };
            if !placeholder.is_implicit() && matches!(ty, Ty::Object(_) | Ty::Array(_) | Ty::Null) {
                let start = usize::try_from(placeholder.start).unwrap_or(usize::MAX);
                let span = source_span(cx, template.source, start..start + "${{".len());
                cx.report(
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
        let mut env = self.env.clone();
        env.set_untrusted_inputs(UNTRUSTED_INPUTS);
        Checker { env }.template(cx, template);
    }

    /// Checks a value written as one `${{ }}` and returns its type, or `None` if it has errors.
    fn one(&self, cx: &mut Context<'_>, expr: &Expr<'_>, what: &str) -> Option<Ty> {
        one_expression(
            cx,
            expr.source,
            std::slice::from_ref(expr.placeholder()),
            what,
        )?;
        self.placeholder(cx, expr.source, expr.placeholder())
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
            cx.report(&WRONG_EXPRESSION_TYPE, expr.span(), message);
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

    /// Checks the expressions of a matrix, which the reference checks while it types `matrix`
    /// (`checkMatrix`). A value written as one `${{ }}` must be an object for the whole matrix,
    /// an array for a row, and an array of objects for `exclude`.
    ///
    /// TODO(#145): type `matrix` from these expressions and values, and check an element of
    /// `include` written as one `${{ }}`, which the reference skips
    /// (`docs/differences.md#matrix-include-expressions`).
    fn matrix(&self, cx: &mut Context<'_>, matrix: &Value<'_, Matrix<'_>>) {
        let matrix = match matrix {
            Value::Expression { expr, .. } => {
                self.object(cx, expr, "matrix");
                return;
            }
            Value::Literal { value, .. } => value,
            Value::Absent | Value::Invalid { .. } => return,
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
                            self.raw_values(cx, assigns.iter().map(|assign| &assign.value));
                        }
                        MatrixCombination::Invalid(_) => {}
                    }
                }
            }
            Value::Absent | Value::Invalid { .. } => {}
        }
        for row in &matrix.rows {
            match &row.value {
                MatrixRow::Values(values) => self.raw_values(cx, values.iter()),
                MatrixRow::Expression(expr) => {
                    self.typed(cx, expr, "matrix row", Want::Array);
                }
                MatrixRow::Invalid => {}
            }
        }
        match &matrix.include {
            Value::Expression { expr, .. } => {
                self.one(cx, expr, "include");
            }
            Value::Literal { value, .. } => {
                for combination in value.iter() {
                    if let MatrixCombination::Assigns { assigns, .. } = combination {
                        self.raw_values(cx, assigns.iter().map(|assign| &assign.value));
                    }
                }
            }
            Value::Absent | Value::Invalid { .. } => {}
        }
    }

    /// Checks the scalars in matrix values, with a stack of its own, because matrix values may
    /// nest as deep as the YAML layer allows. A scalar is not a template: its value becomes a
    /// matrix value, which may be an object.
    fn raw_values<'v, 'a: 'v>(
        &self,
        cx: &mut Context<'_>,
        values: impl Iterator<Item = &'v RawValue<'a>>,
    ) {
        let mut stack: Vec<&'v RawValue<'a>> = values.collect();
        stack.reverse();
        while let Some(value) = stack.pop() {
            match value {
                RawValue::Scalar(scalar) => {
                    for placeholder in scalar.placeholders() {
                        self.placeholder(cx, scalar.source, placeholder);
                    }
                }
                RawValue::Sequence { items, .. } => stack.extend(items.iter().rev()),
                RawValue::Mapping { entries, .. } => {
                    stack.extend(entries.iter().rev().map(|entry| &entry.value));
                }
            }
        }
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
            cx.report(&WRONG_EXPRESSION_TYPE, expr.span(), message);
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
    cx.report(&SINGLE_EXPRESSION_REQUIRED, source.span(), message);
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
    cx.report(&WRONG_EXPRESSION_TYPE, expr.span(), message);
}

/// Returns the span of the bytes `range` of the value of `source`, or the span of `source` if
/// its source map cannot map them.
fn source_span(cx: &Context<'_>, source: Str<'_>, range: std::ops::Range<usize>) -> Span {
    source
        .source_span(cx.file(), range)
        .unwrap_or_else(|| source.span())
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
