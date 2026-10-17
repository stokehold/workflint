//! Reports the parse errors of the expressions in a workflow, after the
//! parser has built it.
//!
//! The parser parses the placeholders of each string in a place that the
//! reference's `expression` rule checks. It reports their errors only once
//! the workflow is built, for the strings that the workflow holds. The
//! reference drops some values after parsing them, such as `with` of a job
//! that has `steps` as well as `uses`, and its rule never checks them. So
//! the walk below visits what the reference's rule visits, and nothing
//! else.

use std::collections::HashSet;
use std::slice;

use super::Parser;
use crate::{
    Concurrency, Container, Credentials, Defaults, Env, Environment, Event, Exec, Expr, Field, Job,
    Keyed, Matrix, MatrixCombination, MatrixRow, Placeholder, RawValue, Runner, Snapshot, Step,
    Str, Template, Value, Workflow, WorkflowCall, WorkflowCallSecrets,
};

/// What the walk calls for each string with placeholders: the string and
/// its placeholders.
type Visit<'v, 'w, 'a> = dyn FnMut(Str<'a>, &'w [Placeholder]) + 'v;

impl<'a> Parser<'a, '_> {
    /// Reports the parse error of each placeholder in `workflow` that the
    /// reference's `expression` rule checks.
    ///
    /// A string reached through several aliases is reported once.
    pub(super) fn report_expressions(&mut self, workflow: &Workflow<'a>) {
        let mut found = Vec::new();
        placeholders(workflow, &mut |source, placeholders| {
            found.push((source, placeholders));
        });
        let mut seen = HashSet::new();
        for (source, placeholders) in found {
            if !seen.insert(placeholders.as_ptr().addr()) {
                continue;
            }
            for placeholder in placeholders {
                self.report_placeholder(source, placeholder);
            }
        }
    }

    /// Reports the parse error of `placeholder`, if it has one, at the
    /// characters of `source` that the error is about.
    pub(super) fn report_placeholder(&mut self, source: Str<'a>, placeholder: &Placeholder) {
        let Err(err) = &placeholder.expr else {
            return;
        };
        let span = source
            .source_span(self.file, placeholder.to_value(err.range()))
            .unwrap_or_else(|| source.span());
        self.error(err.rule, span, err.message.clone());
    }
}

/// Calls `f` for each string with placeholders in `workflow` that the
/// reference's `expression` rule checks, in source order within each
/// section. These are the places of `docs/research/02-expressions.md`
/// section 7.2. [`parse`](crate::parse) reports the parse errors of these
/// placeholders, and the `expression` rule of `workflint-rules` checks
/// them, so a test there compares the two walks.
///
/// The `required` keys of `workflow_call` inputs and secrets are left out,
/// because the reference does not check them, and so are the `value` keys
/// of `workflow_call` outputs in a workflow with no jobs. The expressions of `include`
/// elements are visited, which the reference skips by mistake
/// (`docs/differences.md#matrix-include-expressions`).
pub fn placeholders<'w, 'a>(
    workflow: &'w Workflow<'a>,
    f: &mut dyn FnMut(Str<'a>, &'w [Placeholder]),
) {
    string(f, &workflow.name);
    string(f, &workflow.run_name);
    let has_jobs = workflow.jobs.value().is_some_and(|jobs| !jobs.is_empty());
    for event in workflow
        .on
        .value()
        .into_iter()
        .flat_map(|events| events.iter().flatten())
    {
        event_placeholders(f, event, has_jobs);
    }
    env(f, &workflow.env);
    defaults(f, &workflow.defaults);
    concurrency(f, &workflow.concurrency);
    for job in workflow.jobs.value().into_iter().flatten() {
        job_placeholders(f, &job.value);
    }
}

