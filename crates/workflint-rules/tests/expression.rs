//! The `expression` rule: every location that the reference's rule checks, with its workflow key,
//! its expected type, and the untrusted-input check of scripts.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeSet;

use workflint_diagnostics::{
    Color, Diagnostic, FileId, LineCol, SourceFile, render_oneline, sort_and_dedup,
};

/// Lints `workflow` as the binary does, without the file system, and returns the sorted
/// diagnostics with the file.
fn diagnostics(workflow: &str) -> (SourceFile, Vec<Diagnostic>) {
    let file = SourceFile::new(FileId::new(0), "test.yaml", workflow).unwrap();
    let parsed = workflint_yaml::parse(&file);
    let mut diagnostics = parsed.diagnostics;
    let document = parsed.document.unwrap();
    let syntax = workflint_syntax::parse(&file, &document);
    diagnostics.extend(syntax.diagnostics);
    diagnostics.extend(workflint_rules::run(&file, &syntax.workflow));
    sort_and_dedup(&file, &mut diagnostics);
    (file, diagnostics)
}

/// Lints `workflow` and returns the diagnostics in the one-line format.
fn lint(workflow: &str) -> Vec<String> {
    let (file, diagnostics) = diagnostics(workflow);
    let mut out = Vec::new();
    render_oneline(&mut out, &file, &diagnostics, Color::Off).unwrap();
    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Returns the `L:C` position of the byte offset `offset` in `text`, with columns counted in
/// characters from 1.
fn position(text: &str, offset: usize) -> String {
    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
    format!("{line}:{column}")
}

/// A workflow with an expression in every location of `docs/research/02-expressions.md`
/// section 7.2 except the matrix, whose expressions M4.8 (#145) checks. Each expression reads a
/// context that its workflow key does not allow. Where the key allows every context, it calls a
/// special function that the key does not allow instead. The locations with the empty workflow
/// key read `github`, which every other key allows.
const EVERY_LOCATION: &str = r"
name: ${{ github.name }}
run-name: ${{ env.run_name }}
on:
  push:
    types: ${{ github.types }}
    branches: ['${{ github.branches }}']
    branches-ignore: ${{ github.branches_ignore }}
    tags: ${{ github.tags }}
    tags-ignore: ${{ github.tags_ignore }}
    paths: ${{ github.paths }}
    paths-ignore: ${{ github.paths_ignore }}
    workflows: ${{ github.workflows }}
  schedule:
    - cron: ${{ github.cron }}
      timezone: ${{ github.timezone }}
  workflow_dispatch:
    inputs:
      i:
        description: ${{ github.dispatch_description }}
        default: ${{ github.dispatch_default }}
        required: ${{ github.dispatch_required }}
        type: choice
        options: ['${{ github.dispatch_option }}']
  repository_dispatch:
    types: ['${{ github.repository_dispatch_types }}']
  workflow_call:
    inputs:
      i:
        description: ${{ github.call_input_description }}
        default: ${{ env.call_input_default }}
        type: string
    secrets:
      s:
        description: ${{ github.call_secret_description }}
    outputs:
      o:
        description: ${{ github.call_output_description }}
        value: ${{ env.call_output_value }}
  image_version:
    names: ['${{ github.image_names }}']
    versions: ['${{ github.image_versions }}']
env:
  ${{ env.env_name }}: ${{ env.env_value }}
defaults:
  run:
    shell: ${{ github.defaults_shell }}
    working-directory: ${{ github.defaults_working_directory }}
concurrency:
  group: ${{ env.concurrency_group }}
  cancel-in-progress: ${{ env.concurrency_cancel }}
jobs:
  a:
    name: ${{ env.job_name }}
    needs: ['${{ github.needs }}']
    runs-on:
      labels: ['${{ env.label }}']
      group: ${{ env.runner_group }}
    concurrency: ${{ env.job_concurrency_group }}
    env: ${{ env.job_env }}
    defaults:
      run:
        shell: ${{ secrets.job_defaults_shell }}
        working-directory: ${{ secrets.job_defaults_working_directory }}
    if: ${{ env.job_if }}
    strategy:
      fail-fast: ${{ env.fail_fast }}
      max-parallel: ${{ env.max_parallel }}
    continue-on-error: ${{ env.job_continue_on_error }}
    timeout-minutes: ${{ env.job_timeout_minutes }}
    container:
      image: ${{ env.container_image }}
      credentials: ${{ job.container_credentials }}
      env:
        ${{ steps.container_env_name }}: ${{ steps.container_env_value }}
      ports: ['${{ env.container_port }}']
      volumes: ['${{ env.container_volume }}']
      options: ${{ env.container_options }}
    services:
      redis:
        image: ${{ env.service_image }}
        credentials:
          username: ${{ job.service_username }}
          password: ${{ job.service_password }}
        command: ${{ env.service_command }}
        entrypoint: ${{ env.service_entrypoint }}
    environment:
      name: ${{ env.environment_name }}
      url: ${{ secrets.environment_url }}
      deployment: ${{ env.environment_deployment }}
    outputs:
      o: ${{ hashFiles('job_output') }}
    snapshot:
      image-name: image
      if: ${{ env.snapshot_if }}
    steps:
      - id: ${{ github.step_id }}
        name: ${{ success('step_name') }}
        if: ${{ secrets.step_if }}
        run: echo ${{ success('run') }}
        shell: ${{ matrix.shell }}
        working-directory: ${{ success('step_working_directory') }}
        env: ${{ success('step_env') }}
        continue-on-error: ${{ success('step_continue_on_error') }}
        timeout-minutes: ${{ success('step_timeout_minutes') }}
      - uses: ${{ github.step_uses }}
        with:
          i: ${{ success('step_with') }}
        env:
          ${{ success('step_env_name') }}: ${{ success('step_env_value') }}
      - uses: docker://alpine
        with:
          entrypoint: ${{ success('docker_entrypoint') }}
          args: ${{ success('docker_args') }}
  b:
    runs-on: ${{ env.runs_on }}
    if: env.implicit_job_if
    concurrency:
      group: ${{ env.job_concurrency_group_in_mapping }}
      cancel-in-progress: ${{ env.job_concurrency_cancel }}
    env:
      ${{ env.job_env_name }}: ${{ env.job_env_value }}
    container: ${{ env.container }}
    services: ${{ env.services }}
    environment: ${{ env.environment }}
    steps:
      - run: echo
        if: secrets.implicit_step_if
  c:
    runs-on: [linux, '${{ env.label_in_sequence }}']
    container:
      image: image
      credentials:
        username: ${{ job.container_username }}
        password: ${{ job.container_password }}
    services:
      redis:
        image: redis
        credentials: ${{ job.service_credentials }}
        env:
          ${{ steps.service_env_name }}: ${{ steps.service_env_value }}
        ports: ['${{ env.service_port }}']
        volumes: ['${{ env.service_volume }}']
        options: ${{ env.service_options }}
      postgres:
        image: postgres
        env: ${{ steps.service_env }}
    snapshot: image
    steps:
      - run: echo
  d:
    uses: ${{ github.call_uses }}
    with:
      i: ${{ env.call_with }}
    secrets:
      s: ${{ env.call_secret }}
";

/// The forms of the workflow-level locations of [`EVERY_LOCATION`] that take a value in two
/// forms.
const WORKFLOW_SECOND_FORMS: &str = r"
on: push
env: ${{ secrets.env }}
concurrency: ${{ env.concurrency }}
jobs:
  a:
    runs-on: linux
    steps:
      - run: echo
";

/// Returns the position of the first token of each expression in `text`: just after `${{ `, or
/// after `if: ` for a condition written without `${{ }}`.
fn expression_starts(text: &str) -> BTreeSet<String> {
    let explicit = text.match_indices("${{ ").map(|(at, _)| at + 4);
    let implicit = text
        .match_indices("if: ")
        .map(|(at, _)| at + 4)
        .filter(|at| !text[*at..].starts_with("${{"));
    explicit
        .chain(implicit)
        .map(|at| position(text, at))
        .collect()
}

#[test]
fn every_location_reports_a_context_or_function_that_its_workflow_key_does_not_allow() {
    for text in [EVERY_LOCATION, WORKFLOW_SECOND_FORMS] {
        let lines = lint(text);
        let mut reported = BTreeSet::new();
        for line in &lines {
            let rest = line.strip_prefix("test.yaml:").unwrap();
            let (at, message) = rest.split_once(": ").unwrap();
            assert!(
                message.ends_with("[context-not-allowed]")
                    || message.ends_with("[function-not-allowed]"),
                "{line}"
            );
            assert!(reported.insert(at.to_owned()), "two errors at {at}");
        }
        assert_eq!(reported, expression_starts(text), "{lines:#?}");
    }
}

#[test]
fn locations_with_the_empty_workflow_key_allow_no_context() {
    let lines = lint(EVERY_LOCATION);
    let github: Vec<_> = lines
        .iter()
        .filter(|line| line.contains(r#"context "github" is not allowed here"#))
        .collect();
    assert_eq!(github.len(), 27, "{github:#?}");
    for line in github {
        assert!(
            line.contains("is not allowed here. no context is available here."),
            "{line}"
        );
    }
}

#[test]
fn a_step_shell_does_not_allow_the_matrix_context() {
    let text = "\
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        shell: [bash, pwsh]
    steps:
      - run: echo
        shell: ${{ matrix.shell }}
";
    assert_eq!(
        lint(text),
        [
            r#"test.yaml:10:20: context "matrix" is not allowed here. no context is available here. see https://docs.github.com/en/actions/learn-github-actions/contexts#context-availability for more details [context-not-allowed]"#
        ]
    );
}

#[test]
fn a_context_that_its_workflow_key_allows_is_not_reported() {
    let text = "\
on:
  workflow_call:
    inputs:
      i:
        type: string
        default: ${{ github.ref }}
    outputs:
      o:
        value: ${{ jobs.a.outputs.o }}
run-name: ${{ inputs.i }}
env:
  E: ${{ secrets.TOKEN }}
jobs:
  a:
    runs-on: ${{ matrix.os }}
    if: ${{ needs.b.result == 'success' && always() }}
    strategy:
      matrix:
        os: [ubuntu-latest]
    env:
      F: ${{ secrets.TOKEN }}
    outputs:
      o: ${{ steps.s.outputs.o }}
    steps:
      - id: s
        run: echo ${{ env.E }} ${{ hashFiles('*.lock') }}
        if: steps.x.outcome == 'success' && failure()
  b:
    runs-on: ubuntu-latest
    steps:
      - run: echo
";
    assert_eq!(lint(text), Vec::<String>::new());
}

#[test]
fn every_placeholder_that_the_parser_reports_errors_in_is_checked_by_the_rule() {
    // Each expression reads a variable that does not exist, so the rule reports each one it
    // checks, whatever the workflow key.
    let text = EVERY_LOCATION.replace("${{ ", "${{ undefined_");
    let text = text
        .replace("if: env.", "if: undefined_env.")
        .replace("if: secrets.", "if: undefined_secrets.");
    let (file, diagnostics) = diagnostics(&text);
    let checked: BTreeSet<_> = diagnostics
        .iter()
        .filter(|d| d.rule.id == "undefined-variable")
        .map(|d| file.line_col(d.span.start))
        .map(|LineCol { line, column }| format!("{line}:{column}"))
        .collect();

    let document = workflint_yaml::parse(&file).document.unwrap();
    let parsed = workflint_syntax::parse(&file, &document);
    let mut walked = BTreeSet::new();
    workflint_syntax::placeholders(&parsed.workflow, &mut |source, placeholders| {
        for placeholder in placeholders {
            let expr = placeholder.expr.as_ref().unwrap();
            let span = source
                .source_span(&file, placeholder.to_value(expr.range()))
                .unwrap();
            let LineCol { line, column } = file.line_col(span.start);
            walked.insert(format!("{line}:{column}"));
        }
    });
    assert_eq!(checked, walked);
    assert_eq!(checked, expression_starts(&text));
}

#[test]
fn a_placeholder_with_a_parse_error_gets_no_diagnostic_from_the_rule() {
    let text = "\
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo ${{ a b }} ${{ github.foo( }}
        if: github.event ==
";
    let (_, diagnostics) = diagnostics(text);
    let rules: Vec<_> = diagnostics.iter().map(|d| d.rule.id).collect();
    assert_eq!(
        rules,
        [
            "invalid-expression-syntax",
            "invalid-expression-syntax",
            "invalid-expression-syntax"
        ]
    );
}

#[test]
fn an_invalid_value_gets_no_diagnostic_from_the_rule() {
    let text = "\
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    if: ''
    continue-on-error: [x]
    timeout-minutes: '${{ github.ref }} minutes'
    steps:
      - run: echo
        if: [a]
        env: x
";
    let (_, diagnostics) = diagnostics(text);
    assert!(!diagnostics.is_empty());
    for diagnostic in &diagnostics {
        assert_eq!(diagnostic.rule.kind, "syntax-check", "{diagnostic:?}");
    }
}

#[test]
fn every_expression_in_a_string_is_checked() {
    let text = "\
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo ${{ foo }} ${{ github.event.issue.title }} ${{ bar }}
";
    let lines = lint(text);
    let at: Vec<_> = lines
        .iter()
        .map(|line| {
            let (at, rest) = line
                .strip_prefix("test.yaml:")
                .unwrap()
                .split_once(": ")
                .unwrap();
            format!("{at} {}", rest.rsplit_once(' ').unwrap().1)
        })
        .collect();
    assert_eq!(
        at,
        [
            "6:23 [undefined-variable]",
            "6:33 [untrusted-input]",
            "6:66 [undefined-variable]",
        ]
    );
}

#[test]
fn an_error_inside_a_block_scalar_is_on_its_own_line() {
    let text = "\
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: |
          echo start
          echo ${{ github.event.pull_request.title }}
";
    let lines = lint(text);
    assert_eq!(lines.len(), 1, "{lines:#?}");
    assert!(
        lines[0].starts_with(
            "test.yaml:8:20: \"github.event.pull_request.title\" is potentially untrusted."
        ),
        "{lines:#?}"
    );
}

#[test]
fn untrusted_input_is_reported_in_run_and_in_the_script_of_github_script() {
    let text = "\
on: pull_request_target
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ github.head_ref }}'
      - uses: actions/github-script@v7
        with:
          script: console.log('${{ github.event.issue.body }}')
          other: ${{ github.event.issue.body }}
      - uses: someone/github-script@v7
        with:
          script: console.log('${{ github.event.issue.body }}')
      - run: echo
        env:
          TITLE: ${{ github.event.issue.title }}
        working-directory: ${{ github.head_ref }}
";
    let lines = lint(text);
    let at: Vec<_> = lines
        .iter()
        .map(|line| line.split(": ").next().unwrap())
        .collect();
    assert_eq!(at, ["test.yaml:6:24", "test.yaml:9:37"], "{lines:#?}");
}

#[test]
fn github_head_ref_in_a_condition_is_not_untrusted_input() {
    // rhysd/actionlint#443: only scripts are sinks.
    let text = "\
on: pull_request
jobs:
  test:
    runs-on: ubuntu-latest
    if: github.head_ref == 'main'
    steps:
      - run: echo
        if: ${{ startsWith(github.head_ref, 'release/') || github.head_ref == 'dev' }}
";
    assert_eq!(lint(text), Vec::<String>::new());
}

#[test]
fn an_object_array_or_null_in_a_template_is_reported_at_its_opening_braces() {
    let text = "\
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo ${{ github.event }} ${{ github.ref }}
        name: 'x ${{ fromJSON(''[1]'') }}'
        working-directory: ${{ null }}
      - run: echo
        if: github.event
";
    assert_eq!(
        lint(text),
        [
            "test.yaml:6:19: object, array, and null values should not be evaluated in template with ${{ }} but evaluating the value of type object [object-in-template]",
            "test.yaml:7:18: object, array, and null values should not be evaluated in template with ${{ }} but evaluating the value of type array<number> [object-in-template]",
            "test.yaml:8:28: object, array, and null values should not be evaluated in template with ${{ }} but evaluating the value of type null [object-in-template]",
        ]
    );
}

#[test]
fn a_condition_accepts_a_value_of_any_type() {
    let text = "\
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    if: github.event
    steps:
      - run: echo
        if: ${{ strategy.job-index }}
      - run: echo
        if: ${{ github.ref }}
      - run: echo
        if: '!github.event.commits'
";
    assert_eq!(lint(text), Vec::<String>::new());
}

#[test]
fn a_typed_value_must_have_its_type() {
    let text = "\
on: push
env: ${{ github.ref }}
concurrency:
  group: g
  cancel-in-progress: ${{ github.ref }}
jobs:
  test:
    runs-on: ${{ fromJSON('{}') }}
    continue-on-error: ${{ 'x' }}
    timeout-minutes: ${{ true }}
    strategy:
      max-parallel: ${{ 'x' }}
      fail-fast: ${{ 1 }}
    container:
      image: node
      credentials: ${{ 'x' }}
    services: ${{ 1 }}
    steps:
      - run: echo
        env: ${{ toJSON(github) }}
        timeout-minutes: ${{ fromJSON('1') }}
        continue-on-error: ${{ fromJSON('true') }}
";
    assert_eq!(
        lint(text),
        [
            r#"test.yaml:2:6: type of expression at "env" must be object but found type string [wrong-expression-type]"#,
            "test.yaml:5:23: type of expression must be bool but found type string [wrong-expression-type]",
            r#"test.yaml:8:14: type of expression at "runs-on" must be string or array but found type "{}" [wrong-expression-type]"#,
            "test.yaml:9:24: type of expression must be bool but found type string [wrong-expression-type]",
            r#"test.yaml:10:22: type of expression at "float number value" must be number but found type bool [wrong-expression-type]"#,
            r#"test.yaml:12:21: type of expression at "integer value" must be number but found type string [wrong-expression-type]"#,
            "test.yaml:13:18: type of expression must be bool but found type number [wrong-expression-type]",
            r#"test.yaml:16:20: type of expression at "credentials" must be object but found type string [wrong-expression-type]"#,
            r#"test.yaml:17:15: type of expression at "services" must be object but found type number [wrong-expression-type]"#,
            r#"test.yaml:20:14: type of expression at "env" must be object but found type string [wrong-expression-type]"#,
        ]
    );
}

#[test]
fn a_typed_value_with_an_error_gets_no_type_error() {
    let text = "\
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    continue-on-error: ${{ foo }}
    steps:
      - run: echo
";
    let lines = lint(text);
    assert_eq!(lines.len(), 1, "{lines:#?}");
    assert!(lines[0].ends_with("[undefined-variable]"), "{lines:#?}");
}

#[test]
fn contexts_typed_by_later_tasks_accept_any_property() {
    // M4.7 to M4.10 type these contexts. Until then they accept any property, so that no valid
    // workflow gets an error.
    let text = "\
on:
  workflow_dispatch:
    inputs:
      i:
        type: string
jobs:
  test:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        os: [linux]
    steps:
      - run: echo ${{ matrix.anything }} ${{ steps.any.outputs.x }} ${{ needs.any.result }}
      - run: echo ${{ inputs.anything }} ${{ secrets.anything }} ${{ github.event.inputs.anything }}
";
    assert_eq!(lint(text), Vec::<String>::new());
}
