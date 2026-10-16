#![allow(clippy::unwrap_used, clippy::panic)]

use workflint_diagnostics::{FileId, SourceFile, Span};

use crate::testing::render;
use crate::{Exec, ExecAction, ExecRun, Field, Keyed, Step, Str, parse};

/// The workflow that each test's steps are appended to. The first step is
/// on line 6, and its first key is at column 9.
const PREFIX: &str = "on: push\njobs:\n  test:\n    runs-on: x\n    steps:\n";

const RUN_STEP_KEYS: &str = r#""continue-on-error", "env", "id", "if", "name", "run", "shell", "timeout-minutes", "working-directory""#;

const ACTION_STEP_KEYS: &str =
    r#""continue-on-error", "env", "id", "if", "name", "timeout-minutes", "uses", "with""#;

const NO_RUN_OR_USES: &str = r#"step must run script with "run" section or run action with "uses" section [step-without-run-or-uses]"#;

/// Parses the workflow `text` and returns its diagnostics as [`render`]
/// prints them.
fn check(text: &str) -> Vec<String> {
    let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    render(&file, parse(&file, &document).diagnostics)
}

/// Returns the diagnostics of a workflow whose only job has `steps`.
fn check_steps(steps: &str) -> Vec<String> {
    check(&format!("{PREFIX}{steps}"))
}

