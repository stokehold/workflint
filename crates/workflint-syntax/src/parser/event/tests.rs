//! Tests of the `on` section. The expected diagnostics are the reference's
//! `syntax-check` output for the same text, with workflint's rule IDs.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fmt::{Debug, Write as _};
use std::sync::Arc;

use workflint_diagnostics::{FileId, SourceFile, Span};

use crate::testing::{Text, render};
use crate::{Event, Field, Keyed, Value, Workflow, WorkflowCallEvent, parse};

/// A job, so that the workflow has the `jobs` section.
const JOBS: &str = "jobs:\n  t:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo\n";

/// Parses `on` followed by [`JOBS`] and runs `f` on the workflow.
fn with_workflow<R>(on: &str, f: impl FnOnce(&SourceFile, &Workflow<'_>, Vec<String>) -> R) -> R {
    let file = SourceFile::new(FileId::new(0), "test.yml", format!("{on}{JOBS}")).unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    let parsed = parse(&file, &document);
    let diagnostics = render(&file, parsed.diagnostics);
    f(&file, &parsed.workflow, diagnostics)
}

/// Returns the diagnostics of `on`, followed by one line per event and one
/// indented line per item of an event.
fn check(on: &str) -> Vec<String> {
    with_workflow(on, |file, workflow, mut lines| {
        match &workflow.on {
            Field::Valid { value, .. } => {
                for event in value.iter() {
                    describe(file, event, &mut lines);
                }
            }
            Field::Invalid { .. } => lines.push("invalid".to_owned()),
            Field::Absent => lines.push("absent".to_owned()),
        }
        lines
    })
}

fn at(file: &SourceFile, span: Span) -> String {
    let pos = file.line_col(span.start);
    format!("{}:{}", pos.line, pos.column)
}

/// Appends ` name=value` for each part that is present.
fn parts(parts: &[(&str, Option<String>)]) -> String {
    parts
        .iter()
        .filter_map(|(name, value)| Some(format!(" {name}={}", value.as_ref()?)))
        .collect()
}

fn field<T>(field: &Field<'_, T>, valid: impl FnOnce(&T) -> String) -> Option<String> {
    match field {
        Field::Absent => None,
        Field::Invalid { .. } => Some("invalid".to_owned()),
        Field::Valid { value, .. } => Some(valid(value)),
    }
}

fn string(value: &Field<'_, impl Text>) -> Option<String> {
    field(value, |s| format!("{:?}", s.text()))
}

fn strings(value: &Field<'_, Arc<[impl Text]>>) -> Option<String> {
    field(value, |list| {
        let items: Vec<_> = list.iter().map(Text::text).collect();
        format!("[{}]", items.join(", "))
    })
}

fn kind<T: Debug>(value: &Field<'_, T>) -> Option<String> {
    field(value, |kind| format!("{kind:?}"))
}

fn keys<T>(value: &Field<'_, Keyed<'_, T>>) -> Option<String> {
    field(value, |keyed| {
        let keys: Vec<_> = keyed.iter().map(|entry| entry.key.value()).collect();
        format!("{{{}}}", keys.join(", "))
    })
}

fn boolean(value: &Value<'_, bool>) -> Option<String> {
    match value {
        Value::Absent => None,
        Value::Invalid { .. } => Some("invalid".to_owned()),
        Value::Literal { value, .. } => Some(value.to_string()),
        Value::Expression { expr, .. } => Some(expr.source.value().to_owned()),
    }
}

/// Lists the entries of `keyed`, one line each, as `item "key" at L:C`
/// followed by the parts that `describe` returns.
fn entries<T>(
    file: &SourceFile,
    item: &str,
    keyed: &Field<'_, Keyed<'_, T>>,
    describe: impl Fn(&T) -> String,
    lines: &mut Vec<String>,
) {
    for entry in keyed.value().into_iter().flatten() {
        lines.push(format!(
            "  {item} {:?} at {}{}",
            entry.key.value(),
            at(file, entry.key.span()),
            describe(&entry.value)
        ));
    }
}

/// Appends a line for `event`, as `kind "name" at L:C body L:C` followed by
/// its parts, and a line for each of its items.
fn describe(file: &SourceFile, event: &Event<'_>, lines: &mut Vec<String>) {
    let tag = match event {
        Event::Webhook(_) => "webhook",
        Event::Schedule(_) => "schedule",
        Event::WorkflowDispatch(_) => "dispatch",
        Event::RepositoryDispatch(_) => "repository",
        Event::WorkflowCall(_) => "call",
        Event::ImageVersion(_) => "image",
    };
    let name = event.name();
    let mut line = format!("{tag} {:?} at {}", name.value(), at(file, name.span()));
    if let Some(body) = event.body() {
        let _ = write!(line, " body {}", at(file, body));
    }
    match event {
        Event::Webhook(e) => {
            line.push_str(&parts(&[
                ("types", strings(&e.types)),
                ("branches", strings(&e.branches)),
                ("branches-ignore", strings(&e.branches_ignore)),
                ("tags", strings(&e.tags)),
                ("tags-ignore", strings(&e.tags_ignore)),
                ("paths", strings(&e.paths)),
                ("paths-ignore", strings(&e.paths_ignore)),
                ("workflows", strings(&e.workflows)),
            ]));
            lines.push(line);
        }
        Event::Schedule(e) => {
            lines.push(line);
            for entry in e.schedules.iter() {
                lines.push(format!(
                    "  entry at {}{}",
                    at(file, entry.span),
                    parts(&[
                        ("cron", string(&entry.cron)),
                        ("timezone", string(&entry.timezone)),
                    ])
                ));
            }
        }
        Event::WorkflowDispatch(e) => {
            line.push_str(&parts(&[("inputs", keys(&e.inputs))]));
            lines.push(line);
            entries(
                file,
                "input",
                &e.inputs,
                |input| {
                    parts(&[
                        ("description", string(&input.description)),
                        ("required", boolean(&input.required)),
                        ("default", string(&input.default)),
                        ("type", kind(&input.input_type)),
                        ("options", strings(&input.options)),
                    ])
                },
                lines,
            );
        }
        Event::RepositoryDispatch(e) => {
            line.push_str(&parts(&[("types", strings(&e.types))]));
            lines.push(line);
        }
        Event::WorkflowCall(e) => {
            line.push_str(&parts(&[
                ("inputs", keys(&e.inputs)),
                ("secrets", keys(&e.secrets)),
                ("outputs", keys(&e.outputs)),
            ]));
            lines.push(line);
            describe_call(file, e, lines);
        }
        Event::ImageVersion(e) => {
            line.push_str(&parts(&[
                ("names", strings(&e.names)),
                ("versions", strings(&e.versions)),
            ]));
            lines.push(line);
        }
    }
}

/// Lists the inputs, secrets, and outputs of `event`.
fn describe_call(file: &SourceFile, event: &WorkflowCallEvent<'_>, lines: &mut Vec<String>) {
    entries(
        file,
        "input",
        &event.inputs,
        |input| {
            parts(&[
                ("description", string(&input.description)),
                ("required", boolean(&input.required)),
                ("default", string(&input.default)),
                ("type", kind(&input.input_type)),
            ])
        },
        lines,
    );
    entries(
        file,
        "secret",
        &event.secrets,
        |secret| {
            parts(&[
                ("description", string(&secret.description)),
                ("required", boolean(&secret.required)),
            ])
        },
        lines,
    );
    entries(
        file,
        "output",
        &event.outputs,
        |output| {
            parts(&[
                ("description", string(&output.description)),
                ("value", string(&output.value)),
            ])
        },
        lines,
    );
}

#[test]
fn a_scalar_names_one_event_at_the_scalar() {
    assert_eq!(check("on: push\n"), [r#"webhook "push" at 1:5"#]);
    assert_eq!(
        check("on: workflow_call\n"),
        [r#"call "workflow_call" at 1:5"#]
    );
}

#[test]
fn a_sequence_names_an_event_per_item_at_the_item() {
    assert_eq!(
        check(
            "on: [push, PUSH, workflow_dispatch, repository_dispatch, workflow_call, image_version, '', [x], {y: z}]\n"
        ),
        [
            "1:88: string should not be empty [empty-string]",
            r#"1:92: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            r#"1:97: expected scalar node for string value but found mapping node with "!!map" tag [unexpected-node-kind]"#,
            r#"webhook "push" at 1:6"#,
            r#"webhook "PUSH" at 1:12"#,
            r#"dispatch "workflow_dispatch" at 1:18"#,
            r#"repository "repository_dispatch" at 1:37"#,
            r#"call "workflow_call" at 1:58"#,
            r#"image "image_version" at 1:73"#,
        ]
    );
}

#[test]
fn schedule_needs_a_mapping_to_configure_it() {
    assert_eq!(
        check("on: schedule\n"),
        ["1:5: schedule event must be configured with mapping [schedule-without-config]"]
    );
    assert_eq!(
        check("on:\n  - push\n  - schedule\n"),
        [
            "3:5: schedule event must be configured with mapping [schedule-without-config]",
            r#"webhook "push" at 2:5"#,
        ]
    );
}

#[test]
fn an_empty_on_names_no_event() {
    let cases: [(&str, &str); 4] = [
        ("on:\n", "1:4: string should not be empty [empty-string]"),
        ("on: ''\n", "1:5: string should not be empty [empty-string]"),
        (
            "on: []\n",
            r#"1:5: "on" section should not be empty [empty-section]"#,
        ),
        (
            "on: {}\n",
            r#"1:5: "on" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
        ),
    ];
    for (on, want) in cases {
        assert_eq!(check(on), [want], "{on:?}");
        with_workflow(on, |_, workflow, _| {
            assert_eq!(workflow.on.value().map(|events| events.len()), Some(0));
        });
    }
}

#[test]
fn an_alias_for_on_is_invalid_and_on_is_missing() {
    assert_eq!(
        check("&r\non: *r\n"),
        [
            r#"1:1: "on" section is missing in workflow [missing-required-key]"#,
            r#"2:5: "on" section value is expected to be mapping or sequence but found alias node [unexpected-node-kind]"#,
            "invalid",
        ]
    );
}

#[test]
fn a_mapping_names_an_event_per_key_at_the_key() {
    assert_eq!(
        check(
            "on:\n  push:\n  Push:\n  foo: {}\n  workflow_dispatch:\n  repository_dispatch:\n  workflow_call:\n  image_version:\n  push: {}\n  <<: {}\n  '': {}\n"
        ),
        [
            r#"9:3: key "push" is duplicated in "on" section. previously defined at line:2,col:3 [duplicate-key]"#,
            r#"10:3: GitHub Actions does not support YAML merge key "<<" [merge-key-unsupported]"#,
            "11:3: string should not be empty [empty-string]",
            r#"webhook "push" at 2:3 body 2:8"#,
            r#"webhook "Push" at 3:3 body 3:8"#,
            r#"webhook "foo" at 4:3 body 4:8"#,
            r#"dispatch "workflow_dispatch" at 5:3 body 5:21"#,
            r#"repository "repository_dispatch" at 6:3 body 6:23"#,
            r#"call "workflow_call" at 7:3 body 7:17"#,
            r#"image "image_version" at 8:3 body 8:17"#,
            r#"webhook "" at 11:3 body 11:7"#,
        ]
    );
}

#[test]
fn an_event_key_that_is_not_a_scalar_is_reported_and_its_body_parsed() {
    assert_eq!(
        check(
            "on:\n  [push]: x\n  workflow_call:\n    inputs:\n      [a]: {}\n    outputs:\n      {b: c}: {}\n"
        ),
        [
            r#"2:3: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            r#"2:11: "" section is scalar node but mapping node is expected [unexpected-node-kind]"#,
            r#"5:7: "type" is missing at "" input of workflow_call event [missing-required-key]"#,
            r#"5:7: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            r#"7:7: "value" is missing at "" output of workflow_call event [missing-required-key]"#,
            r#"7:7: expected scalar node for string value but found mapping node with "!!map" tag [unexpected-node-kind]"#,
            r#"call "workflow_call" at 3:3 body 4:5 inputs={} outputs={}"#,
        ]
    );
}

#[test]
fn webhook_filters_take_a_string_or_a_sequence() {
    assert_eq!(
        check(
            "on:\n  push:\n    types: created\n    branches: [main, 'release/**']\n    branches-ignore: dev\n    tags: [v1]\n    tags-ignore: v0\n    paths: [src/**]\n    paths-ignore: docs/**\n    workflows: [CI]\n"
        ),
        [
            r#"webhook "push" at 2:3 body 3:5 types=[created] branches=[main, release/**] branches-ignore=[dev] tags=[v1] tags-ignore=[v0] paths=[src/**] paths-ignore=[docs/**] workflows=[CI]"#
        ]
    );
}

#[test]
fn webhook_filters_must_not_be_empty() {
    assert_eq!(
        check(
            "on:\n  push:\n    types:\n    branches: []\n    branches-ignore: ['']\n    tags: {a: b}\n    tags-ignore: ''\n    paths: [[a]]\n    paths-ignore: ~\n    workflows: []\n    Types: x\n  pull_request: x\n  issues: [opened]\n  release:\n    types: [a]\n    types: [b]\n"
        ),
        [
            "3:11: string should not be empty [empty-string]",
            r#"4:15: "branches" section should not be empty [empty-section]"#,
            "5:23: string should not be empty [empty-string]",
            r#"6:11: "tags" section must be sequence node but got mapping node with "!!map" tag [unexpected-node-kind]"#,
            "7:18: string should not be empty [empty-string]",
            r#"8:13: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            r#"10:16: "workflows" section should not be empty [empty-section]"#,
            r#"11:5: unexpected key "Types" for "push" section. expected one of "branches", "branches-ignore", "paths", "paths-ignore", "tags", "tags-ignore", "types", "workflows" [unexpected-key]"#,
            r#"12:17: "pull_request" section is scalar node but mapping node is expected [unexpected-node-kind]"#,
            r#"13:11: "issues" section is sequence node but mapping node is expected [unexpected-node-kind]"#,
            r#"16:5: key "types" is duplicated in "release" section. previously defined at line:15,col:5 [duplicate-key]"#,
            r#"webhook "push" at 2:3 body 3:5 types=invalid branches=invalid branches-ignore=[] tags=invalid tags-ignore=invalid paths=[] paths-ignore=[~] workflows=invalid"#,
            r#"webhook "pull_request" at 12:3 body 12:17"#,
            r#"webhook "issues" at 13:3 body 13:11"#,
            r#"webhook "release" at 14:3 body 15:5 types=[a]"#,
        ]
    );
}

#[test]
fn schedule_entries_have_cron_and_timezone() {
    assert_eq!(
        check(
            "on:\n  schedule:\n    - cron: '0 0 * * *'\n    - cron: '*/5 * * * *'\n      timezone: Europe/Oslo\n"
        ),
        [
            r#"schedule "schedule" at 2:3 body 3:5"#,
            r#"  entry at 3:7 cron="0 0 * * *""#,
            r#"  entry at 4:7 cron="*/5 * * * *" timezone="Europe/Oslo""#,
        ]
    );
}

#[test]
fn schedule_body_is_the_span_of_its_sequence() {
    with_workflow(
        "on:\n  schedule:\n    - cron: '0 0 * * *'\n",
        |file, workflow, _| {
            let Some(Event::Schedule(schedule)) = workflow.on.value().unwrap().iter().next() else {
                panic!("no schedule event");
            };
            let body: Span = schedule.body;
            assert_eq!(at(file, body), "3:5");
        },
    );
}

#[test]
fn schedule_entries_without_a_valid_cron_are_dropped() {
    assert_eq!(
        check(
            "on:\n  schedule:\n    - timezone: UTC\n    - cron: ''\n    - x\n    - cron: [a]\n      foo: 1\n    - cron: '0 0 * * *'\n      timezone:\n    - {}\n    - ~\n"
        ),
        [
            "4:13: string should not be empty [empty-string]",
            r#"5:7: element of "schedule" section is scalar node but mapping node is expected [unexpected-node-kind]"#,
            r#"6:13: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            r#"7:7: unexpected key "foo" for element of "schedule" section. expected one of "cron", "timezone" [unexpected-key]"#,
            "9:16: string should not be empty [empty-string]",
            r#"10:7: element of "schedule" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
            r#"11:7: element of "schedule" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
            r#"schedule "schedule" at 2:3 body 3:5"#,
            r#"  entry at 8:7 cron="0 0 * * *" timezone=invalid"#,
        ]
    );
}

#[test]
fn schedule_that_is_not_a_sequence_with_items_is_dropped() {
    let cases: [(&str, &str); 3] = [
        (
            "on:\n  schedule: {cron: x}\n",
            r#"2:13: "schedule" section must be sequence node but got mapping node with "!!map" tag [unexpected-node-kind]"#,
        ),
        (
            "on:\n  schedule: []\n",
            r#"2:13: "schedule" section should not be empty [empty-section]"#,
        ),
        (
            "on:\n  schedule:\n",
            r#"2:12: "schedule" section must be sequence node but got scalar node with "!!null" tag [unexpected-node-kind]"#,
        ),
    ];
    for (on, want) in cases {
        assert_eq!(check(on), [want], "{on:?}");
    }
}

#[test]
fn workflow_dispatch_inputs_keep_source_order_and_every_type() {
    assert_eq!(
        check(
            "on:\n  workflow_dispatch:\n    inputs:\n      Name:\n        description: ''\n        required: true\n        default: ''\n        type: choice\n        options: [a, b]\n      count:\n        required: ${{ inputs.x }}\n        type: number\n      flag: {type: boolean}\n      env: {type: environment}\n      text: {type: string}\n      bare:\n"
        ),
        [
            r#"dispatch "workflow_dispatch" at 2:3 body 3:5 inputs={Name, count, flag, env, text, bare}"#,
            r#"  input "Name" at 4:7 description="" required=true default="" type=Choice options=[a, b]"#,
            r#"  input "count" at 10:7 required=${{ inputs.x }} type=Number"#,
            r#"  input "flag" at 13:7 type=Boolean"#,
            r#"  input "env" at 14:7 type=Environment"#,
            r#"  input "text" at 15:7 type=String"#,
            r#"  input "bare" at 16:7"#,
        ]
    );
}

#[test]
fn workflow_dispatch_input_names_are_case_insensitive() {
    with_workflow(
        "on:\n  workflow_dispatch:\n    inputs:\n      MyInput:\n        type: string\n",
        |_, workflow, diagnostics| {
            assert_eq!(diagnostics, Vec::<String>::new());
            let Some(Event::WorkflowDispatch(event)) = workflow.on.value().unwrap().first() else {
                panic!("not workflow_dispatch");
            };
            let inputs = event.inputs.value().unwrap();
            assert_eq!(inputs.get("myinput").unwrap().key.value(), "MyInput");
        },
    );
}

#[test]
fn workflow_dispatch_inputs_report_invalid_keys_and_list_all_five() {
    assert_eq!(
        check(
            "on:\n  workflow_dispatch:\n    inputs:\n      a:\n        description: [x]\n        required: yes\n        default: {x: y}\n        type: unknown\n        options: []\n      B:\n        required: 1\n        type: ''\n        options: ['', [x]]\n      b:\n        type: string\n      c:\n        type: [string]\n        options: x\n        choices: [x]\n      d: x\n    foo: 1\n"
        ),
        [
            r#"5:22: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            r#"6:19: expecting a single ${{...}} expression or boolean literal "true" or "false", but found plain text node [expected-single-expression]"#,
            r#"7:18: expected scalar node for string value but found mapping node with "!!map" tag [unexpected-node-kind]"#,
            r#"8:15: input type of workflow_dispatch event must be one of "string", "number", "boolean", "choice", "environment" but got "unknown" [invalid-dispatch-input-type]"#,
            r#"9:18: "options" section should not be empty [empty-section]"#,
            r#"11:19: expected bool value but found scalar node with "!!int" tag [invalid-literal-value]"#,
            "12:15: string should not be empty [empty-string]",
            "13:19: string should not be empty [empty-string]",
            r#"13:23: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            r#"14:7: key "b" is duplicated in "inputs" section. previously defined at line:10,col:7. note that this key is case insensitive [duplicate-key]"#,
            r#"17:15: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            r#"18:18: "options" section must be sequence node but got scalar node with "!!str" tag [unexpected-node-kind]"#,
            r#"19:9: unexpected key "choices" for "inputs" section. expected one of "default", "description", "options", "required", "type" [unexpected-key]"#,
            r"20:10: input settings of workflow_dispatch event is scalar node but mapping node is expected [unexpected-node-kind]",
            r#"21:5: expected "inputs" key for "workflow_dispatch" section but got "foo" [unexpected-key]"#,
            r#"dispatch "workflow_dispatch" at 2:3 body 3:5 inputs={a, B, c, d}"#,
            r#"  input "a" at 4:7 description=invalid required=invalid default=invalid type=invalid options=invalid"#,
            r#"  input "B" at 10:7 required=invalid type=invalid options=[]"#,
            r#"  input "c" at 16:7 type=invalid options=invalid"#,
            r#"  input "d" at 20:7"#,
        ]
    );
}

#[test]
fn workflow_dispatch_with_a_body_or_inputs_that_is_not_a_mapping() {
    assert_eq!(
        check("on:\n  workflow_dispatch:\n    inputs: [a]\n"),
        [
            r#"3:13: "inputs" section is sequence node but mapping node is expected [unexpected-node-kind]"#,
            r#"dispatch "workflow_dispatch" at 2:3 body 3:5 inputs=invalid"#,
        ]
    );
    assert_eq!(
        check("on:\n  workflow_dispatch: x\n"),
        [
            r#"2:22: "workflow_dispatch" section is scalar node but mapping node is expected [unexpected-node-kind]"#,
            r#"dispatch "workflow_dispatch" at 2:3 body 2:22"#,
        ]
    );
}

#[test]
fn repository_dispatch_types_take_a_string_or_a_sequence() {
    assert_eq!(
        check("on:\n  repository_dispatch:\n    types: [a, b]\n"),
        [r#"repository "repository_dispatch" at 2:3 body 3:5 types=[a, b]"#]
    );
    assert_eq!(
        check("on:\n  repository_dispatch:\n    types: a\n"),
        [r#"repository "repository_dispatch" at 2:3 body 3:5 types=[a]"#]
    );
}

#[test]
fn repository_dispatch_types_must_not_be_empty() {
    assert_eq!(
        check("on:\n  repository_dispatch:\n    types: []\n    events: x\n"),
        [
            r#"3:12: "types" section should not be empty [empty-section]"#,
            r#"4:5: expected "types" key for "repository_dispatch" section but got "events" [unexpected-key]"#,
            r#"repository "repository_dispatch" at 2:3 body 3:5 types=invalid"#,
        ]
    );
    assert_eq!(
        check("on:\n  repository_dispatch:\n    types:\n"),
        [
            "3:11: string should not be empty [empty-string]",
            r#"repository "repository_dispatch" at 2:3 body 3:5 types=invalid"#,
        ]
    );
}

#[test]
fn workflow_call_has_inputs_secrets_and_outputs() {
    assert_eq!(
        check(
            "on:\n  workflow_call:\n    inputs:\n      Version:\n        description: ''\n        required: true\n        default: ''\n        type: string\n      n: {type: number, required: false, default: 1}\n      b: {type: boolean}\n    secrets:\n      Token:\n        description: x\n        required: true\n      other:\n    outputs:\n      Out:\n        description: x\n        value: ${{ jobs.t.outputs.x }}\n"
        ),
        [
            r#"call "workflow_call" at 2:3 body 3:5 inputs={Version, n, b} secrets={Token, other} outputs={Out}"#,
            r#"  input "Version" at 4:7 description="" required=true default="" type=String"#,
            r#"  input "n" at 9:7 required=false default="1" type=Number"#,
            r#"  input "b" at 10:7 type=Boolean"#,
            r#"  secret "Token" at 12:7 description="x" required=true"#,
            r#"  secret "other" at 15:7"#,
            r#"  output "Out" at 17:7 description="x" value="${{ jobs.t.outputs.x }}""#,
        ]
    );
}

#[test]
fn workflow_call_reports_invalid_keys_and_missing_type_and_value() {
    assert_eq!(
        check(
            "on:\n  workflow_call:\n    inputs:\n      a:\n        description: [x]\n        required: yes\n        default: [x]\n        type: object\n      b:\n      c: x\n      A:\n        type: string\n      d:\n        type: ''\n      e:\n        type: {x: y}\n        unknown: 1\n    secrets:\n      s:\n        description: [x]\n        required: 1\n        value: x\n      S:\n      t: x\n    outputs:\n      o:\n        description: [x]\n      p:\n        value:\n        x: 1\n      O:\n        value: x\n      q: x\n    other: 1\n"
        ),
        [
            r#"5:22: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            r#"6:19: expecting a single ${{...}} expression or boolean literal "true" or "false", but found plain text node [expected-single-expression]"#,
            r#"7:18: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            r#"8:15: invalid value "object" for input type of workflow_call event. it must be one of "boolean", "number", or "string" [invalid-workflow-call-input-type]"#,
            r#"9:7: "type" is missing at "b" input of workflow_call event [missing-required-key]"#,
            r#"10:7: "type" is missing at "c" input of workflow_call event [missing-required-key]"#,
            r"10:10: input of workflow_call event is scalar node but mapping node is expected [unexpected-node-kind]",
            r#"11:7: key "A" is duplicated in "inputs" section. previously defined at line:4,col:7. note that this key is case insensitive [duplicate-key]"#,
            "14:15: string should not be empty [empty-string]",
            r#"16:15: expected scalar node for string value but found mapping node with "!!map" tag [unexpected-node-kind]"#,
            r#"17:9: unexpected key "unknown" for inputs at workflow_call event. expected one of "default", "description", "required", "type" [unexpected-key]"#,
            r#"20:22: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            r#"21:19: expected bool value but found scalar node with "!!int" tag [invalid-literal-value]"#,
            r#"22:9: unexpected key "value" for "secrets" section. expected one of "description", "required" [unexpected-key]"#,
            r#"23:7: key "S" is duplicated in "secrets" section. previously defined at line:19,col:7. note that this key is case insensitive [duplicate-key]"#,
            r"24:10: secret of workflow_call event is scalar node but mapping node is expected [unexpected-node-kind]",
            r#"26:7: "value" is missing at "o" output of workflow_call event [missing-required-key]"#,
            r#"27:22: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            "29:15: string should not be empty [empty-string]",
            r#"30:9: unexpected key "x" for outputs at workflow_call event. expected one of "description", "value" [unexpected-key]"#,
            r#"31:7: key "O" is duplicated in "outputs" section. previously defined at line:26,col:7. note that this key is case insensitive [duplicate-key]"#,
            r#"33:7: "value" is missing at "q" output of workflow_call event [missing-required-key]"#,
            r"33:10: output of workflow_call event is scalar node but mapping node is expected [unexpected-node-kind]",
            r#"34:5: unexpected key "other" for "workflow_call" section. expected one of "inputs", "outputs", "secrets" [unexpected-key]"#,
            r#"call "workflow_call" at 2:3 body 3:5 inputs={a, b, c, d, e} secrets={s, t} outputs={o, p, q}"#,
            r#"  input "a" at 4:7 description=invalid required=invalid default=invalid type=invalid"#,
            r#"  input "b" at 9:7"#,
            r#"  input "c" at 10:7"#,
            r#"  input "d" at 13:7 type=invalid"#,
            r#"  input "e" at 15:7 type=invalid"#,
            r#"  secret "s" at 19:7 description=invalid required=invalid"#,
            r#"  secret "t" at 24:7"#,
            r#"  output "o" at 26:7 description=invalid"#,
            r#"  output "p" at 28:7 value=invalid"#,
            r#"  output "q" at 33:7"#,
        ]
    );
}

#[test]
fn workflow_call_secrets_that_are_absent_differ_from_empty_ones() {
    let secrets = |on: &str| {
        with_workflow(on, |_, workflow, diagnostics| {
            assert_eq!(diagnostics, Vec::<String>::new(), "{on:?}");
            let call = workflow.workflow_call_event().unwrap();
            (
                call.secrets.is_present(),
                call.secrets.value().map(Keyed::len),
            )
        })
    };
    assert_eq!(secrets("on: workflow_call\n"), (false, None));
    assert_eq!(secrets("on:\n  workflow_call:\n"), (false, None));
    assert_eq!(
        secrets("on:\n  workflow_call:\n    inputs: {}\n"),
        (false, None)
    );
    assert_eq!(
        secrets("on:\n  workflow_call:\n    secrets:\n"),
        (true, Some(0))
    );
    assert_eq!(
        secrets("on:\n  workflow_call:\n    secrets: {}\n"),
        (true, Some(0))
    );
}

#[test]
fn workflow_call_sections_that_are_not_mappings_are_invalid() {
    assert_eq!(
        check("on:\n  workflow_call:\n    secrets: inherit\n    inputs: [a]\n    outputs: x\n"),
        [
            r#"3:14: "secrets" section is scalar node but mapping node is expected [unexpected-node-kind]"#,
            r#"4:13: "inputs" section is sequence node but mapping node is expected [unexpected-node-kind]"#,
            r#"5:14: "outputs" section is scalar node but mapping node is expected [unexpected-node-kind]"#,
            r#"call "workflow_call" at 2:3 body 3:5 inputs=invalid secrets=invalid outputs=invalid"#,
        ]
    );
}

#[test]
fn image_version_has_names_and_versions() {
    assert_eq!(
        check("on:\n  image_version:\n    names: [MyImage]\n    versions: ['1.0', '2.*']\n"),
        [r#"image "image_version" at 2:3 body 3:5 names=[MyImage] versions=[1.0, 2.*]"#]
    );
}

#[test]
fn image_version_names_and_versions_are_non_empty_sequences() {
    assert_eq!(
        check(
            "on:\n  image_version:\n    names: MyImage\n    versions: ['']\n    entrypoint: run.sh\n"
        ),
        [
            r#"3:12: "names" section must be sequence node but got scalar node with "!!str" tag [unexpected-node-kind]"#,
            "4:16: string should not be empty [empty-string]",
            r#"5:5: unexpected key "entrypoint" for "image_version" section. expected one of "names", "versions" [unexpected-key]"#,
            r#"image "image_version" at 2:3 body 3:5 names=invalid versions=[]"#,
        ]
    );
    assert_eq!(
        check("on:\n  image_version:\n    names: []\n    versions:\n"),
        [
            r#"3:12: "names" section should not be empty [empty-section]"#,
            r#"4:14: "versions" section must be sequence node but got scalar node with "!!null" tag [unexpected-node-kind]"#,
            r#"image "image_version" at 2:3 body 3:5 names=invalid versions=invalid"#,
        ]
    );
}
