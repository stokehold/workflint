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
fn repository_dispatch_and_image_version_are_not_checked_here() {
    let on = "  repository_dispatch:\n    types: [x]\n  image_version:\n    names: [a]\n";
    assert_eq!(events(&workflow(on)), NONE);
}

/// Returns the diagnostics for a `schedule` with one `cron`, written as a double-quoted YAML
/// string so that every diagnostic is at 3:13.
fn cron(spec: &str) -> Vec<String> {
    let quoted = spec
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\t', "\\t");
    events(&workflow(&format!(
        "  schedule:\n    - cron: \"{quoted}\"\n"
    )))
}

/// The `invalid-cron` line for `spec` with robfig/cron's error `err`.
fn invalid_cron(spec: &str, err: &str) -> String {
    format!(
        "3:13: invalid CRON format {} in schedule event: {err} [invalid-cron]",
        workflint_diagnostics::quote::quote(spec)
    )
}

/// The `cron-too-frequent` line for an interval of `seconds`.
fn too_frequent(seconds: &str) -> String {
    format!(
        "3:13: scheduled job runs too frequently. it runs once per {seconds} seconds. the shortest interval is once every 5 minutes [cron-too-frequent]"
    )
}

/// Each error of robfig/cron v3.0.1's parser that a 5-field spec without descriptors can reach,
/// with the text that the reference binary prints. `empty spec string` cannot be reached, since
/// the parser reports an empty `cron` and drops its item.
#[test]
fn each_cron_parser_error_has_the_reference_text() {
    let cases = [
        (
            "0 */3 * *",
            "expected exactly 5 fields, found 4: [0 */3 * *]",
        ),
        (
            "* * * * * *",
            "expected exactly 5 fields, found 6: [* * * * * *]",
        ),
        (" ", "expected exactly 5 fields, found 0: []"),
        ("@daily", "parser does not accept descriptors: @daily"),
        ("@every 5m", "parser does not accept descriptors: @every 5m"),
        (
            "TZ=UTC @daily",
            "parser does not accept descriptors: @daily",
        ),
        (
            "TZ=Not/AZone */5 * * * *",
            "provided bad location Not/AZone: unknown time zone Not/AZone",
        ),
        (
            "TZ=america/new_york 0 0 * * *",
            "provided bad location america/new_york: unknown time zone america/new_york",
        ),
        (
            "TZ=utc 0 0 * * *",
            "provided bad location utc: unknown time zone utc",
        ),
        (
            "TZ=../etc */5 * * * *",
            "provided bad location ../etc: time: invalid location name",
        ),
        (
            "TZ=/etc */5 * * * *",
            "provided bad location /etc: time: invalid location name",
        ),
        ("1-2-3 * * * *", "too many hyphens: 1-2-3"),
        ("1-x-3 * * * *", "too many hyphens: 1-x-3"),
        ("*/2/3 * * * *", "too many slashes: */2/3"),
        ("1/2/x * * * *", "too many slashes: 1/2/x"),
        (
            "x * * * *",
            r#"failed to parse int from x: strconv.Atoi: parsing "x": invalid syntax"#,
        ),
        (
            "x-1-2 * * * *",
            r#"failed to parse int from x: strconv.Atoi: parsing "x": invalid syntax"#,
        ),
        (
            "-1 * * * *",
            r#"failed to parse int from : strconv.Atoi: parsing "": invalid syntax"#,
        ),
        (
            "*/ * * * *",
            r#"failed to parse int from : strconv.Atoi: parsing "": invalid syntax"#,
        ),
        (
            "*/x * * * *",
            r#"failed to parse int from x: strconv.Atoi: parsing "x": invalid syntax"#,
        ),
        (
            "99999999999999999999 * * * *",
            r#"failed to parse int from 99999999999999999999: strconv.Atoi: parsing "99999999999999999999": value out of range"#,
        ),
        ("*/-1 * * * *", "negative number (-1) not allowed: -1"),
        ("60 * * * *", "end of range (60) above maximum (59): 60"),
        ("* 24 * * *", "end of range (24) above maximum (23): 24"),
        ("* * 32 * *", "end of range (32) above maximum (31): 32"),
        ("* * * 13 *", "end of range (13) above maximum (12): 13"),
        ("* * * * 7", "end of range (7) above maximum (6): 7"),
        ("* * 0 * *", "beginning of range (0) below minimum (1): 0"),
        ("* * * 0 *", "beginning of range (0) below minimum (1): 0"),
        (
            "5-3 * * * *",
            "beginning of range (5) beyond end of range (3): 5-3",
        ),
        (
            "*/0 * * * *",
            "step of range should be a positive number: */0",
        ),
    ];
    for (spec, err) in cases {
        assert_eq!(cron(spec), [invalid_cron(spec, err)], "{spec:?}");
    }
}