fn event_placeholders<'w, 'a>(f: &mut Visit<'_, 'w, 'a>, event: &'w Event<'a>, has_jobs: bool) {
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
                strings(f, filter);
            }
        }
        Event::Schedule(schedule) => {
            for entry in schedule.schedules.iter() {
                string(f, &entry.cron);
                string(f, &entry.timezone);
            }
        }
        Event::WorkflowDispatch(dispatch) => {
            for input in dispatch.inputs.value().into_iter().flatten() {
                string(f, &input.value.description);
                string(f, &input.value.default);
                expression(f, &input.value.required);
                strings(f, &input.value.options);
            }
        }
        Event::RepositoryDispatch(dispatch) => strings(f, &dispatch.types),
        Event::WorkflowCall(call) => {
            for input in call.inputs.value().into_iter().flatten() {
                string(f, &input.value.description);
                string(f, &input.value.default);
            }
            for secret in call.secrets.value().into_iter().flatten() {
                string(f, &secret.value.description);
            }
            for output in call.outputs.value().into_iter().flatten() {
                string(f, &output.value.description);
                if has_jobs {
                    string(f, &output.value.value);
                }
            }
        }
        Event::ImageVersion(image) => {
            strings(f, &image.names);
            strings(f, &image.versions);
        }
    }
}

fn job_placeholders<'w, 'a>(f: &mut Visit<'_, 'w, 'a>, job: &'w Job<'a>) {
    string(f, &job.name);
    strings(f, &job.needs);
    if let Some(Runner { labels, group }) = job.runs_on.value() {
        expression(f, labels);
        for label in labels
            .literal()
            .into_iter()
            .flat_map(|labels| labels.iter().flatten())
        {
            template(f, label);
        }
        string(f, group);
    }
    concurrency(f, &job.concurrency);
    env(f, &job.env);
    defaults(f, &job.defaults);
    string(f, &job.condition);
    if let Some(strategy) = job.strategy.value() {
        matrix(f, &strategy.matrix);
        expression(f, &strategy.fail_fast);
        expression(f, &strategy.max_parallel);
    }
    expression(f, &job.continue_on_error);
    expression(f, &job.timeout_minutes);
    if let Some(container) = job.container.value() {
        container_placeholders(f, container);
    }
    expression(f, &job.services);
    for service in job.services.literal().into_iter().flatten() {
        container_placeholders(f, &service.value);
    }
    if let Some(WorkflowCall {
        uses,
        inputs,
        secrets,
    }) = &job.workflow_call
    {
        if let Some(uses) = uses.value() {
            template(f, &uses.template);
        }
        keyed(f, inputs.value());
        if let Some(WorkflowCallSecrets::Map(secrets)) = secrets.value() {
            keyed(f, Some(secrets));
        }
    }
    if let Some(Snapshot { condition, .. }) = job.snapshot.value() {
        string(f, condition);
    }
    if let Some(Environment {
        name,
        url,
        deployment,
    }) = job.environment.value()
    {
        string(f, name);
        string(f, url);
        expression(f, deployment);
    }
    keyed(f, job.outputs.value());
    for step in job.steps.value().into_iter().flat_map(|steps| steps.iter()) {
        step_placeholders(f, step);
    }
}

fn step_placeholders<'w, 'a>(f: &mut Visit<'_, 'w, 'a>, step: &'w Step<'a>) {
    string(f, &step.id);
    string(f, &step.condition);
    string(f, &step.name);
    match &step.exec {
        Some(Exec::Run(run)) => {
            string(f, &run.run);
            string(f, &run.shell);
            string(f, &run.working_directory);
        }
        Some(Exec::Action(action)) => {
            if let Some(uses) = action.uses.value() {
                template(f, &uses.template);
            }
            keyed(f, action.inputs.value());
            string(f, &action.entrypoint);
            string(f, &action.args);
        }
        None => {}
    }
    env(f, &step.env);
    expression(f, &step.continue_on_error);
    expression(f, &step.timeout_minutes);
}

fn container_placeholders<'w, 'a>(f: &mut Visit<'_, 'w, 'a>, container: &'w Container<'a>) {
    string(f, &container.image);
    expression(f, &container.credentials);
    if let Some(Credentials { username, password }) = container.credentials.literal() {
        string(f, username);
        string(f, password);
    }
    env(f, &container.env);
    strings(f, &container.ports);
    strings(f, &container.volumes);
    string(f, &container.options);
    string(f, &container.command);
    string(f, &container.entrypoint);
}

