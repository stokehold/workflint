//! The `expression` rule: every location that the reference's rule checks, with its workflow key,
//! its expected type, and the untrusted-input check of scripts.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeSet;

use workflint_diagnostics::{
    Color, Diagnostic, FileId, LineCol, SourceFile, render_oneline, sort_and_dedup,
};
use workflint_rules::local::{LocalMetadata, MemoryProject, NoProject};

/// Lints `workflow` as the binary does, without the file system, and returns the sorted
/// diagnostics with the file.
fn diagnostics(workflow: &str) -> (SourceFile, Vec<Diagnostic>) {
    diagnostics_in(&NoProject, workflow)
}

/// Does what [`diagnostics`] does, in a project with the local actions of `project`.
fn diagnostics_in(project: &dyn LocalMetadata, workflow: &str) -> (SourceFile, Vec<Diagnostic>) {
    let file = SourceFile::new(FileId::new(0), "test.yaml", workflow).unwrap();
    let parsed = workflint_yaml::parse(&file);
    let mut diagnostics = parsed.diagnostics;
    let document = parsed.document.unwrap();
    let syntax = workflint_syntax::parse(&file, &document);
    diagnostics.extend(syntax.diagnostics);
    diagnostics.extend(workflint_rules::run_in_project(
        &file,
        &syntax.workflow,
        project,
    ));
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
    lint_in(&NoProject, workflow)
}

