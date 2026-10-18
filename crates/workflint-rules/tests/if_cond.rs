//! The `if-cond` rule: the cases of the reference's `rule_if_cond_test.go` and the differences
//! that `docs/differences.md` describes.

#![allow(clippy::unwrap_used, clippy::panic)]

use workflint_diagnostics::{Color, FileId, SourceFile, render_oneline};

/// Parses `text` as a workflow, runs the rules over it, and returns the diagnostics of the
/// `if-cond` rule in the one-line format. The other rules, such as `expression`, also report on
/// some of these conditions.
fn lint(text: &str) -> Vec<String> {
    let file = SourceFile::new(FileId::new(0), "test.yaml", text).unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    let parsed = workflint_syntax::parse(&file, &document);
    let diagnostics = workflint_rules::run(&file, &parsed.workflow);
    let mut out = Vec::new();
    render_oneline(&mut out, &file, &diagnostics, Color::Off).unwrap();
    String::from_utf8(out)
        .unwrap()
        .lines()
        .filter(|line| line.ends_with("[if-always-true]") || line.ends_with("[constant-condition]"))
        .map(str::to_owned)
        .collect()
}

/// Returns the rule IDs that the rules report for a workflow whose step has `if: <condition>`.
fn step_rules(condition: &str) -> Vec<String> {
    let text = format!(
        "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo\n        if: {condition}\n"
    );
    rule_ids(&lint(&text))
}

/// Returns the rule IDs that the rules report for a workflow whose job and snapshot have
/// `if: <condition>`.
fn job_rules(condition: &str) -> Vec<String> {
    let text = format!(
        "on: push\njobs:\n  test:\n    if: {condition}\n    runs-on: ubuntu-latest\n    snapshot:\n      image-name: test\n      if: {condition}\n    steps:\n      - run: echo\n"
    );
    rule_ids(&lint(&text))
}

fn rule_ids(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            let start = line.rfind('[').unwrap() + 1;
            line[start..line.len() - 1].to_owned()
        })
        .collect()
}

/// The cases of `rule_if_cond_test.go`, written as YAML flow scalars, with the rule ID the
/// reference reports for each, if any.
const REFERENCE_CASES: [(&str, Option<&str>); 13] = [
    ("''", None),
    ("true", Some("constant-condition")),
    ("false", Some("constant-condition")),
    ("true || false", Some("constant-condition")),
    ("github.ref_name == 'foo'", None),
    ("${{ true }}", Some("constant-condition")),
    ("${{ contains('foo', 'f') }}", Some("constant-condition")),
    ("${{ github.ref_name == 'foo' }}", None),
    ("${{ foo }}", None),
    ("\"${{ foo }}\\n\"", Some("if-always-true")),
    ("'${{ foo }} '", Some("if-always-true")),
    ("' ${{ foo }}'", Some("if-always-true")),
    ("${{ true }} && ${{ true }}", Some("if-always-true")),
];

#[test]
fn the_reference_cases_report_the_same_rules_at_a_step() {
    for (condition, rule) in REFERENCE_CASES {
        let expected: Vec<String> = rule.into_iter().map(str::to_owned).collect();
        assert_eq!(step_rules(condition), expected, "{condition}");
    }
}

#[test]
fn the_reference_cases_report_the_same_rules_at_a_job_and_its_snapshot() {
    for (condition, rule) in REFERENCE_CASES {
        let expected: Vec<String> = rule
            .into_iter()
            .flat_map(|r| [r, r])
            .map(str::to_owned)
            .collect();
        assert_eq!(job_rules(condition), expected, "{condition}");
    }
}

#[test]
fn a_constant_condition_is_reported_at_the_start_of_the_value_with_its_trimmed_text() {
    let lines = lint(
        "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo\n        if: |\n          ${{  true && 42 || !null  }}\n",
    );
    assert_eq!(
        lines,
        [
            "test.yaml:7:13: if: condition \"${{  true && 42 || !null  }}\\n\" is always evaluated to true because extra characters are around ${{ }} [if-always-true]"
        ]
    );
    let lines = lint(
        "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo\n        if: '${{  true && 42 || !null  }}'\n",
    );
    assert_eq!(
        lines,
        [
            "test.yaml:7:13: constant expression \"true && 42 || !null\" in condition. remove the if: section [constant-condition]"
        ]
    );
}

