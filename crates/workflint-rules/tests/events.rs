//! The `events` rule (`rule_events.go`): webhook events, their activity types and filters, and
//! the inputs of `workflow_call` and `workflow_dispatch`.
//!
//! Each test lints a workflow and compares the diagnostics of the `events` kind, sorted by
//! position, with the lines that the reference prints for the same workflow, apart from the
//! differences that `docs/differences.md` lists.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fmt::Write as _;

use workflint_diagnostics::{FileId, SourceFile, sort_and_dedup};

/// Lints `text` as `test.yaml` and returns the diagnostics of the `events` kind as
/// `line:col: message [rule-id]`.
fn events(text: &str) -> Vec<String> {
    let file = SourceFile::new(FileId::new(0), "test.yaml", text).unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    let parsed = workflint_syntax::parse(&file, &document);
    let mut diagnostics = workflint_rules::run(&file, &parsed.workflow);
    sort_and_dedup(&file, &mut diagnostics);
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.rule.kind == "events")
        .map(|diagnostic| {
            let position = file.line_col(diagnostic.span.start);
            let mut line = String::new();
            write!(
                line,
                "{}:{}: {} [{}]",
                position.line, position.column, diagnostic.message, diagnostic.rule.id
            )
            .unwrap();
            line
        })
        .collect()
}

/// Puts `on` in a workflow with one job.
fn workflow(on: &str) -> String {
    format!("on:\n{on}jobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo\n")
}

/// No diagnostics.
const NONE: [&str; 0] = [];

const UNKNOWN_EVENT_HINT: &str = "see https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#webhook-events for list of all Webhook event names";

#[test]
fn an_unknown_event_is_reported_at_its_name() {
    assert_eq!(
        events(&workflow("  pullreq:\n  push:\n")),
        [format!(
            r#"2:3: unknown Webhook event "pullreq". {UNKNOWN_EVENT_HINT} [unknown-event]"#
        )]
    );
}

