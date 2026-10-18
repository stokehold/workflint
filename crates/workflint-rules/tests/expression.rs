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

/// Lints `workflow` and returns the diagnostics of the `expression` kind in the one-line format.
/// Other rules, such as `events`, report on some of the workflows here.
fn lint_expressions(workflow: &str) -> Vec<String> {
    lint(workflow)
        .into_iter()
        .filter(|line| {
            let id = line.rsplit_once(" [").unwrap().1.trim_end_matches(']');
            workflint_rules::registry::find(id).unwrap().kind == "expression"
        })
        .collect()
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
/// section 7.2. Each expression reads a
/// context that its workflow key does not allow. Where the key allows every context, it calls a
/// special function that the key does not allow instead. The locations with the empty workflow
/// key read `github`, which every other key allows.
const EVERY_LOCATION: &str = r"
name: ${{ github }}
run-name: ${{ env }}
on:
  push:
    types: ${{ github }}
    branches: ['${{ github }}']
    branches-ignore: ${{ github }}
    tags: ${{ github }}
    tags-ignore: ${{ github }}
    paths: ${{ github }}
    paths-ignore: ${{ github }}
    workflows: ${{ github }}
  schedule:
    - cron: ${{ github }}
      timezone: ${{ github }}
  workflow_dispatch:
    inputs:
      i:
        description: ${{ github }}
        default: ${{ github }}
        required: ${{ github }}
        type: choice
        options: ['${{ github }}']
  repository_dispatch:
    types: ['${{ github }}']
  workflow_call:
    inputs:
      i:
        description: ${{ github }}
        default: ${{ env }}
        type: string
    secrets:
      s:
        description: ${{ github }}
    outputs:
      o:
        description: ${{ github }}
        value: ${{ env }}
  image_version:
    names: ['${{ github }}']
    versions: ['${{ github }}']
env:
  ${{ env }}: ${{ env }}
defaults:
  run:
    shell: ${{ github }}
    working-directory: ${{ github }}
concurrency:
  group: ${{ env }}
  cancel-in-progress: ${{ env }}
jobs:
  a:
    name: ${{ env }}
    needs: ['${{ github }}']
    runs-on:
      labels: ['${{ env }}']
      group: ${{ env }}
    concurrency: ${{ env }}
    env: ${{ env }}
    defaults:
      run:
        shell: ${{ secrets }}
        working-directory: ${{ secrets }}
    if: ${{ env }}
    strategy:
      matrix:
        r: ['${{ env }}', {k: ['${{ env }}']}]
        e: ${{ env }}
        include:
          - r: ${{ env }}
          - ${{ env }}
        exclude:
          - r: [{k: '${{ env }}'}]
          - ${{ env }}
      fail-fast: ${{ env }}
      max-parallel: ${{ env }}
    continue-on-error: ${{ env }}
    timeout-minutes: ${{ env }}
    container:
      image: ${{ env }}
      credentials: ${{ job }}
      env:
        ${{ steps }}: ${{ steps }}
      ports: ['${{ env }}']
      volumes: ['${{ env }}']
      options: ${{ env }}
    services:
      redis:
        image: ${{ env }}
        credentials:
          username: ${{ job }}
          password: ${{ job }}
        command: ${{ env }}
        entrypoint: ${{ env }}
    environment:
      name: ${{ env }}
      url: ${{ secrets }}
      deployment: ${{ env }}
    outputs:
      o: ${{ hashFiles('job_output') }}
    snapshot:
      image-name: image
      if: ${{ env }}
    steps:
      - id: ${{ github }}
        name: ${{ success() }}
        if: ${{ secrets }}
        run: echo ${{ success() }}
        shell: ${{ matrix }}
        working-directory: ${{ success() }}
        env: ${{ success() }}
        continue-on-error: ${{ success() }}
        timeout-minutes: ${{ success() }}
      - uses: ${{ github }}
        with:
          i: ${{ success() }}
        env:
          ${{ success() }}: ${{ success() }}
      - uses: docker://alpine
        with:
          entrypoint: ${{ success() }}
          args: ${{ success() }}
  b:
    runs-on: ${{ env }}
    if: env
    concurrency:
      group: ${{ env }}
      cancel-in-progress: ${{ env }}
    env:
      ${{ env }}: ${{ env }}
    container: ${{ env }}
    services: ${{ env }}
    environment: ${{ env }}
    steps:
      - run: echo
        if: secrets
  c:
    runs-on: [linux, '${{ env }}']
    container:
      image: image
      credentials:
        username: ${{ job }}
        password: ${{ job }}
    services:
      redis:
        image: redis
        credentials: ${{ job }}
        env:
          ${{ steps }}: ${{ steps }}
        ports: ['${{ env }}']
        volumes: ['${{ env }}']
        options: ${{ env }}
      postgres:
        image: postgres
        env: ${{ steps }}
    snapshot: image
    steps:
      - run: echo
  d:
    uses: ${{ github }}
    with:
      i: ${{ env }}
    secrets:
      s: ${{ env }}
";

/// The forms of the workflow-level locations of [`EVERY_LOCATION`] that take a value in two
/// forms.
const WORKFLOW_SECOND_FORMS: &str = r"
on: push
env: ${{ job }}
concurrency: ${{ env }}
jobs:
  a:
    runs-on: linux
    strategy:
      matrix: ${{ env }}
    steps:
      - run: echo
  b:
    runs-on: linux
    strategy:
      matrix:
        include: ${{ env }}
        exclude: ${{ env }}
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
        let lines = lint_expressions(text);
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
    let lines = lint_expressions(EVERY_LOCATION);
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
    // Each expression reads a variable or calls a function that does not exist, so the rule
    // reports each one it checks, whatever the workflow key.
    let text = EVERY_LOCATION.replace("${{ ", "${{ undefined_");
    let text = text
        .replace("if: env\n", "if: undefined_env\n")
        .replace("if: secrets\n", "if: undefined_secrets\n");
    let (file, diagnostics) = diagnostics(&text);
    let checked: BTreeSet<_> = diagnostics
        .iter()
        .filter(|d| matches!(d.rule.id, "undefined-variable" | "undefined-function"))
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
    assert_ne!(diagnostics.len(), 0);
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
            "6:34 [untrusted-input]",
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
    // `other` is not an input of `actions/github-script@v7`, which the `action` rule reports.
    let lines = lint(text);
    let at: Vec<_> = lines
        .iter()
        .filter(|line| line.ends_with("[untrusted-input]"))
        .map(|line| line.split(": ").next().unwrap())
        .collect();
    assert_eq!(at, ["test.yaml:6:24", "test.yaml:9:36"], "{lines:#?}");
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
    // M4.9 and M4.10 type these contexts. Until then they accept any property, so that no valid
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
    steps:
      - run: echo ${{ steps.any.outputs.x }} ${{ needs.any.result }}
";
    assert_eq!(lint(text), Vec::<String>::new());
}