#[test]
fn closing_braces_inside_a_string_literal_do_not_end_the_expression() {
    assert_eq!(
        step_rules("${{ github.ref == '}}' }}"),
        Vec::<String>::new()
    );
    assert_eq!(step_rules("${{ '}}' == '}}' }}"), ["constant-condition"]);
}

#[test]
fn a_closing_pair_before_the_first_placeholder_is_always_true() {
    assert_eq!(step_rules("'}} ${{ github.ref }}'"), ["if-always-true"]);
}

#[test]
fn a_condition_that_does_not_parse_is_left_to_the_expression_rule() {
    assert_eq!(step_rules("${{ github.ref # x }}"), Vec::<String>::new());
    assert_eq!(step_rules("github.ref # x"), Vec::<String>::new());
}

/// rhysd/actionlint#110: quotes outside `${{ }}` are text, so the condition is always true.
#[test]
fn quotes_mixed_with_a_placeholder_are_always_true() {
    assert_eq!(
        step_rules("${{ github.ref }} == \"refs/heads/main\""),
        ["if-always-true"]
    );
    assert_eq!(
        step_rules("'${{ github.event_name }} == ''push'''"),
        ["if-always-true"]
    );
}

fn step_message(condition: &str) -> String {
    let text = format!(
        "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo\n        if: {condition}\n"
    );
    let lines = lint(&text);
    let [line] = &lines[..] else {
        panic!("{lines:?}")
    };
    line.clone()
}

/// rhysd/actionlint#367: placeholders joined by operators suggest the one expression they mean.
#[test]
fn placeholders_joined_by_operators_suggest_one_expression() {
    assert_eq!(
        step_message("${{ success() }} || ${{ failure() }}"),
        "test.yaml:7:13: if: condition \"${{ success() }} || ${{ failure() }}\" is always evaluated to true because extra characters are around ${{ }}. did you mean \"${{ success() || failure() }}\"? [if-always-true]"
    );
    assert_eq!(
        step_message("${{ github.event_name == 'push' }} && ${{ github.ref_name == 'main' }}"),
        "test.yaml:7:13: if: condition \"${{ github.event_name == 'push' }} && ${{ github.ref_name == 'main' }}\" is always evaluated to true because extra characters are around ${{ }}. did you mean \"${{ github.event_name == 'push' && github.ref_name == 'main' }}\"? [if-always-true]"
    );
    assert_eq!(
        step_message("'!${{ github.ref == ''main'' }}'"),
        "test.yaml:7:13: if: condition \"!${{ github.ref == 'main' }}\" is always evaluated to true because extra characters are around ${{ }}. did you mean \"${{ !(github.ref == 'main') }}\"? [if-always-true]"
    );
}

#[test]
fn the_suggestion_keeps_the_meaning_of_each_placeholder() {
    assert_eq!(
        step_message("${{ success() || failure() }} && ${{ github.ref == 'main' }}"),
        "test.yaml:7:13: if: condition \"${{ success() || failure() }} && ${{ github.ref == 'main' }}\" is always evaluated to true because extra characters are around ${{ }}. did you mean \"${{ (success() || failure()) && github.ref == 'main' }}\"? [if-always-true]"
    );
    assert_eq!(
        step_message("${{ success() && github.ref == 'main' }} || ${{ failure() }}"),
        "test.yaml:7:13: if: condition \"${{ success() && github.ref == 'main' }} || ${{ failure() }}\" is always evaluated to true because extra characters are around ${{ }}. did you mean \"${{ success() && github.ref == 'main' || failure() }}\"? [if-always-true]"
    );
}

#[test]
fn no_suggestion_for_whitespace_text_or_a_constant() {
    let plain = |condition: &str| {
        format!(
            "test.yaml:7:13: if: condition {} is always evaluated to true because extra characters are around ${{{{ }}}} [if-always-true]",
            workflint_diagnostics::quote::quote(condition)
        )
    };
    assert_eq!(
        step_message("'${{ success() }} '"),
        plain("${{ success() }} ")
    );
    assert_eq!(
        step_message("echo ${{ success() }} and ${{ failure() }}"),
        plain("echo ${{ success() }} and ${{ failure() }}")
    );
    assert_eq!(
        step_message("${{ false }} && ${{ false }}"),
        plain("${{ false }} && ${{ false }}")
    );
}