/// Parses a workflow whose only job has `steps`, asserts that it has no
/// diagnostics, and runs `f` on the job's steps.
fn parsed_steps(steps: &str, f: impl FnOnce(&SourceFile, &[Step<'_>])) {
    let text = format!("{PREFIX}{steps}");
    let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    let parsed = parse(&file, &document);
    assert_eq!(render(&file, parsed.diagnostics), Vec::<String>::new());
    let job = &parsed
        .workflow
        .jobs
        .value()
        .unwrap()
        .get("test")
        .unwrap()
        .value;
    f(&file, job.steps.value().unwrap());
}

/// Formats the start of `span` as `line:column`.
fn pos(file: &SourceFile, span: Span) -> String {
    let pos = file.line_col(span.start);
    format!("{}:{}", pos.line, pos.column)
}

fn string(field: &Field<'_, Str<'_>>) -> Option<String> {
    field.value().map(|s| s.value().to_owned())
}

fn keyed_strings(keyed: &Keyed<'_, Option<Str<'_>>>) -> Vec<String> {
    keyed
        .iter()
        .map(|entry| {
            let value = entry.value.map_or("-", |value| value.value());
            format!("{}={value}", entry.key.value())
        })
        .collect()
}

fn run<'s, 'a>(step: &'s Step<'a>) -> &'s ExecRun<'a> {
    match &step.exec {
        Some(Exec::Run(run)) => run,
        other => panic!("not a run step: {other:?}"),
    }
}

fn action<'s, 'a>(step: &'s Step<'a>) -> &'s ExecAction<'a> {
    match &step.exec {
        Some(Exec::Action(action)) => action,
        other => panic!("not an action step: {other:?}"),
    }
}

#[test]
fn a_run_step_has_run_shell_and_working_directory() {
    let steps = "      - run: echo hi
        shell: bash
        working-directory: dir
";
    parsed_steps(steps, |file, steps| {
        let [step] = steps else {
            panic!("{steps:?}");
        };
        assert_eq!(pos(file, step.span), "6:9");
        let run = run(step);
        assert_eq!(string(&run.run).as_deref(), Some("echo hi"));
        // The key's span is the reference's `RunPos`.
        assert_eq!(pos(file, run.run.key().unwrap().span()), "6:9");
        assert_eq!(string(&run.shell).as_deref(), Some("bash"));
        assert_eq!(string(&run.working_directory).as_deref(), Some("dir"));
    });
}

#[test]
fn an_action_step_has_uses_and_with_with_case_insensitive_keys() {
    let steps = "      - uses: actions/checkout@v6
        with:
          Fetch-Depth: 0
          entrypoint: e
          args: ''
";
    parsed_steps(steps, |_, steps| {
        let checkout = action(&steps[0]);
        assert_eq!(
            string(&checkout.uses).as_deref(),
            Some("actions/checkout@v6")
        );
        let inputs = checkout.inputs.value().unwrap();
        // `entrypoint` and `args` are inputs of an action that is not a
        // Docker image.
        assert_eq!(
            keyed_strings(inputs),
            ["Fetch-Depth=0", "entrypoint=e", "args="]
        );
        assert!(inputs.contains_key("fetch-depth"));
        assert_eq!(checkout.entrypoint, Field::Absent);
        assert_eq!(checkout.args, Field::Absent);
    });
}

#[test]
fn a_docker_action_step_moves_entrypoint_and_args_out_of_with() {
    let steps = "      - uses: 'docker://ubuntu:latest'
        with:
          Entrypoint: /bin/bash
          ARGS: ''
          foo: Hello
      - uses: docker://alpine
";
    parsed_steps(steps, |_, steps| {
        let docker = action(&steps[0]);
        assert_eq!(string(&docker.entrypoint).as_deref(), Some("/bin/bash"));
        assert_eq!(docker.entrypoint.key().map(Str::value), Some("Entrypoint"));
        // `args` may be empty.
        assert_eq!(string(&docker.args).as_deref(), Some(""));
        assert_eq!(keyed_strings(docker.inputs.value().unwrap()), ["foo=Hello"]);
        let bare = action(&steps[1]);
        assert_eq!(bare.inputs, Field::Absent);
        assert_eq!(bare.entrypoint, Field::Absent);
    });
}

#[test]
fn a_docker_entrypoint_must_be_a_non_empty_string_and_args_a_string() {
    let steps = "      - uses: docker://alpine
        with: {entrypoint: '', args: ''}
      - uses: docker://alpine
        with: {entrypoint: [a], args: [b]}
";
    assert_eq!(
        check_steps(steps),
        [
            "7:28: string should not be empty [empty-string]",
            r#"9:28: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            r#"9:39: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
        ]
    );
}

#[test]
fn with_must_be_a_mapping_of_strings_with_entries() {
    let steps = "      - uses: a/b@v1
        with: foo
      - uses: a/b@v1
        with:
      - uses: a/b@v1
        with: {}
      - uses: a/b@v1
        with: {A: 1, a: 2, b: '', c: [x]}
";
    assert_eq!(
        check_steps(steps),
        [
            r#"7:15: "with" section is scalar node but mapping node is expected [unexpected-node-kind]"#,
            r#"9:14: "with" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
            r#"11:15: "with" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
            r#"13:22: key "a" is duplicated in "with" section. previously defined at line:13,col:16. note that this key is case insensitive [duplicate-key]"#,
            r#"13:38: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
        ]
    );
}

#[test]
fn the_later_of_run_and_uses_decides_the_kind_and_the_other_is_unexpected() {
    let steps = "      - run: echo hello
        uses: actions/checkout@v6
      - uses: actions/checkout@v6
        run: echo hello
      - run: echo hello
        uses: docker://alpine
        with: {args: x}
";
    assert_eq!(
        check_steps(steps),
        [
            format!(
                r#"6:9: unexpected key "run" for step to execute action. expected one of {ACTION_STEP_KEYS} [unexpected-key]"#
            ),
            format!(
                r#"8:9: unexpected key "uses" for step to run shell command. expected one of {RUN_STEP_KEYS} [unexpected-key]"#
            ),
            format!(
                r#"10:9: unexpected key "run" for step to execute action. expected one of {ACTION_STEP_KEYS} [unexpected-key]"#
            ),
        ]
    );
}

#[test]
fn the_later_of_run_and_uses_gives_the_steps_exec() {
    let text = format!(
        "{PREFIX}      - run: echo hello
        uses: docker://alpine
        with: {{args: x}}
      - uses: a/b@v1
        run: echo hello
"
    );
    let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    let parsed = parse(&file, &document);
    let job = &parsed
        .workflow
        .jobs
        .value()
        .unwrap()
        .get("test")
        .unwrap()
        .value;
    let steps = job.steps.value().unwrap();
    let docker = action(&steps[0]);
    assert_eq!(string(&docker.args).as_deref(), Some("x"));
    assert_eq!(
        run(&steps[1]).run.value().map(Str::value),
        Some("echo hello")
    );
    // The key that lost is not parsed.
    assert_eq!(run(&steps[1]).shell, Field::Absent);
}

#[test]
fn a_step_with_neither_run_nor_uses_is_reported_at_the_step() {
    let steps = "      - foo: aaa
        with: {x: y}
        shell: ''
      - null
      -
      - {}
      - foo
      - [run]
";
    // The keys of such a step are not checked, as in the reference.
    assert_eq!(
        check_steps(steps),
        [
            format!("6:9: {NO_RUN_OR_USES}"),
            r#"9:9: element of "steps" section should not be empty. please remove this section if it's unnecessary [empty-section]"#.to_owned(),
            format!("9:9: {NO_RUN_OR_USES}"),
            r#"10:8: element of "steps" section should not be empty. please remove this section if it's unnecessary [empty-section]"#.to_owned(),
            format!("10:8: {NO_RUN_OR_USES}"),
            r#"11:9: element of "steps" section should not be empty. please remove this section if it's unnecessary [empty-section]"#.to_owned(),
            format!("11:9: {NO_RUN_OR_USES}"),
            format!("12:9: {NO_RUN_OR_USES}"),
            r#"12:9: element of "steps" section is scalar node but mapping node is expected [unexpected-node-kind]"#.to_owned(),
            format!("13:9: {NO_RUN_OR_USES}"),
            r#"13:9: element of "steps" section is sequence node but mapping node is expected [unexpected-node-kind]"#.to_owned(),
        ]
    );
}

#[test]
fn steps_keep_source_order_and_steps_without_run_or_uses() {
    let text = format!("{PREFIX}      - run: a\n      - foo\n      - uses: b/c@v1\n");
    let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    let parsed = parse(&file, &document);
    let job = &parsed
        .workflow
        .jobs
        .value()
        .unwrap()
        .get("test")
        .unwrap()
        .value;
    let steps = job.steps.value().unwrap();
    assert_eq!(steps.len(), 3);
    assert_eq!(string(&run(&steps[0]).run).as_deref(), Some("a"));
    assert_eq!(steps[1].exec, None);
    assert_eq!(pos(&file, steps[1].span), "7:9");
    assert_eq!(string(&action(&steps[2]).uses).as_deref(), Some("b/c@v1"));
}

#[test]
fn step_keys_are_case_sensitive() {
    let steps = "      - Run: x
      - Uses: x
        run: y
      - run: x
        run: y
";
    assert_eq!(
        check_steps(steps),
        [
            format!("6:9: {NO_RUN_OR_USES}"),
            format!(
                r#"7:9: unexpected key "Uses" for step to run shell command. expected one of {RUN_STEP_KEYS} [unexpected-key]"#
            ),
            r#"10:9: key "run" is duplicated in element of "steps" section. previously defined at line:9,col:9 [duplicate-key]"#.to_owned(),
        ]
    );
}

#[test]
fn each_kind_of_step_rejects_the_keys_of_the_other() {
    let steps = "      - run: x
        with: {foo: bar}
        foooo: unknown
        [a]: b
      - uses: a/b@v1
        shell: bash
        working-directory: dir
        foooo: unknown
";
    assert_eq!(
        check_steps(steps),
        [
            format!(
                r#"7:9: unexpected key "with" for step to run shell command. expected one of {RUN_STEP_KEYS} [unexpected-key]"#
            ),
            format!(
                r#"8:9: unexpected key "foooo" for step to run shell command. expected one of {RUN_STEP_KEYS} [unexpected-key]"#
            ),
            format!(
                r#"9:9: unexpected key "" for step to run shell command. expected one of {RUN_STEP_KEYS} [unexpected-key]"#
            ),
            r#"9:9: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#.to_owned(),
            format!(
                r#"11:9: unexpected key "shell" for step to execute action. expected one of {ACTION_STEP_KEYS} [unexpected-key]"#
            ),
            format!(
                r#"12:9: unexpected key "working-directory" for step to execute action. expected one of {ACTION_STEP_KEYS} [unexpected-key]"#
            ),
            format!(
                r#"13:9: unexpected key "foooo" for step to execute action. expected one of {ACTION_STEP_KEYS} [unexpected-key]"#
            ),
        ]
    );
}

#[test]
fn run_uses_shell_and_working_directory_must_be_non_empty_strings() {
    let steps = "      - run: ''
      - uses: ''
      - uses: [a]
      - run: [a]
      - run: x
        shell: ''
        working-directory: ''
";
    // A `uses` that is not a scalar still makes an action step.
    assert_eq!(
        check_steps(steps),
        [
            "6:14: string should not be empty [empty-string]",
            "7:15: string should not be empty [empty-string]",
            r#"8:15: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            r#"9:14: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            "11:16: string should not be empty [empty-string]",
            "12:28: string should not be empty [empty-string]",
        ]
    );
}

#[test]
fn the_common_keys_are_parsed_in_both_kinds_of_step() {
    let steps = "      - run: x
        id: build
        if: success()
        name: ''
        env: {A: a}
        continue-on-error: true
        timeout-minutes: 1.5
      - uses: a/b@v1
        id: other
        env: ${{ fromJSON(x) }}
        continue-on-error: ${{ x }}
        timeout-minutes: ${{ x }}
";
    parsed_steps(steps, |_, steps| {
        let step = &steps[0];
        assert_eq!(string(&step.id).as_deref(), Some("build"));
        assert_eq!(string(&step.condition).as_deref(), Some("success()"));
        // `name` may be empty.
        assert_eq!(string(&step.name).as_deref(), Some(""));
        assert_eq!(keyed_strings(step.env.literal().unwrap()), ["A=a"]);
        assert_eq!(step.continue_on_error.literal(), Some(&true));
        assert_eq!(step.timeout_minutes.literal(), Some(&1.5));
        let step = &steps[1];
        assert_eq!(string(&step.id).as_deref(), Some("other"));
        assert!(step.env.expression().is_some());
        assert!(step.continue_on_error.expression().is_some());
        assert!(step.timeout_minutes.expression().is_some());
        assert_eq!(step.condition, Field::Absent);
        assert_eq!(step.name, Field::Absent);
    });
}

#[test]
fn the_common_keys_report_invalid_values() {
    let steps = "      - run: x
        id: ''
        if: ''
        name: [a]
      - run: x
        env: []
      - run: x
        env: {}
      - run: x
        continue-on-error: yes
      - run: x
        timeout-minutes: abc
";
    assert_eq!(
        check_steps(steps),
        [
            "7:13: string should not be empty [empty-string]",
            "8:13: string should not be empty [empty-string]",
            r#"9:15: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            r#"11:14: "env" section is sequence node but mapping node is expected [unexpected-node-kind]"#,
            r#"13:14: "env" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
            r#"15:28: expecting a single ${{...}} expression or boolean literal "true" or "false", but found plain text node [expected-single-expression]"#,
            r"17:26: expecting a single ${{...}} expression or float number literal, but found plain text node [expected-single-expression]",
        ]
    );
}

#[test]
fn timeout_minutes_of_a_step_must_be_greater_than_zero() {
    let steps = "      - run: x
        timeout-minutes: 0
      - run: x
        timeout-minutes: -1.5
";
    assert_eq!(
        check_steps(steps),
        [
            r#"7:26: value at "timeout-minutes" must be greater than zero: 0 [non-positive-number]"#,
            r#"9:26: value at "timeout-minutes" must be greater than zero: -1.5 [non-positive-number]"#,
        ]
    );
}

#[test]
fn a_non_positive_timeout_is_kept_as_a_literal() {
    let text = format!("{PREFIX}      - run: x\n        timeout-minutes: -2\n");
    let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    let parsed = parse(&file, &document);
    let job = &parsed
        .workflow
        .jobs
        .value()
        .unwrap()
        .get("test")
        .unwrap()
        .value;
    let step = &job.steps.value().unwrap()[0];
    assert_eq!(
        step.timeout_minutes.literal().map(ToString::to_string),
        Some("-2".to_owned())
    );
}

#[test]
fn steps_must_be_a_non_empty_sequence_and_otherwise_count_as_missing() {
    let text = "\
on: push
jobs:
  a:
    runs-on: x
    steps: []
  b:
    runs-on: x
    steps: foo
  c:
    runs-on: x
    steps:
";
    assert_eq!(
        check(text),
        [
            r#"3:3: "steps" section is missing in job "a" [missing-required-key]"#,
            r#"5:12: "steps" section should not be empty [empty-section]"#,
            r#"6:3: "steps" section is missing in job "b" [missing-required-key]"#,
            r#"8:12: "steps" section must be sequence node but got scalar node with "!!str" tag [unexpected-node-kind]"#,
            r#"9:3: "steps" section is missing in job "c" [missing-required-key]"#,
            r#"11:11: "steps" section must be sequence node but got scalar node with "!!null" tag [unexpected-node-kind]"#,
        ]
    );
}

#[test]
fn invalid_steps_are_kept_as_an_invalid_field() {
    let text = "on: push\njobs:\n  test:\n    runs-on: x\n    steps: foo\n";
    let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    let parsed = parse(&file, &document);
    let job = &parsed
        .workflow
        .jobs
        .value()
        .unwrap()
        .get("test")
        .unwrap()
        .value;
    assert!(matches!(job.steps, Field::Invalid { .. }));
    assert_eq!(job.steps.key().map(Str::value), Some("steps"));
}

#[test]
fn steps_of_a_reusable_workflow_call_are_parsed_and_reported() {
    let text = "\
on: push
jobs:
  test:
    uses: o/r/.github/workflows/w.yml@v1
    steps:
      - foo
";
    assert_eq!(
        check(text),
        [
            r#"5:5: when a reusable workflow is called with "uses", "steps" is not available. only following keys are allowed: "name", "uses", "with", "secrets", "needs", "if", and "permissions" in job "test" [invalid-reusable-call-key]"#.to_owned(),
            format!("6:9: {NO_RUN_OR_USES}"),
            r#"6:9: element of "steps" section is scalar node but mapping node is expected [unexpected-node-kind]"#.to_owned(),
        ]
    );
}