#[test]
fn inputs_has_the_inputs_of_workflow_dispatch_and_workflow_call() {
    let text = "\
on:
  workflow_dispatch:
    inputs:
      count:
        type: number
      Verbose:
        type: boolean
  workflow_call:
    inputs:
      count:
        type: string
      name:
        type: string
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo ${{ inputs.count }} ${{ inputs.verbose }} ${{ inputs.name }} ${{ inputs.other }}
      - run: echo ${{ github.event.inputs.verbose }} ${{ github.event.inputs.name }}
      - run: echo ${{ inputs.verbose.x }}
";
    assert_eq!(
        lint_expressions(text),
        [
            r#"test.yaml:18:84: property "other" is not defined in object type {count: string; name: string; verbose: bool} [undefined-property]"#,
            r#"test.yaml:19:58: property "name" is not defined in object type {count: string; verbose: string} [undefined-property]"#,
            r#"test.yaml:20:23: receiver of object dereference "x" must be type of object but got "bool" [invalid-property-access]"#,
        ]
    );
}

#[test]
fn a_workflow_call_default_sees_only_the_inputs_declared_before_it() {
    let text = "\
on:
  workflow_call:
    inputs:
      first:
        type: string
        default: ${{ inputs.second }}
      second:
        type: string
        default: ${{ inputs.first }}
      third:
        type: string
        default: ${{ inputs.third }}
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo ${{ inputs.first }} ${{ inputs.second }} ${{ inputs.third }}
";
    assert_eq!(
        lint_expressions(text),
        [
            r#"test.yaml:6:22: property "second" is not defined in object type {} [undefined-property]"#,
            r#"test.yaml:12:22: property "third" is not defined in object type {first: string; second: string} [undefined-property]"#,
        ]
    );
}

#[test]
fn a_boolean_or_number_input_whose_default_is_one_expression_of_another_type_is_reported() {
    let text = "\
on:
  workflow_call:
    inputs:
      name:
        type: string
      flag:
        type: boolean
        default: ${{ inputs.name }}
      count:
        type: number
        default: ${{ inputs.name }}
      other_flag:
        type: boolean
        default: ${{ inputs.flag }}
      any_count:
        type: number
        default: ${{ fromJSON(inputs.name) }}
      text:
        type: number
        default: x${{ inputs.name }}
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo
";
    assert_eq!(
        lint_expressions(text),
        [
            r#"test.yaml:8:18: type of input "flag" must be bool but found type string [wrong-input-default-type]"#,
            r#"test.yaml:11:18: type of input "count" must be number but found type string [wrong-input-default-type]"#,
        ]
    );
}