fn matrix<'w, 'a>(f: &mut Visit<'_, 'w, 'a>, matrix: &'w Value<'a, Matrix<'a>>) {
    expression(f, matrix);
    let Some(matrix) = matrix.literal() else {
        return;
    };
    for row in &matrix.rows {
        match &row.value {
            MatrixRow::Values(values) => raw_values(f, values.iter()),
            MatrixRow::Expression(expr) => expr_placeholders(f, expr),
            MatrixRow::Invalid => {}
        }
    }
    for combinations in [&matrix.include, &matrix.exclude] {
        expression(f, combinations);
        for combination in combinations.literal().into_iter().flat_map(|c| c.iter()) {
            match combination {
                MatrixCombination::Assigns { assigns, .. } => {
                    raw_values(f, assigns.iter().map(|assign| &assign.value));
                }
                MatrixCombination::Expression(expr) => expr_placeholders(f, expr),
                MatrixCombination::Invalid(_) => {}
            }
        }
    }
}

/// Visits the scalars in matrix values, with a stack of its own, because
/// matrix values may nest as deep as the YAML layer allows.
fn raw_values<'w, 'a: 'w>(
    f: &mut Visit<'_, 'w, 'a>,
    values: impl Iterator<Item = &'w RawValue<'a>>,
) {
    let mut stack: Vec<&'w RawValue<'a>> = values.collect();
    stack.reverse();
    while let Some(value) = stack.pop() {
        match value {
            RawValue::Scalar(scalar) => template(f, scalar),
            RawValue::Sequence { items, .. } => stack.extend(items.iter().rev()),
            RawValue::Mapping { entries, .. } => {
                stack.extend(entries.iter().rev().map(|entry| &entry.value));
            }
        }
    }
}

fn env<'w, 'a>(f: &mut Visit<'_, 'w, 'a>, env: &'w Value<'a, Env<'a>>) {
    expression(f, env);
    for var in env.literal().into_iter().flatten() {
        template(f, &var.value.name);
        if let Some(value) = &var.value.value {
            template(f, value);
        }
    }
}

fn defaults<'w, 'a>(f: &mut Visit<'_, 'w, 'a>, defaults: &'w Field<'a, Defaults<'a>>) {
    if let Some(run) = defaults.value().and_then(|defaults| defaults.run.value()) {
        string(f, &run.shell);
        string(f, &run.working_directory);
    }
}

fn concurrency<'w, 'a>(f: &mut Visit<'_, 'w, 'a>, concurrency: &'w Field<'a, Concurrency<'a>>) {
    if let Some(concurrency) = concurrency.value() {
        string(f, &concurrency.group);
        expression(f, &concurrency.cancel_in_progress);
    }
}

fn keyed<'w, 'a>(f: &mut Visit<'_, 'w, 'a>, keyed: Option<&'w Keyed<'a, Option<Template<'a>>>>) {
    for entry in keyed.into_iter().flatten() {
        if let Some(value) = &entry.value {
            template(f, value);
        }
    }
}

