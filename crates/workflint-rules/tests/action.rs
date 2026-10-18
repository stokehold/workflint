//! The `action` rule on repository actions, Docker actions, and popular actions.

#![allow(clippy::unwrap_used, clippy::panic)]

use workflint_diagnostics::{Color, FileId, SourceFile, render_oneline, sort_and_dedup};

/// Parses `text` as a workflow, runs the rules over it, and returns the diagnostics of the parser
/// and the rules in the one-line format.
fn lint(text: &str) -> Vec<String> {
    let file = SourceFile::new(FileId::new(0), "test.yaml", text).unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    let parsed = workflint_syntax::parse(&file, &document);
    let mut diagnostics = parsed.diagnostics.clone();
    diagnostics.extend(workflint_rules::run(&file, &parsed.workflow));
    sort_and_dedup(&file, &mut diagnostics);
    let mut out = Vec::new();
    render_oneline(&mut out, &file, &diagnostics, Color::Off).unwrap();
    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Lints a workflow whose only step is `step`, a YAML mapping indented for a step.
fn lint_step(step: &str) -> Vec<String> {
    lint(&format!(
        "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - {step}\n"
    ))
}

#[test]
fn an_empty_or_non_string_uses_gets_only_the_parser_error() {
    assert_eq!(
        lint_step("uses: ''"),
        ["test.yaml:6:15: string should not be empty [empty-string]"]
    );
    assert_eq!(
        lint_step("uses: [x]"),
        [
            "test.yaml:6:15: expected scalar node for string value but found sequence node with \"!!seq\" tag [unexpected-node-kind]"
        ]
    );
}

#[test]
fn uses_with_an_expression_or_a_local_path_is_not_checked_here() {
    assert_eq!(
        lint_step("uses: actions/checkout@${{ 'v2' }}"),
        [] as [&str; 0]
    );
    assert_eq!(lint_step("uses: ./no-ref-needed"), [] as [&str; 0]);
}

#[test]
fn an_action_with_empty_parts_gets_the_format_error() {
    assert_eq!(
        lint_step("uses: actions/@v4"),
        [
            "test.yaml:6:15: specifying action \"actions/@v4\" in invalid format because owner and repo and ref should not be empty. available formats are \"{owner}/{repo}@{ref}\" or \"{owner}/{repo}/{path}@{ref}\" [invalid-action-format]"
        ]
    );
}

#[test]
fn a_docker_uri_that_go_cannot_parse_gets_the_url_parse_error_and_the_tag() {
    assert_eq!(
        lint_step("uses: 'docker://my image:1.0'"),
        [
            "test.yaml:6:15: URI for Docker container \"docker://my image\" is invalid: parse \"docker://my image\": invalid character \" \" in host name (tag=1.0) [invalid-docker-action]"
        ]
    );
}

#[test]
fn a_docker_action_with_or_without_a_tag_is_valid() {
    assert_eq!(lint_step("uses: docker://alpine"), [] as [&str; 0]);
    assert_eq!(
        lint_step("uses: docker://ghcr.io/owner/image:1.0"),
        [] as [&str; 0]
    );
}

#[test]
fn input_names_are_case_insensitive_and_a_repeated_key_is_checked_once() {
    assert_eq!(
        lint_step("uses: actions/cache@v4\n        with:\n          KEY: k\n          Path: p"),
        [] as [&str; 0]
    );
    assert_eq!(
        lint_step(
            "uses: actions/cache@v4\n        with:\n          key: k\n          path: p\n          paths: a\n          PATHS: b"
        ),
        [
            "test.yaml:10:11: input \"paths\" is not defined in action \"actions/cache@v4\". available inputs are \"enableCrossOsArchive\", \"fail-on-cache-miss\", \"key\", \"lookup-only\", \"path\", \"restore-keys\", \"save-always\", \"upload-chunk-size\" [undefined-action-input]",
            "test.yaml:11:11: key \"PATHS\" is duplicated in \"with\" section. previously defined at line:10,col:11. note that this key is case insensitive [duplicate-key]",
        ]
    );
}

#[test]
fn every_missing_required_input_is_reported_at_uses() {
    assert_eq!(
        lint_step("uses: actions/cache@v4"),
        [
            "test.yaml:6:15: missing input \"key\" which is required by action \"actions/cache@v4\". all required inputs are \"key\", \"path\" [missing-required-action-input]",
            "test.yaml:6:15: missing input \"path\" which is required by action \"actions/cache@v4\". all required inputs are \"key\", \"path\" [missing-required-action-input]",
        ]
    );
}

#[test]
fn an_action_that_skips_inputs_accepts_any_input() {
    assert_eq!(
        lint_step("uses: octokit/request-action@v2.x\n        with:\n          anything: x"),
        [] as [&str; 0]
    );
}

#[test]
fn an_outdated_action_is_not_checked_for_inputs() {
    assert_eq!(
        lint_step("uses: actions/checkout@v2\n        with:\n          nope: x"),
        [
            "test.yaml:6:15: the runner of \"actions/checkout@v2\" action is too old to run on GitHub Actions. update the action's version to fix this issue [outdated-action]"
        ]
    );
}

#[test]
fn required_inputs_are_missing_when_with_is_not_a_mapping() {
    assert_eq!(
        lint_step("uses: actions/cache@v4\n        with: foo"),
        [
            "test.yaml:6:15: missing input \"key\" which is required by action \"actions/cache@v4\". all required inputs are \"key\", \"path\" [missing-required-action-input]",
            "test.yaml:6:15: missing input \"path\" which is required by action \"actions/cache@v4\". all required inputs are \"key\", \"path\" [missing-required-action-input]",
            "test.yaml:7:15: \"with\" section is scalar node but mapping node is expected [unexpected-node-kind]",
        ]
    );
}