#[test]
fn secrets_has_only_the_declared_and_built_in_secrets_when_workflow_call_is_the_only_trigger() {
    let text = "\
on:
  workflow_call:
    secrets:
      token:
        required: true
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo ${{ secrets.token }} ${{ secrets.GITHUB_TOKEN }} ${{ secrets.other }}
";
    assert_eq!(
        lint_expressions(text),
        [
            r#"test.yaml:10:72: property "other" is not defined in object type {actions_runner_debug: string; actions_step_debug: string; github_token: string; token: string} [undefined-property]"#,
        ]
    );
}

#[test]
fn secrets_accepts_any_name_when_the_workflow_has_another_trigger() {
    let text = "\
on:
  push:
  workflow_call:
    secrets:
      token:
        required: true
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo ${{ secrets.token }} ${{ secrets.other }}
";
    assert_eq!(lint_expressions(text), Vec::<String>::new());
}

#[test]
fn secrets_accepts_any_name_when_workflow_call_declares_no_secrets() {
    let text = "\
on: workflow_call
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo ${{ secrets.other }}
";
    assert_eq!(lint_expressions(text), Vec::<String>::new());
}

#[test]
fn jobs_in_the_workflow_call_outputs_has_the_outputs_and_result_of_each_job() {
    let text = "\
on:
  workflow_call:
    outputs:
      tag:
        value: ${{ jobs.build.outputs.tag }}
      result:
        value: ${{ jobs.build.result }}
      called:
        value: ${{ jobs.call.outputs.anything }}
      typo:
        value: ${{ jobs.build.outputs.tags }}
      missing:
        value: ${{ jobs.deploy.result }}
jobs:
  build:
    runs-on: ubuntu-latest
    outputs:
      tag: ${{ steps.tag.outputs.tag }}
    steps:
      - id: tag
        run: echo
  call:
    uses: ./.github/workflows/called.yaml
";
    assert_eq!(
        lint_expressions(text),
        [
            r#"test.yaml:11:20: property "tags" is not defined in object type {tag: string} [undefined-property]"#,
            r#"test.yaml:13:20: property "deploy" is not defined in object type {build: {outputs: {tag: string}; result: string}; call: {outputs: object; result: string}} [undefined-property]"#,
        ]
    );
}

/// Returns the message that reading the property `x` of a value of type `ty` reports.
fn not_an_object(ty: &str) -> String {
    format!(
        r#"receiver of object dereference "x" must be type of object but got "{ty}" [invalid-property-access]"#
    )
}

/// Lints a job whose `matrix:` has the lines `matrix`, and whose step reads the property `x` of
/// each property `name` of `matrix` in `names`. Returns the messages without their positions.
fn matrix_messages(matrix: &[&str], names: &[&str]) -> Vec<String> {
    let matrix: String = matrix
        .iter()
        .flat_map(|line| ["        ", line, "\n"])
        .collect();
    let reads: Vec<String> = names
        .iter()
        .map(|name| format!("${{{{ matrix.{name}.x }}}}"))
        .collect();
    let text = format!(
        "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    strategy:\n      matrix:\n{matrix}    steps:\n      - run: echo {}\n",
        reads.join(" ")
    );
    lint(&text)
        .into_iter()
        .map(|line| line.splitn(4, ':').nth(3).unwrap().trim_start().to_owned())
        .collect()
}

#[test]
fn matrix_scalars_are_typed_from_their_tag_and_style() {
    let matrix = [
        r#"double: ["3.10"]"#,
        "single: ['true']",
        "number: [3.10]",
        "bool: [true]",
        "nil: [null]",
        "tagged: [!!str 1]",
        "plain: [ubuntu-latest]",
        "block:",
        "  - |",
        "    1",
    ];
    let names = [
        "double", "single", "number", "bool", "nil", "tagged", "plain", "block",
    ];
    let types = [
        "string", "string", "number", "bool", "null", "string", "string", "string",
    ];
    assert_eq!(
        matrix_messages(&matrix, &names),
        types.map(not_an_object).to_vec()
    );
}