fn strings<'w, 'a>(
    f: &mut Visit<'_, 'w, 'a>,
    strings: &'w Field<'a, std::sync::Arc<[Option<Template<'a>>]>>,
) {
    for string in strings
        .value()
        .into_iter()
        .flat_map(|strings| strings.iter().flatten())
    {
        template(f, string);
    }
}

fn string<'w, 'a>(f: &mut Visit<'_, 'w, 'a>, string: &'w Field<'a, Template<'a>>) {
    if let Some(string) = string.value() {
        template(f, string);
    }
}

fn expression<'w, 'a, T>(f: &mut Visit<'_, 'w, 'a>, value: &'w Value<'a, T>) {
    if let Some(expr) = value.expression() {
        expr_placeholders(f, expr);
    }
}

fn expr_placeholders<'w, 'a>(f: &mut Visit<'_, 'w, 'a>, expr: &'w Expr<'a>) {
    f(expr.source, slice::from_ref(expr.placeholder()));
}

fn template<'w, 'a>(f: &mut Visit<'_, 'w, 'a>, template: &'w Template<'a>) {
    let placeholders = template.placeholders();
    if !placeholders.is_empty() {
        f(template.source, placeholders);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use workflint_diagnostics::{FileId, SourceFile};

    use super::*;
    use crate::testing::render;

    /// A workflow with an expression `${{ x.<place> }}` in every place of
    /// `docs/research/02-expressions.md` section 7.2. Places that take a
    /// value in two forms, such as `env` as a mapping or as one expression,
    /// are in two jobs.
    const EVERY_PLACE: &str = r"
name: ${{ x.name }}
run-name: ${{ x.run_name }}
on:
  push:
    types: ${{ x.types }}
    branches: ['${{ x.branches }}']
    branches-ignore: ${{ x.branches_ignore }}
    tags: ${{ x.tags }}
    tags-ignore: ${{ x.tags_ignore }}
    paths: ${{ x.paths }}
    paths-ignore: ${{ x.paths_ignore }}
    workflows: ${{ x.workflows }}
  schedule:
    - cron: ${{ x.cron }}
      timezone: ${{ x.timezone }}
  workflow_dispatch:
    inputs:
      i:
        description: ${{ x.dispatch_description }}
        default: ${{ x.dispatch_default }}
        required: ${{ x.dispatch_required }}
        type: choice
        options: ['${{ x.dispatch_option }}']
  repository_dispatch:
    types: ['${{ x.repository_dispatch_types }}']
  workflow_call:
    inputs:
      i:
        description: ${{ x.call_input_description }}
        default: ${{ x.call_input_default }}
        type: string
    secrets:
      s:
        description: ${{ x.call_secret_description }}
    outputs:
      o:
        description: ${{ x.call_output_description }}
        value: ${{ x.call_output_value }}
  image_version:
    names: ['${{ x.image_names }}']
    versions: ['${{ x.image_versions }}']
env:
  ${{ x.env_name }}: ${{ x.env_value }}
defaults:
  run:
    shell: ${{ x.defaults_shell }}
    working-directory: ${{ x.defaults_working_directory }}
concurrency:
  group: ${{ x.concurrency_group }}
  cancel-in-progress: ${{ x.concurrency_cancel }}
jobs:
  a:
    name: ${{ x.job_name }}
    needs: ['${{ x.needs }}']
    runs-on:
      labels: ['${{ x.label }}']
      group: ${{ x.runner_group }}
    concurrency: ${{ x.job_concurrency_group }}
    env: ${{ x.job_env }}
    defaults:
      run:
        shell: ${{ x.job_defaults_shell }}
        working-directory: ${{ x.job_defaults_working_directory }}
    if: ${{ x.job_if }}
    strategy:
      matrix:
        row: ${{ x.matrix_row }}
        values: ['${{ x.matrix_value }}', {k: ['${{ x.matrix_nested_value }}']}]
        include:
          - ${{ x.include_element }}
          - k: ${{ x.include_value }}
        exclude: ${{ x.exclude }}
      fail-fast: ${{ x.fail_fast }}
      max-parallel: ${{ x.max_parallel }}
    continue-on-error: ${{ x.job_continue_on_error }}
    timeout-minutes: ${{ x.job_timeout_minutes }}
    container:
      image: ${{ x.container_image }}
      credentials: ${{ x.container_credentials }}
      env:
        ${{ x.container_env_name }}: ${{ x.container_env_value }}
      ports: ['${{ x.container_port }}']
      volumes: ['${{ x.container_volume }}']
      options: ${{ x.container_options }}
    services:
      redis:
        image: ${{ x.service_image }}
        credentials:
          username: ${{ x.service_username }}
          password: ${{ x.service_password }}
        command: ${{ x.service_command }}
        entrypoint: ${{ x.service_entrypoint }}
    environment:
      name: ${{ x.environment_name }}
      url: ${{ x.environment_url }}
      deployment: ${{ x.environment_deployment }}
    outputs:
      o: ${{ x.job_output }}
    snapshot:
      image-name: image
      if: ${{ x.snapshot_if }}
    steps:
      - id: ${{ x.step_id }}
        name: ${{ x.step_name }}
        if: ${{ x.step_if }}
        run: echo ${{ x.run }}
        shell: ${{ x.step_shell }}
        working-directory: ${{ x.step_working_directory }}
        env: ${{ x.step_env }}
        continue-on-error: ${{ x.step_continue_on_error }}
        timeout-minutes: ${{ x.step_timeout_minutes }}
      - uses: ${{ x.step_uses }}
        with:
          i: ${{ x.step_with }}
        env:
          E: ${{ x.step_env_value }}
      - uses: docker://alpine
        with:
          entrypoint: ${{ x.docker_entrypoint }}
          args: ${{ x.docker_args }}
  b:
    runs-on: ${{ x.runs_on }}
    concurrency:
      group: ${{ x.job_concurrency_group_in_mapping }}
      cancel-in-progress: ${{ x.job_concurrency_cancel }}
    env:
      ${{ x.job_env_name }}: ${{ x.job_env_value }}
    strategy:
      matrix: ${{ x.matrix }}
    container: ${{ x.container }}
    services: ${{ x.services }}
    environment: ${{ x.environment }}
    steps:
      - run: echo
  c:
    runs-on: [linux, '${{ x.label_in_sequence }}']
    strategy:
      matrix:
        row: [1]
        include: ${{ x.include }}
        exclude:
          - ${{ x.exclude_element }}
          - k: ${{ x.exclude_value }}
    container:
      image: image
      credentials:
        username: ${{ x.container_username }}
        password: ${{ x.container_password }}
    services:
      redis:
        image: redis
        credentials: ${{ x.service_credentials }}
        env:
          ${{ x.service_env_name }}: ${{ x.service_env_value }}
        ports: ['${{ x.service_port }}']
        volumes: ['${{ x.service_volume }}']
        options: ${{ x.service_options }}
      postgres:
        image: postgres
        env: ${{ x.service_env }}
    steps:
      - run: echo
  d:
    uses: ${{ x.call_uses }}
    with:
      i: ${{ x.call_with }}
    secrets:
      s: ${{ x.call_secret }}
";

    /// The second form of the workflow-level places of [`EVERY_PLACE`]
    /// that take a value in two forms.
    const WORKFLOW_SECOND_FORMS: &str = r"
on: push
env: ${{ x.env }}
concurrency: ${{ x.concurrency }}
jobs:
  a:
    runs-on: linux
    steps:
      - run: echo
";

    /// Parses `text` and returns the name after `x.` of each valid
    /// expression that the walk finds, sorted, and the parser's
    /// diagnostics.
    fn walk(text: &str) -> (Vec<String>, Vec<String>) {
        let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();
        let document = workflint_yaml::parse(&file).document.unwrap();
        let parsed = crate::parse(&file, &document);
        let mut names = Vec::new();
        placeholders(&parsed.workflow, &mut |source, placeholders| {
            for placeholder in placeholders {
                if let Ok(expr) = &placeholder.expr {
                    let text = &source.value()[placeholder.to_value(expr.range())];
                    names.push(text.strip_prefix("x.").unwrap_or(text).to_owned());
                }
            }
        });
        names.sort();
        (names, render(&file, parsed.diagnostics))
    }

    #[test]
    fn every_place_of_the_expression_rule_has_its_expressions_parsed() {
        for text in [EVERY_PLACE, WORKFLOW_SECOND_FORMS] {
            let (names, diagnostics) = walk(text);
            let mut want: Vec<_> = text
                .split("${{ x.")
                .skip(1)
                .map(|rest| rest.split_once(" }}").unwrap().0.to_owned())
                .collect();
            want.sort();
            assert_eq!(names, want);
            assert!(
                !diagnostics.iter().any(|d| d.contains("expression-syntax")),
                "{diagnostics:#?}"
            );
        }
    }

    #[test]
    fn errors_are_reported_where_the_expression_rule_checks() {
        let text = "\
on: push
jobs:
  test:
    runs-on: ${{ a b }}
    steps:
      - run: echo ${{ c d }} ${{ e f }}
";
        let (_, diagnostics) = walk(text);
        let syntax: Vec<_> = diagnostics
            .iter()
            .filter_map(|d| d.strip_suffix(" [invalid-expression-syntax]"))
            .map(|d| d.split_once(": ").unwrap().0)
            .collect();
        assert_eq!(syntax, ["4:20", "6:25", "6:36"]);
    }

    /// Returns the `L:C` position of each expression syntax error in
    /// `text`.
    fn syntax_errors(text: &str) -> Vec<String> {
        walk(text)
            .1
            .iter()
            .filter(|d| d.contains("expression-syntax"))
            .map(|d| d.split_once(": ").unwrap().0.to_owned())
            .collect()
    }

    #[test]
    fn an_error_at_the_end_of_a_block_scalar_is_after_its_last_character() {
        // The reference reports 8:22 for `|` and 9:22 for `|+`, and 7:33
        // for `|-`, on the line of the header.
        for header in ["|", "|+", "|-", ">", ">+", ">-"] {
            let text = format!(
                "\
on: push
jobs:
  test:
    runs-on: linux
    steps:
      - run: echo start
      - run: {header}
          echo ${{{{ github.ref

      - run: echo done
"
            );
            assert_eq!(syntax_errors(&text), ["8:30"], "{header}");
        }
    }

    #[test]
    fn an_error_at_the_end_of_a_block_if_condition_is_after_its_last_character() {
        // The reference reports 6:9.
        let text = "\
on: push
jobs:
  test:
    runs-on: linux
    if: |
      github.ref == (
    timeout-minutes: 5
    steps:
      - run: echo
";
        assert_eq!(syntax_errors(text), ["6:22"]);
    }

    #[test]
    fn a_reusable_call_with_services_has_its_inputs_and_secrets_checked() {
        let text = "\
on: push
jobs:
  call:
    uses: o/r/.github/workflows/x.yml@v1
    services:
      s: redis
    with:
      a: ${{ 1 + }}
    secrets:
      b: ${{ 2 + }}
";
        assert_eq!(syntax_errors(text), ["8:16", "10:16"]);
    }

    #[test]
    fn places_that_the_expression_rule_skips_report_nothing() {
        let text = "\
on:
  workflow_call:
    inputs:
      i:
        type: string
        required: ${{ a b }}
    secrets:
      s:
        required: ${{ a b }}
jobs:
  ${{ a b }}:
    runs-on: linux
    permissions:
      contents: ${{ a b }}
    snapshot:
      image-name: ${{ a b }}
      version: ${{ a b }}
    steps:
      - id: '}} ${{ a b }}'
        run: echo
  call:
    uses: ${{ a b }}
    with:
      i: ${{ a b }}
    secrets:
      s: ${{ a b }}
    steps:
      - run: echo
  no-uses:
    runs-on: linux
    with:
      i: ${{ a b }}
    steps:
      - run: echo
";
        let (_, diagnostics) = walk(text);
        assert!(
            !diagnostics.iter().any(|d| d.contains("expression-syntax")),
            "{diagnostics:#?}"
        );
    }

    #[test]
    fn workflow_call_outputs_are_checked_only_in_a_workflow_with_jobs() {
        let outputs = "\
on:
  workflow_call:
    outputs:
      o:
        value: ${{ x.value }}
";
        assert_eq!(walk(&format!("{outputs}jobs:\n")).0, Vec::<String>::new());
        let (names, _) = walk(&format!("{outputs}jobs:\n  a:\n    uses: ./a.yml\n"));
        assert_eq!(names, ["value"]);
    }

    #[test]
    fn a_string_reached_through_aliases_is_reported_once() {
        let text = "\
on: push
jobs:
  a:
    runs-on: linux
    steps: &steps
      - run: echo ${{ a b }}
  b:
    runs-on: linux
    steps: *steps
";
        let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();
        let document = workflint_yaml::parse(&file).document.unwrap();
        let parsed = crate::parse(&file, &document);
        let syntax = parsed
            .diagnostics
            .iter()
            .filter(|d| d.rule.id == "invalid-expression-syntax")
            .count();
        assert_eq!(syntax, 1);
    }
}