#[test]
fn event_names_are_case_sensitive() {
    assert_eq!(
        events(&workflow("  SCHEDULE:\n  Push:\n")),
        [
            format!(
                r#"2:3: unknown Webhook event "SCHEDULE". {UNKNOWN_EVENT_HINT} [unknown-event]"#
            ),
            format!(r#"3:3: unknown Webhook event "Push". {UNKNOWN_EVENT_HINT} [unknown-event]"#),
        ]
    );
}

#[test]
fn unknown_events_in_a_string_or_a_sequence_are_reported() {
    let text = "on: [push, pullreq]\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo\n";
    assert_eq!(
        events(text),
        [format!(
            r#"1:12: unknown Webhook event "pullreq". {UNKNOWN_EVENT_HINT} [unknown-event]"#
        )]
    );
}

#[test]
fn an_event_with_an_empty_name_is_unknown() {
    assert_eq!(
        events(&workflow("  '': x\n")),
        [format!(
            r#"2:3: unknown Webhook event "". {UNKNOWN_EVENT_HINT} [unknown-event]"#
        )]
    );
}

#[test]
fn an_event_key_that_is_not_a_string_is_not_reported_again() {
    assert_eq!(events(&workflow("  [a]: x\n  push:\n")), NONE);
}

#[test]
fn an_invalid_activity_type_is_reported_with_the_sorted_types() {
    assert_eq!(
        events(&workflow("  issues:\n    types: [opened, created]\n")),
        [
            r#"3:21: invalid activity type "created" for "issues" Webhook event. available types are "assigned", "closed", "deleted", "demilestoned", "edited", "labeled", "locked", "milestoned", "opened", "pinned", "reopened", "transferred", "typed", "unassigned", "unlabeled", "unlocked", "unpinned", "untyped" [invalid-activity-type]"#
        ]
    );
}

#[test]
fn activity_types_are_case_sensitive() {
    assert_eq!(
        events(&workflow("  merge_group:\n    types: Checks_Requested\n")),
        [
            r#"3:12: invalid activity type "Checks_Requested" for "merge_group" Webhook event. available types are "checks_requested" [invalid-activity-type]"#
        ]
    );
}

#[test]
fn activity_types_that_are_not_valid_strings_are_not_reported_again() {
    let on = "  pull_request:\n    types: [[x]]\n  pull_request_target:\n    types: ['']\n  issues:\n    types: ''\n  release:\n    types:\n";
    assert_eq!(events(&workflow(on)), NONE);
}

#[test]
fn an_activity_type_from_an_alias_is_reported_at_the_anchor() {
    let on = "  label:\n    types: &created [created]\n  pull_request:\n    types: *created\n";
    let diagnostics = events(&workflow(on));
    assert_eq!(diagnostics.len(), 1);
    assert!(
        diagnostics[0].starts_with(
            r#"3:22: invalid activity type "created" for "pull_request" Webhook event."#
        ),
        "{diagnostics:?}"
    );
}

#[test]
fn types_on_an_event_without_activity_types_are_reported_at_the_event() {
    assert_eq!(
        events(&workflow("  push:\n    types: [created]\n")),
        [
            r#"2:3: "types" cannot be specified for "push" Webhook event [activity-types-not-supported]"#
        ]
    );
}

#[test]
fn an_invalid_type_still_counts_as_types_on_an_event_without_activity_types() {
    assert_eq!(
        events(&workflow("  push:\n    types: [[x]]\n")),
        [
            r#"2:3: "types" cannot be specified for "push" Webhook event [activity-types-not-supported]"#
        ]
    );
}

#[test]
fn repository_dispatch_takes_any_activity_type() {
    assert_eq!(
        events(&workflow("  repository_dispatch:\n    types: [anything]\n")),
        NONE
    );
}

#[test]
fn workflow_run_without_workflows_is_reported() {
    assert_eq!(
        events(&workflow("  workflow_run:\n    types: [completed]\n")),
        [
            r#"2:3: no workflow is configured for "workflow_run" event [missing-workflow-run-workflows]"#
        ]
    );
    assert_eq!(
        events(&workflow("  workflow_run:\n")),
        [
            r#"2:3: no workflow is configured for "workflow_run" event [missing-workflow-run-workflows]"#
        ]
    );
}

#[test]
fn workflow_run_with_an_invalid_workflow_has_workflows() {
    assert_eq!(
        events(&workflow("  workflow_run:\n    workflows: [[x]]\n")),
        NONE
    );
}

#[test]
fn workflows_on_another_event_are_reported() {
    assert_eq!(
        events(&workflow("  pull_request:\n    workflows: [ci.yaml]\n")),
        [
            r#"2:3: "workflows" cannot be configured for "pull_request" event. it is only for workflow_run event [workflows-filter-not-supported]"#
        ]
    );
}

#[test]
fn a_filter_that_an_event_does_not_support_is_reported_at_its_key() {
    let on = "  release:\n    tags: v*\n    tags-ignore: dev\n  pull_request_review:\n    paths: a\n    branches: main\n";
    assert_eq!(
        events(&workflow(on)),
        [
            r#"3:5: "tags" filter is not available for release event. it is only for push event [filter-not-supported]"#,
            r#"4:5: "tags-ignore" filter is not available for release event. it is only for push event [filter-not-supported]"#,
            r#"6:5: "paths" filter is not available for pull_request_review event. it is only for push, pull_request, pull_request_target events [filter-not-supported]"#,
            r#"7:5: "branches" filter is not available for pull_request_review event. it is only for merge_group, push, pull_request, pull_request_target, workflow_run events [filter-not-supported]"#,
        ]
    );
}

#[test]
fn a_filter_and_its_ignore_filter_conflict_at_the_later_key() {
    let on = "  push:\n    paths-ignore: a\n    paths: b\n    tags: v*\n    tags-ignore: dev\n";
    assert_eq!(
        events(&workflow(on)),
        [
            r#"4:5: both "paths" and "paths-ignore" filters cannot be used for the same event "push". note: use '!' to negate patterns [conflicting-filters]"#,
            r#"6:5: both "tags" and "tags-ignore" filters cannot be used for the same event "push". note: use '!' to negate patterns [conflicting-filters]"#,
        ]
    );
}

#[test]
fn an_empty_filter_does_not_conflict() {
    let on = "  pull_request:\n    branches-ignore: [a]\n    branches: []\n";
    assert_eq!(events(&workflow(on)), NONE);
}

#[test]
fn a_filter_with_only_invalid_items_still_conflicts() {
    assert_eq!(
        events(&workflow(
            "  push:\n    branches: [[x]]\n    branches-ignore: ['']\n"
        )),
        [
            r#"4:5: both "branches" and "branches-ignore" filters cannot be used for the same event "push". note: use '!' to negate patterns [conflicting-filters]"#
        ]
    );
}

#[test]
fn a_workflow_call_default_that_does_not_match_its_type_is_reported() {
    let on = "  workflow_call:\n    inputs:\n      MyNum:\n        type: number\n        default: x\n      flag:\n        type: boolean\n        default: yes\n";
    assert_eq!(
        events(&workflow(on)),
        [
            r#"6:18: input of workflow_call event "MyNum" is typed as number but its default value "x" cannot be parsed as a float number: strconv.ParseFloat: parsing "x": invalid syntax [invalid-input-default]"#,
            r#"9:18: input of workflow_call event "flag" is typed as boolean. its default value must be true or false but got "yes" [invalid-input-default]"#,
        ]
    );
}

#[test]
fn a_workflow_call_default_parses_as_go_parses_numbers_and_booleans() {
    let on = "  workflow_call:\n    inputs:\n      a:\n        type: number\n        default: -Infinity\n      b:\n        type: number\n        default: 0x1p-2\n      c:\n        type: number\n        default: 1_000\n      d:\n        type: boolean\n        default: TRUE\n      e:\n        type: number\n        default: 1e400\n";
    assert_eq!(
        events(&workflow(on)),
        [
            r#"18:18: input of workflow_call event "e" is typed as number but its default value "1e400" cannot be parsed as a float number: strconv.ParseFloat: parsing "1e400": value out of range [invalid-input-default]"#
        ]
    );
}

#[test]
fn a_workflow_call_default_with_an_expression_is_not_type_checked() {
    let on = "  workflow_call:\n    inputs:\n      a:\n        type: number\n        default: ${{ github.run_number }}\n      b:\n        type: boolean\n        default: x ${{ true }}\n";
    assert_eq!(events(&workflow(on)), NONE);
}

#[test]
fn a_required_workflow_call_input_with_a_default_is_reported() {
    let on = "  workflow_call:\n    inputs:\n      Path:\n        type: string\n        required: true\n        default: ''\n      b:\n        type: boolean\n        required: true\n        default: ${{ true }}\n      c:\n        type: string\n        required: false\n        default: x\n";
    assert_eq!(
        events(&workflow(on)),
        [
            r#"7:18: input "Path" of workflow_call event has the default value "", but it is also required. if an input is marked as required, its default value will never be used [required-input-with-default]"#,
            r#"11:18: input "b" of workflow_call event has the default value "${{ true }}", but it is also required. if an input is marked as required, its default value will never be used [required-input-with-default]"#,
        ]
    );
}

#[test]
fn an_invalid_workflow_call_default_is_not_reported_again() {
    let on = "  workflow_call:\n    inputs:\n      a:\n        type: string\n        required: true\n        default: [x]\n      b:\n        type: number\n        default: [x]\n";
    assert_eq!(events(&workflow(on)), NONE);
}

#[test]
fn a_choice_input_without_options_is_reported_at_its_name() {
    let on = "  workflow_dispatch:\n    inputs:\n      NoOptions:\n        type: choice\n      empty:\n        type: choice\n        options: []\n";
    assert_eq!(
        events(&workflow(on)),
        [
            r#"4:7: input type of "nooptions" is "choice" but "options" is not set [choice-input-without-options]"#,
            r#"6:7: input type of "empty" is "choice" but "options" is not set [choice-input-without-options]"#,
        ]
    );
}

#[test]
fn a_choice_input_with_only_an_invalid_option_has_options() {
    let on = "  workflow_dispatch:\n    inputs:\n      pick:\n        type: choice\n        options: ['']\n";
    assert_eq!(events(&workflow(on)), NONE);
}

#[test]
fn each_repeated_option_is_reported() {
    let on = "  workflow_dispatch:\n    inputs:\n      Pick:\n        type: choice\n        options: [a, b, a, a, '']\n";
    assert_eq!(
        events(&workflow(on)),
        [
            r#"6:25: option "a" is duplicated in options of "pick" input [duplicate-input-option]"#,
            r#"6:28: option "a" is duplicated in options of "pick" input [duplicate-input-option]"#,
        ]
    );
}

#[test]
fn a_default_that_is_not_an_option_is_reported_with_the_options_quoted_once() {
    let on = "  workflow_dispatch:\n    inputs:\n      name:\n        type: choice\n        options: [Tama, Mike, Tama]\n        default: Chobi\n";
    assert_eq!(
        events(&workflow(on)),
        [
            r#"6:31: option "Tama" is duplicated in options of "name" input [duplicate-input-option]"#,
            r#"7:18: default value "Chobi" of "name" input is not included in its options "Tama", "Mike", "Tama" [default-not-in-options]"#,
        ]
    );
}

#[test]
fn a_default_is_not_checked_against_options_with_an_invalid_option() {
    let on = "  workflow_dispatch:\n    inputs:\n      a:\n        type: choice\n        options: ['']\n        default: ''\n      b:\n        type: choice\n        options: [x, [y]]\n        default: y\n";
    assert_eq!(events(&workflow(on)), NONE);
}

#[test]
fn an_invalid_default_of_a_choice_input_is_not_reported_again() {
    let on = "  workflow_dispatch:\n    inputs:\n      pick:\n        type: choice\n        options: [a]\n        default: [x]\n";
    assert_eq!(events(&workflow(on)), NONE);
}

#[test]
fn options_on_an_input_that_is_not_a_choice_are_reported() {
    let on = "  workflow_dispatch:\n    inputs:\n      Text:\n        type: string\n        options: [[x]]\n      untyped:\n        options: [a]\n      unknown:\n        type: text\n        options: [a]\n      empty:\n        type: boolean\n        options: []\n";
    assert_eq!(
        events(&workflow(on)),
        [
            r#"4:7: "options" can not be set to "text" input because its input type is not "choice" [options-on-non-choice-input]"#,
            r#"7:7: "options" can not be set to "untyped" input because its input type is not "choice" [options-on-non-choice-input]"#,
            r#"9:7: "options" can not be set to "unknown" input because its input type is not "choice" [options-on-non-choice-input]"#,
        ]
    );
}

#[test]
fn a_workflow_dispatch_default_that_does_not_match_its_type_is_reported() {
    let on = "  workflow_dispatch:\n    inputs:\n      MyNum:\n        type: number\n        default: x\n      MyFlag:\n        type: boolean\n        default: ${{ true }}\n      ok:\n        type: boolean\n        default: False\n      env:\n        type: environment\n        default: x\n";
    assert_eq!(
        events(&workflow(on)),
        [
            r#"6:18: type of "MyNum" input is "number" but its default value "x" cannot be parsed as a float number: strconv.ParseFloat: parsing "x": invalid syntax [invalid-input-default]"#,
            r#"9:18: type of "myflag" input is "boolean". its default value "${{ true }}" must be "true" or "false" [invalid-input-default]"#,
        ]
    );
}

#[test]
fn an_invalid_workflow_dispatch_default_is_not_reported_again() {
    let on = "  workflow_dispatch:\n    inputs:\n      num:\n        type: number\n        default: [x]\n      flag:\n        type: boolean\n        default: {}\n";
    assert_eq!(events(&workflow(on)), NONE);
}

/// Returns `workflow_dispatch` with `count` string inputs.
fn dispatch_inputs(count: usize) -> String {
    let mut on = String::from("  workflow_dispatch:\n    inputs:\n");
    for index in 1..=count {
        write!(on, "      input{index}:\n        type: string\n").unwrap();
    }
    on
}

#[test]
fn workflow_dispatch_takes_at_most_25_inputs() {
    assert_eq!(events(&workflow(&dispatch_inputs(25))), NONE);
    assert_eq!(
        events(&workflow(&dispatch_inputs(26))),
        [
            r#"2:3: maximum number of inputs for "workflow_dispatch" event is 25 but 26 inputs are provided. see https://docs.github.com/en/actions/using-workflows/events-that-trigger-workflows#providing-inputs [too-many-dispatch-inputs]"#
        ]
    );
}

#[test]
fn schedule_repository_dispatch_and_image_version_are_not_checked_here() {
    let on = "  schedule:\n    - cron: '0 0 * * *'\n  repository_dispatch:\n    types: [x]\n  image_version:\n    names: [a]\n";
    assert_eq!(events(&workflow(on)), NONE);
}