#[test]
fn valid_cron_specs_are_not_reported() {
    for spec in [
        "*/5 * * * *",
        "0 0 * * 0-6",
        "+5 * * * *",
        "0\t*\t*\t*\t*",
        "0 0 29 2 *",
        "0 0 31 * *",
        "0 0 1 1 0",
        "TZ=Asia/Tokyo 0 0 * * *",
        "CRON_TZ=UTC 0 0 * * *",
        "TZ=Local 0 0 * * *",
        "TZ= 0 0 * * *",
        "TZ=UTC  0 0 * * *",
    ] {
        assert_eq!(cron(spec), NONE, "{spec:?}");
    }
}

#[test]
fn a_schedule_that_runs_more_often_than_every_5_minutes_is_reported() {
    let cases = [
        ("*/4 * * * *", "240"),
        ("* */3 * * *", "60"),
        ("*/1 * * * *", "60"),
        ("0-59/2 * * * *", "120"),
        ("5/2 * * * *", "120"),
        ("1,,2 * * * *", "60"),
        ("* * * JAN-mar MON", "60"),
        ("? ? ? ? ?", "60"),
        ("*-5 * * * *", "60"),
        ("TZ=Asia/Tokyo * * * * *", "60"),
    ];
    for (spec, seconds) in cases {
        assert_eq!(cron(spec), [too_frequent(seconds)], "{spec:?}");
    }
}

/// A schedule that never runs within five years of the epoch gives two zero times, so the
/// reference reports an interval of 0 seconds.
#[test]
fn a_schedule_that_never_runs_has_an_interval_of_0_seconds() {
    for spec in [", * * * *", "0 0 30 2 *"] {
        assert_eq!(cron(spec), [too_frequent("0")], "{spec:?}");
    }
}

/// Only the first two runs after the Unix epoch count, computed in UTC. `0,1 0 * * *` runs at
/// 00:01 and then at 00:00 the next day in UTC, so its interval is 86340 seconds.
#[test]
fn the_interval_is_between_the_first_two_runs_after_the_epoch_in_utc() {
    assert_eq!(cron("0,1 0 * * *"), NONE);
    assert_eq!(cron("TZ=Asia/Tokyo 0,1 0 * * *"), NONE);
    assert_eq!(cron("0 0,23 * * *"), NONE);
}

/// The reference panics on `TZ=` without a space after it. workflint takes the rest of the spec
/// as the time zone name and reports the spec's missing fields
/// (`docs/differences.md#cron-time-zone-without-fields`).
#[test]
fn a_time_zone_prefix_without_fields_is_reported() {
    assert_eq!(
        cron("TZ=UTC"),
        [invalid_cron(
            "TZ=UTC",
            "expected exactly 5 fields, found 0: []"
        )]
    );
    assert_eq!(
        cron("CRON_TZ=Nope"),
        [invalid_cron(
            "CRON_TZ=Nope",
            "provided bad location Nope: unknown time zone Nope"
        )]
    );
}

/// Returns the diagnostics for a `schedule` whose `cron` is valid and whose `timezone` is
/// `zone`, at 4:17.
fn timezone(zone: &str) -> Vec<String> {
    events(&workflow(&format!(
        "  schedule:\n    - cron: '0 0 * * *'\n      timezone: '{zone}'\n"
    )))
}

#[test]
fn an_unknown_time_zone_is_reported() {
    for zone in [
        "Not/A/Timezone",
        "Asia/Somewhere",
        "UTC",
        "utc",
        "local",
        "Local",
        "america/new_york",
        "localtime",
    ] {
        assert_eq!(
            timezone(zone),
            [format!(
                r#"4:17: invalid timezone "{zone}" in schedule event. it must be a valid IANA timezone name [invalid-timezone]"#
            )],
            "{zone:?}"
        );
    }
}

#[test]
fn iana_time_zones_are_accepted() {
    for zone in ["America/New_York", "Europe/London", "Etc/UTC", "Zulu"] {
        assert_eq!(timezone(zone), NONE, "{zone:?}");
    }
}

#[test]
fn an_invalid_cron_and_time_zone_are_both_reported() {
    let on = "  schedule:\n    - cron: '0 */3 * *'\n      timezone: Nowhere\n";
    assert_eq!(
        events(&workflow(on)),
        [
            invalid_cron("0 */3 * *", "expected exactly 5 fields, found 4: [0 */3 * *]"),
            r#"4:17: invalid timezone "Nowhere" in schedule event. it must be a valid IANA timezone name [invalid-timezone]"#.to_owned(),
        ]
    );
}