/// Does what [`lint`] does, in a project with the local actions of `project`.
fn lint_in(project: &dyn LocalMetadata, workflow: &str) -> Vec<String> {
    let (file, diagnostics) = diagnostics_in(project, workflow);
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
  ${{ insert }}: ${{ job }}
defaults:
  run:
    shell: ${{ github }}
    working-directory: ${{ github }}
concurrency:
  group: ${{ env }}
  cancel-in-progress: ${{ env }}
  ${{ insert }}: ${{ env }}
jobs:
  a:
    name: ${{ env }}
    needs: ['${{ github }}']
    runs-on:
      labels: ['${{ env }}']
      group: ${{ env }}
      ${{ insert }}: ${{ env }}
    concurrency: ${{ env }}
    env: ${{ env }}
    defaults:
      run:
        shell: ${{ secrets }}
        working-directory: ${{ secrets }}
        ${{ insert }}: ${{ secrets }}
    if: ${{ env }}
    strategy:
      matrix:
        r: ['${{ env }}', {k: ['${{ env }}']}]
        e: ${{ env }}
        ${{ insert }}: ${{ env }}
        include:
          - r: ${{ env }}
            ${{ insert }}: ${{ env }}
          - ${{ env }}
        exclude:
          - r: [{k: '${{ env }}'}]
            ${{ insert }}: ${{ env }}
          - ${{ env }}
      fail-fast: ${{ env }}
      max-parallel: ${{ env }}
      ${{ insert }}: ${{ env }}
    continue-on-error: ${{ env }}
    timeout-minutes: ${{ env }}
    container:
      image: ${{ env }}
      credentials: ${{ job }}
      env:
        ${{ steps }}: ${{ steps }}
        ${{ insert }}: ${{ steps }}
      ports: ['${{ env }}']
      volumes: ['${{ env }}']
      options: ${{ env }}
      ${{ insert }}: ${{ env }}
    services:
      redis:
        image: ${{ env }}
        credentials:
          username: ${{ job }}
          password: ${{ job }}
          ${{ insert }}: ${{ job }}
        command: ${{ env }}
        entrypoint: ${{ env }}
        ${{ insert }}: ${{ env }}
    environment:
      name: ${{ env }}
      url: ${{ secrets }}
      deployment: ${{ env }}
      ${{ insert }}: ${{ env }}
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
          ${{ insert }}: ${{ success() }}
        env:
          ${{ success() }}: ${{ success() }}
          ${{ insert }}: ${{ success() }}
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
      ${{ insert }}: ${{ env }}
    env:
      ${{ env }}: ${{ env }}
      ${{ insert }}: ${{ env }}
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
        ${{ insert }}: ${{ job }}
    services:
      ${{ insert }}: ${{ env }}
      redis:
        image: redis
        credentials: ${{ job }}
        env:
          ${{ steps }}: ${{ steps }}
          ${{ insert }}: ${{ steps }}
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

/// The key whose value GitHub merges into its mapping. It is not an expression of its own.
const INSERT: &str = "${{ insert }}";

/// Returns the position of the first token of each expression in `text`: just after `${{ `, or
/// after `if: ` for a condition written without `${{ }}`. An `${{ insert }}` key is not one.
fn expression_starts(text: &str) -> BTreeSet<String> {
    let explicit = text
        .match_indices("${{ ")
        .filter(|(at, _)| !text[*at..].starts_with(INSERT))
        .map(|(at, _)| at + 4);
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
    needs: b
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
      - id: x
        run: echo
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
    let text = EVERY_LOCATION
        .replace("${{ ", "${{ undefined_")
        .replace("${{ undefined_insert }}", INSERT);
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

/// The paths below `github.event.workflow_run` that the reference misses
/// (`docs/differences.md#workflow-run-untrusted-input`).
const WORKFLOW_RUN_UNTRUSTED_INPUTS: [&str; 9] = [
    "github.event.workflow_run.head_branch",
    "github.event.workflow_run.display_title",
    "github.event.workflow_run.head_commit.message",
    "github.event.workflow_run.head_commit.author.email",
    "github.event.workflow_run.head_commit.author.name",
    "github.event.workflow_run.head_commit.committer.email",
    "github.event.workflow_run.head_commit.committer.name",
    "github.event.workflow_run.head_repository.description",
    "github.event.workflow_run.head_repository.owner.login",
];

/// Lints a `workflow_run` workflow with one step made by `step`, which gets the path, and returns
/// the `untrusted-input` diagnostics.
fn workflow_run_untrusted_inputs(path: &str, step: impl Fn(&str) -> String) -> Vec<String> {
    let text = format!(
        "\
on:
  workflow_run:
    workflows: [CI]
    types: [completed]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
{}",
        step(path)
    );
    lint(&text)
        .into_iter()
        .filter(|line| line.ends_with("[untrusted-input]"))
        .collect()
}

#[test]
fn the_head_branch_and_head_commit_of_workflow_run_are_untrusted_in_run() {
    for path in WORKFLOW_RUN_UNTRUSTED_INPUTS {
        let lines = workflow_run_untrusted_inputs(path, |path| {
            format!("      - run: echo '${{{{ {path} }}}}'\n")
        });
        assert_eq!(lines.len(), 1, "{path}: {lines:#?}");
        assert!(
            lines[0].starts_with(&format!(
                "test.yaml:9:24: \"{path}\" is potentially untrusted."
            )),
            "{path}: {lines:#?}"
        );
    }
}

#[test]
fn the_head_branch_and_head_commit_of_workflow_run_are_untrusted_in_github_script() {
    for path in WORKFLOW_RUN_UNTRUSTED_INPUTS {
        let lines = workflow_run_untrusted_inputs(path, |path| {
            format!(
                "      - uses: actions/github-script@v7\n        with:\n          script: console.log('${{{{ {path} }}}}')\n"
            )
        });
        assert_eq!(lines.len(), 1, "{path}: {lines:#?}");
        assert!(
            lines[0].starts_with(&format!(
                "test.yaml:11:36: \"{path}\" is potentially untrusted."
            )),
            "{path}: {lines:#?}"
        );
    }
}

#[test]
fn the_head_branch_and_head_commit_of_workflow_run_are_not_untrusted_in_a_condition() {
    for path in WORKFLOW_RUN_UNTRUSTED_INPUTS {
        let lines = workflow_run_untrusted_inputs(path, |path| {
            format!("      - run: echo\n        if: ${{{{ {path} == 'main' }}}}\n")
        });
        assert_eq!(lines, Vec::<String>::new(), "{path}");
    }
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
fn secrets_has_only_the_declared_secrets_when_the_other_trigger_is_an_unknown_event() {
    let text = "\
on:
  workflow_call:
    secrets:
      tok:
        required: true
  unknown_event:
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo ${{ secrets.other }}
";
    assert_eq!(
        lint_expressions(text),
        [
            r#"test.yaml:11:23: property "other" is not defined in object type {actions_runner_debug: string; actions_step_debug: string; github_token: string; tok: string} [undefined-property]"#,
        ]
    );
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

/// Lints the steps `steps` of one job, in a project with the local action
/// `./.github/actions/greet`, whose outputs are `greeting` and `fullName`, and the local action
/// `./.github/actions/broken`, whose metadata does not decode. Returns the messages of the
/// `expression` kind, without positions.
fn steps_messages(steps: &str) -> Vec<String> {
    let project = MemoryProject::new()
        .with_file(
            ".github/actions/greet/action.yml",
            "name: Greet\ndescription: d\noutputs:\n  greeting:\n    description: d\n  fullName:\n    description: d\nruns:\n  using: node24\n  main: index.js\n",
        )
        .with_file(".github/actions/greet/index.js", "")
        .with_file(".github/actions/broken/action.yml", "outputs: x\n")
        .linting(".github/workflows/test.yaml");
    let text = format!("on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n{steps}");
    lint_in(&project, &text)
        .into_iter()
        .filter(|line| {
            let id = line.rsplit_once(" [").unwrap().1.trim_end_matches(']');
            workflint_rules::registry::find(id).unwrap().kind == "expression"
        })
        .map(|line| line.splitn(4, ':').nth(3).unwrap().trim().to_owned())
        .collect()
}

#[test]
fn a_step_sees_only_the_steps_before_it() {
    let messages = steps_messages(
        "      - run: echo ${{ steps.a.outcome }}
      - id: a
        run: echo ${{ steps.a.outcome }}
      - run: echo ${{ steps.a.outcome }} ${{ steps.a.conclusion }} ${{ steps.b.outcome }}
      - id: b
        run: echo
",
    );
    assert_eq!(
        messages,
        [
            r#"property "a" is not defined in object type {} [undefined-property]"#,
            r#"property "a" is not defined in object type {} [undefined-property]"#,
            r#"property "b" is not defined in object type {a: {conclusion: string; outcome: string; outputs: {string => string}}} [undefined-property]"#,
        ]
    );
}

#[test]
fn each_job_starts_with_no_steps_and_its_outputs_see_every_step() {
    let text = "\
on: push
jobs:
  first:
    runs-on: ubuntu-latest
    environment:
      name: e
      url: ${{ steps.a.outputs.x }}
    outputs:
      x: ${{ steps.a.outputs.x }}
    steps:
      - id: a
        run: echo
  second:
    runs-on: ubuntu-latest
    steps:
      - run: echo ${{ steps.a.outputs.x }}
";
    assert_eq!(
        lint_expressions(text),
        [r#"test.yaml:16:23: property "a" is not defined in object type {} [undefined-property]"#]
    );
}

#[test]
fn an_id_with_an_expression_makes_steps_accept_any_step() {
    let messages = steps_messages(
        "      - run: echo ${{ steps.a.outcome }}
      - id: a-${{ 'b' }}
        run: echo
      - run: echo ${{ steps.a.outcome }} ${{ steps.anything.outputs.x }}
",
    );
    assert_eq!(
        messages,
        [r#"property "a" is not defined in object type {} [undefined-property]"#]
    );
}

#[test]
fn step_ids_and_output_names_are_case_insensitive() {
    let messages = steps_messages(
        "      - id: MyGreet
        uses: ./.github/actions/greet
      - id: Project
        uses: actions/add-to-project@v1.0.1
        with:
          project-url: u
          github-token: t
      - run: echo ${{ steps.mygreet.outputs.fullName }} ${{ steps.MYGREET.outputs.FULLNAME }} ${{ steps['MyGreet'].outputs['fullname'] }}
      - run: echo ${{ steps.project.outputs.itemId }} ${{ steps.PROJECT.outputs.ITEMID }}
      - run: echo ${{ steps.mygreet.outputs.full_name }}
",
    );
    assert_eq!(
        messages,
        [
            r#"property "full_name" is not defined in object type {fullname: string; greeting: string} [undefined-property]"#
        ]
    );
}

#[test]
fn outputs_come_from_the_action_that_the_step_runs() {
    let messages = steps_messages(
        "      - id: run
        run: echo
      - id: local
        uses: ./.github/actions/greet
      - id: missing
        uses: ./.github/actions/missing
      - id: popular
        uses: actions/upload-artifact@v4
        with:
          path: p
      - id: script
        uses: actions/github-script@v7
        with:
          script: s
      - id: skip
        uses: google-github-actions/get-secretmanager-secrets@v2
        with:
          secrets: s
      - id: unknown
        uses: example/unknown@v1
      - run: echo ${{ steps.run.outputs.x }} ${{ steps.missing.outputs.x }} ${{ steps.unknown.outputs.x }}
      - run: echo ${{ steps.script.outputs.x }} ${{ steps.skip.outputs.x }}
      - run: echo ${{ steps.local.outputs.greeting }} ${{ steps.popular.outputs.artifact-id }}
      - run: echo ${{ steps.local.outputs.x }}
      - run: echo ${{ steps.popular.outputs.x }}
      - run: echo ${{ steps.run.outputs.x.y }}
      - run: echo ${{ steps.script.outputs }}
",
    );
    assert_eq!(
        messages,
        [
            r#"property "x" is not defined in object type {fullname: string; greeting: string} [undefined-property]"#,
            r#"property "x" is not defined in object type {artifact-digest: string; artifact-id: string; artifact-url: string} [undefined-property]"#,
            r#"receiver of object dereference "y" must be type of object but got "string" [invalid-property-access]"#,
            "object, array, and null values should not be evaluated in template with ${{ }} but evaluating the value of type object [object-in-template]",
        ]
    );
}

#[test]
fn a_local_action_with_broken_metadata_has_outputs_of_any_name() {
    // The `action` rule reports the broken metadata. This rule reports nothing.
    let messages = steps_messages(
        "      - id: broken
        uses: ./.github/actions/broken
      - run: echo ${{ steps.broken.outputs.x }} ${{ steps.broken.outputs.x.y }}
",
    );
    assert_eq!(
        messages,
        [
            r#"receiver of object dereference "y" must be type of object but got "string" [invalid-property-access]"#
        ]
    );
}

/// Lints `workflow` in a project with the local reusable workflows `callee`, at
/// `./.github/workflows/callee.yaml`, and `./.github/workflows/broken.yaml`, which has no
/// `workflow_call` event. Returns the diagnostics of the `expression` kind and of the errors of
/// local reusable workflows, which this rule reports when its lookup comes first.
fn call_lines(callee: &str, workflow: &str) -> Vec<String> {
    let project = MemoryProject::new()
        .with_file(".github/workflows/callee.yaml", callee)
        .with_file(".github/workflows/broken.yaml", "on: push\njobs: {}\n")
        .linting(".github/workflows/test.yaml");
    lint_in(&project, workflow)
        .into_iter()
        .filter(|line| {
            let id = line.rsplit_once(" [").unwrap().1.trim_end_matches(']');
            workflint_rules::registry::find(id).unwrap().kind == "expression"
                || id.ends_with("reusable-workflow")
                || id == "reusable-workflow-not-found"
        })
        .collect()
}

#[test]
fn needs_has_the_outputs_and_result_of_each_direct_dependency() {
    let text = "\
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    outputs:
      Version: ${{ steps.v.outputs.version }}
    steps:
      - id: v
        run: echo
  test:
    needs: [Build]
    runs-on: ubuntu-latest
    steps:
      - run: echo ${{ needs.build.outputs.version }} ${{ needs.BUILD.result }}
      - run: echo ${{ needs.build.outputs.tag }}
      - run: echo ${{ needs.deploy }}
  other:
    runs-on: ubuntu-latest
    steps:
      - run: echo ${{ needs.build }}
";
    assert_eq!(
        lint_expressions(text),
        [
            r#"test.yaml:15:23: property "tag" is not defined in object type {version: string} [undefined-property]"#,
            r#"test.yaml:16:23: property "deploy" is not defined in object type {build: {outputs: {version: string}; result: string}} [undefined-property]"#,
            r#"test.yaml:20:23: property "build" is not defined in object type {} [undefined-property]"#,
        ]
    );
}

#[test]
fn needs_does_not_have_the_dependencies_of_a_dependency() {
    // rhysd/actionlint#151: GitHub gives a job only the jobs it lists in `needs`.
    let text = "\
on: push
jobs:
  first:
    runs-on: ubuntu-latest
    outputs:
      first: x
    steps:
      - run: echo
  second:
    needs: first
    runs-on: ubuntu-latest
    steps:
      - run: echo
  third:
    needs: [second]
    runs-on: ubuntu-latest
    steps:
      - run: echo ${{ needs.second.result }} ${{ needs.first.outputs.first }}
";
    assert_eq!(
        lint_expressions(text),
        [
            r#"test.yaml:18:50: property "first" is not defined in object type {second: {outputs: {}; result: string}} [undefined-property]"#
        ]
    );
}

#[test]
fn needs_is_typed_before_the_matrix() {
    let text = "\
on: push
jobs:
  first:
    runs-on: ubuntu-latest
    outputs:
      os: x
    steps:
      - run: echo
  second:
    needs: first
    strategy:
      matrix:
        os: ${{ fromJSON(needs.first.outputs.os) }}
        bad: ['${{ needs.first.outputs.arch }}']
    runs-on: ubuntu-latest
    steps:
      - run: echo
";
    assert_eq!(
        lint_expressions(text),
        [
            r#"test.yaml:14:20: property "arch" is not defined in object type {os: string} [undefined-property]"#
        ]
    );
}

/// A local reusable workflow with the outputs `Version` and `digest`.
const CALLEE_WITH_OUTPUTS: &str = "\
on:
  workflow_call:
    outputs:
      Version:
        value: ${{ jobs.build.outputs.version }}
      digest:
        value: ${{ jobs.build.outputs.digest }}
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo
";

#[test]
fn a_reusable_workflow_job_has_the_outputs_of_its_callee() {
    let text = "\
on: push
jobs:
  build:
    uses: ./.github/workflows/callee.yaml
  remote:
    uses: owner/repo/.github/workflows/build.yaml@v1
  test:
    needs: [build, remote]
    runs-on: ubuntu-latest
    steps:
      - run: echo ${{ needs.build.outputs.version }} ${{ needs.build.outputs.DIGEST }}
      - run: echo ${{ needs.build.outputs.tag }}
      - run: echo ${{ needs.remote.outputs.anything }} ${{ needs.remote.outputs.x.y }}
";
    assert_eq!(
        call_lines(CALLEE_WITH_OUTPUTS, text),
        [
            r#"test.yaml:12:23: property "tag" is not defined in object type {digest: string; version: string} [undefined-property]"#,
            r#"test.yaml:13:60: receiver of object dereference "y" must be type of object but got "string" [invalid-property-access]"#,
        ]
    );
}

#[test]
fn a_broken_callee_of_a_needed_later_job_is_reported_once() {
    // `first` names `later` in `needs` before the `workflow-call` rule visits `later`, so this
    // rule looks the callee up first and reports its error. The outputs have any name.
    let text = "\
on: push
jobs:
  first:
    needs: [later, missing]
    runs-on: ubuntu-latest
    steps:
      - run: echo ${{ needs.later.outputs.x }} ${{ needs.missing.outputs.x }}
  later:
    uses: ./.github/workflows/broken.yaml
  missing:
    uses: ./.github/workflows/missing.yaml
";
    assert_eq!(
        call_lines(CALLEE_WITH_OUTPUTS, text),
        [
            r#"test.yaml:9:11: error while parsing reusable workflow "./.github/workflows/broken.yaml": "workflow_call" event trigger is not found in "on:" at line:1, column:5 [broken-reusable-workflow]"#,
            r#"test.yaml:11:11: could not read reusable workflow file for "./.github/workflows/missing.yaml": open .github/workflows/missing.yaml: no such file or directory [reusable-workflow-not-found]"#,
        ]
    );
}

/// A local reusable workflow with one input of each type, and one without a type.
const CALLEE_WITH_INPUTS: &str = "\
on:
  workflow_call:
    inputs:
      Str:
        type: string
      num:
        type: number
      flag:
        type: boolean
      untyped:
        description: no type
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo
";

/// Calls the local reusable workflow [`CALLEE_WITH_INPUTS`] with the `with:` lines `with`, and
/// returns the lines that [`call_lines`] returns.
fn with_lines(with: &str) -> Vec<String> {
    let text = format!(
        "on: push\njobs:\n  call:\n    uses: ./.github/workflows/callee.yaml\n    with:\n{with}"
    );
    call_lines(CALLEE_WITH_INPUTS, &text)
}

#[test]
fn a_quoted_true_passed_to_a_string_input_is_a_string() {
    // rhysd/actionlint#225 (`docs/differences.md#quoted-scalar-types`).
    assert_eq!(
        with_lines("      str: 'true'\n      num: \"42\"\n      flag: \"false\"\n"),
        [
            r#"test.yaml:7:12: input "num" is typed as number by reusable workflow "./.github/workflows/callee.yaml". string value cannot be assigned [wrong-workflow-call-input-type]"#,
            r#"test.yaml:8:13: input "flag" is typed as bool by reusable workflow "./.github/workflows/callee.yaml". string value cannot be assigned [wrong-workflow-call-input-type]"#,
        ]
    );
}

#[test]
fn a_plain_literal_has_the_type_of_its_tag() {
    assert_eq!(
        with_lines("      STR: true\n      num: 1.5e3\n      flag: True\n      untyped: null\n"),
        [
            r#"test.yaml:6:12: input "Str" is typed as string by reusable workflow "./.github/workflows/callee.yaml". bool value cannot be assigned [wrong-workflow-call-input-type]"#,
        ]
    );
    assert_eq!(
        with_lines("      str: 42\n      num: 0x1F\n      flag: ~\n"),
        [
            r#"test.yaml:8:13: input "flag" is typed as bool by reusable workflow "./.github/workflows/callee.yaml". null value cannot be assigned [wrong-workflow-call-input-type]"#,
        ]
    );
}

#[test]
fn a_boolean_input_rejects_a_value_of_another_type() {
    // `docs/differences.md#bool-assignability`: the reference accepts any value here.
    assert_eq!(
        with_lines("      flag: yes\n"),
        [
            r#"test.yaml:6:13: input "flag" is typed as bool by reusable workflow "./.github/workflows/callee.yaml". string value cannot be assigned [wrong-workflow-call-input-type]"#,
        ]
    );
}

#[test]
fn an_empty_value_is_null() {
    assert_eq!(
        with_lines("      str:\n      untyped:\n"),
        [
            r#"test.yaml:6:11: input "Str" is typed as string by reusable workflow "./.github/workflows/callee.yaml". null value cannot be assigned [wrong-workflow-call-input-type]"#,
        ]
    );
}

#[test]
fn a_value_written_as_one_expression_has_the_type_of_the_expression() {
    assert_eq!(
        with_lines(
            "      str: ${{ true }}\n      num: \" ${{ 'x' }} \"\n      flag: ${{ fromJSON('true') }}\n"
        ),
        [
            r#"test.yaml:6:12: input "Str" is typed as string by reusable workflow "./.github/workflows/callee.yaml". bool value cannot be assigned [wrong-workflow-call-input-type]"#,
            r#"test.yaml:7:12: input "num" is typed as number by reusable workflow "./.github/workflows/callee.yaml". string value cannot be assigned [wrong-workflow-call-input-type]"#,
        ]
    );
}

#[test]
fn a_value_with_text_around_an_expression_is_a_string() {
    assert_eq!(
        with_lines("      str: v${{ 1 }}\n      num: ${{ 1 }}${{ 2 }}\n"),
        [
            r#"test.yaml:7:12: input "num" is typed as number by reusable workflow "./.github/workflows/callee.yaml". string value cannot be assigned [wrong-workflow-call-input-type]"#,
        ]
    );
}

#[test]
fn a_value_whose_expression_has_errors_is_not_type_checked() {
    let lines = with_lines("      num: ${{ github.nope.x }}\n");
    assert_eq!(lines.len(), 1, "{lines:#?}");
    assert!(lines[0].ends_with("[undefined-property]"), "{lines:#?}");
}

#[test]
fn an_input_that_the_callee_does_not_declare_is_not_type_checked() {
    // The `workflow-call` rule reports the input.
    assert_eq!(with_lines("      other: true\n"), Vec::<String>::new());
}

#[test]
fn the_values_of_a_remote_call_are_not_type_checked() {
    let text = "\
on: push
jobs:
  call:
    uses: owner/repo/.github/workflows/callee.yaml@v1
    with:
      num: x
";
    assert_eq!(call_lines(CALLEE_WITH_INPUTS, text), Vec::<String>::new());
}

#[test]
fn an_undefined_name_carries_the_closest_name_as_help() {
    let (_, diagnostics) = diagnostics(
        "\
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ gihub.sha }}'
        if: always
",
    );
    let helps: Vec<_> = diagnostics
        .iter()
        .map(|d| (d.rule.id, d.help.as_deref()))
        .collect();
    assert_eq!(
        helps,
        [
            ("undefined-variable", Some("did you mean \"github\"?")),
            ("undefined-variable", Some("did you mean \"always()\"?")),
        ]
    );
}

/// A diagnostic as its position, its rule ID, and the position and note of each label.
type Labelled = (String, &'static str, Vec<(String, String)>);

/// Lints `workflow` and returns each diagnostic of the `expression` kind as a [`Labelled`].
fn labelled(workflow: &str) -> Vec<Labelled> {
    let (file, diagnostics) = diagnostics(workflow);
    let at = |offset: u32| {
        let pos = file.line_col(offset);
        format!("{}:{}", pos.line, pos.column)
    };
    diagnostics
        .iter()
        .filter(|d| d.rule.kind == "expression")
        .map(|d| {
            let labels = d
                .labels
                .iter()
                .map(|label| (at(label.span.start), label.note.clone()))
                .collect();
            (at(d.span.start), d.rule.id, labels)
        })
        .collect()
}

/// Owns the labels of [`labelled`] written as string slices.
fn labels(labels: &[(&str, &str)]) -> Vec<(String, String)> {
    labels
        .iter()
        .map(|&(at, note)| (at.to_owned(), note.to_owned()))
        .collect()
}

#[test]
fn an_error_reached_through_an_alias_is_labelled_at_the_alias() {
    // `steps` is available in a step's `env`, but not in a job's.
    let text = "\
on: push
jobs:
  a:
    runs-on: ubuntu-latest
    steps:
      - run: echo
        env:
          A: &v ${{ toJSON(steps) }}
  b:
    runs-on: ubuntu-latest
    env:
      B: *v
    steps:
      - run: echo
";
    assert_eq!(
        labelled(text),
        [(
            "8:28".to_owned(),
            "context-not-allowed",
            labels(&[("12:10", "reached through alias *v")]),
        )]
    );
}

#[test]
fn an_error_reached_through_two_aliases_is_labelled_at_both() {
    let text = "\
on: push
jobs:
  a:
    runs-on: ubuntu-latest
    steps:
      - run: echo
        env:
          A: &v ${{ toJSON(steps) }}
      - run: echo
        env: &env
          B: *v
  b:
    runs-on: ubuntu-latest
    env: *env
    steps:
      - run: echo
";
    assert_eq!(
        labelled(text),
        [(
            "8:28".to_owned(),
            "context-not-allowed",
            labels(&[
                ("14:10", "reached through alias *env"),
                ("11:14", "reached through alias *v"),
            ]),
        )]
    );
}

#[test]
fn an_error_repeated_through_aliases_is_reported_once_with_a_label_per_alias() {
    let text = "\
on: push
jobs:
  a:
    runs-on: ubuntu-latest
    steps:
      - run: echo
        env:
          A: &v ${{ toJSON(steps) }}
      - run: echo
        env: &env
          B: *v
  b:
    runs-on: ubuntu-latest
    env: *env
    steps:
      - run: echo
  c:
    runs-on: ubuntu-latest
    env: *env
    steps:
      - run: echo
";
    assert_eq!(
        labelled(text),
        [(
            "8:28".to_owned(),
            "context-not-allowed",
            labels(&[
                ("14:10", "reached through alias *env"),
                ("11:14", "reached through alias *v"),
                ("19:10", "reached through alias *env"),
            ]),
        )]
    );
}

#[test]
fn an_error_in_an_anchored_node_used_directly_has_no_label() {
    let text = "\
on: push
jobs:
  b:
    runs-on: ubuntu-latest
    env:
      B: &v ${{ toJSON(steps) }}
    steps:
      - run: echo
        env:
          A: *v
";
    assert_eq!(
        labelled(text),
        [("6:24".to_owned(), "context-not-allowed", Vec::new())]
    );
}