#[test]
fn a_matrix_row_has_the_merged_type_of_its_values() {
    let matrix = [
        "mixed: [1, 'a', true]",
        "conflict: [1, true]",
        "objects: [{a: 1}, {b: 'x'}]",
        "arrays: [[1], []]",
        "expr: ${{ fromJSON('[1, 2]') }}",
    ];
    let names = ["mixed", "conflict", "objects.a", "arrays", "expr"];
    assert_eq!(
        matrix_messages(&matrix, &names),
        [
            not_an_object("string"),
            not_an_object("number"),
            not_an_object("array<any>"),
            not_an_object("number"),
        ]
    );
}

#[test]
fn include_adds_its_values_to_the_matrix_type() {
    let matrix = [
        "os: [linux]",
        "include:",
        "  - os: 1",
        "    extra: 'x'",
        r#"  - ${{ fromJSON('{"element":true}') }}"#,
    ];
    assert_eq!(
        matrix_messages(&matrix, &["os", "extra", "element", "nothing"]),
        [
            not_an_object("string"),
            not_an_object("string"),
            not_an_object("bool"),
            r#"property "nothing" is not defined in object type {element: bool; extra: string; os: string} [undefined-property]"#.to_owned(),
        ]
    );
}

#[test]
fn an_include_element_written_as_one_expression_is_checked() {
    let matrix = [
        "os: [linux]",
        "include:",
        "  - ${{ inputs.nothing }}",
        "  - ${{ 'not an object' }}",
    ];
    assert_eq!(
        matrix_messages(&matrix, &[]),
        [
            r#"property "nothing" is not defined in object type {} [undefined-property]"#,
            r#"type of expression at "matrix combination at element of include section" must be object but found type string [wrong-expression-type]"#,
        ]
    );
}

#[test]
fn a_whole_include_expression_merges_its_element_type() {
    let objects = [
        "os: [linux]",
        r#"include: ${{ fromJSON('[{"os":1,"extra":true}]') }}"#,
    ];
    assert_eq!(
        matrix_messages(&objects, &["os", "extra"]),
        [not_an_object("string"), not_an_object("bool")]
    );
    // Any other type makes the matrix accept every property.
    let unknown = ["os: [linux]", "include: ${{ fromJSON(github.event_name) }}"];
    assert_eq!(
        matrix_messages(&unknown, &["os", "anything"]),
        Vec::<String>::new()
    );
}

#[test]
fn a_whole_matrix_expression_gives_its_object_type() {
    let literal = [r#"${{ fromJSON('{"os":["linux"],"include":[{"extra":1}],"exclude":[]}') }}"#];
    assert_eq!(
        matrix_messages(&literal, &["os", "extra", "exclude"]),
        [
            // As in the reference, a row keeps the type of the whole array.
            not_an_object("array<string>"),
            not_an_object("number"),
            r#"property "exclude" is not defined in object type {extra: number; os: array<string>} [undefined-property]"#.to_owned(),
        ]
    );
    let unknown = ["${{ fromJSON(github.event_name) }}"];
    assert_eq!(
        matrix_messages(&unknown, &["anything"]),
        Vec::<String>::new()
    );
}

#[test]
fn every_job_key_sees_the_matrix_type() {
    let text = "\
on: push
jobs:
  test:
    name: ${{ matrix.nothing }}
    runs-on: ${{ matrix.nothing }}
    container: ${{ matrix.nothing }}
    strategy:
      matrix:
        os: [linux]
    steps:
      - run: echo ${{ matrix.nothing }}
";
    let lines = lint(text);
    assert_eq!(lines.len(), 4, "{lines:#?}");
    for line in lines {
        assert!(
            line.ends_with(
                r#"property "nothing" is not defined in object type {os: string} [undefined-property]"#
            ),
            "{line}"
        );
    }
}

#[test]
fn a_matrix_value_nested_10000_levels_deep_is_typed_on_a_small_stack() {
    // The row is the outermost of the 10,000 flow levels that the YAML layer accepts.
    const DEPTH: usize = 10_000;
    let lines = std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(|| {
            let row = format!(
                "{}'${{{{ nothing }}}}'{}",
                "[".repeat(DEPTH),
                "]".repeat(DEPTH)
            );
            matrix_messages(&[&format!("deep: {row}")], &["deep"])
        })
        .unwrap()
        .join()
        .unwrap();
    // The type nests 128 levels deep and then is `any`.
    let ty = format!("{}any{}", "array<".repeat(128), ">".repeat(128));
    assert_eq!(lines.len(), 2, "{lines:#?}");
    assert!(
        lines[0].starts_with(r#"undefined variable "nothing""#),
        "{}",
        lines[0]
    );
    assert_eq!(lines[1], not_an_object(&ty));
}
