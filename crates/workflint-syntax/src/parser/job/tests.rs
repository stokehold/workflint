#![allow(clippy::unwrap_used, clippy::panic)]

use workflint_diagnostics::{FileId, SourceFile};

use crate::testing::{Text, render};
use crate::{
    Field, Job, JobUses, JobUsesKind, Keyed, Permissions, Str, Template, Value, Workflow,
    WorkflowCallSecrets, parse,
};

/// Parses the workflow `text` and returns its diagnostics as
/// [`render`] prints them.
fn check(text: &str) -> Vec<String> {
    let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    render(&file, parse(&file, &document).diagnostics)
}

/// Parses the workflow `text`, asserts that it has no diagnostics, and runs
/// `f` on its workflow.
fn parsed(text: &str, f: impl FnOnce(&Workflow<'_>)) {
    let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    let parsed = parse(&file, &document);
    assert_eq!(render(&file, parsed.diagnostics), Vec::<String>::new());
    f(&parsed.workflow);
}

fn jobs<'w, 'a>(workflow: &'w Workflow<'a>) -> &'w Keyed<'a, Job<'a>> {
    workflow.jobs.value().unwrap()
}

fn job<'w, 'a>(workflow: &'w Workflow<'a>, id: &str) -> &'w Job<'a> {
    &jobs(workflow).get(id).unwrap().value
}

fn strings(strings: &[impl Text]) -> Vec<String> {
    strings.iter().map(|s| s.text().to_owned()).collect()
}

fn keyed_strings(keyed: &Keyed<'_, impl Text>) -> Vec<String> {
    keyed
        .iter()
        .map(|entry| format!("{}={}", entry.key.value(), entry.value.text()))
        .collect()
}

const ALL_JOB_KEYS: &str = r#""concurrency", "container", "continue-on-error", "defaults", "env", "environment", "if", "name", "needs", "outputs", "permissions", "runs-on", "secrets", "services", "snapshot", "steps", "strategy", "timeout-minutes", "uses", "with""#;

const ALLOWED_CALL_KEYS: &str = r#"only following keys are allowed: "name", "uses", "with", "secrets", "needs", "if", and "permissions""#;

#[test]
fn jobs_keep_source_order() {
    let text = "\
on: push
jobs:
  zeta:
    runs-on: x
    steps: [{run: a}]
  Alpha:
    runs-on: x
    steps: [{run: a}]
  mid:
    runs-on: x
    steps: [{run: a}]
";
    parsed(text, |workflow| {
        let ids: Vec<_> = jobs(workflow).iter().map(|e| e.key.value()).collect();
        assert_eq!(ids, ["zeta", "Alpha", "mid"]);
        assert_eq!(workflow.jobs.key().map(Str::value), Some("jobs"));
        assert_eq!(jobs(workflow).get("alpha").unwrap().key.value(), "Alpha");
    });
}

#[test]
fn a_job_id_that_differs_only_in_case_is_a_duplicate() {
    let text = "\
on: push
jobs:
  test:
    runs-on: x
    steps: [{run: a}]
  TEST:
    runs-on: x
    steps: [{run: a}]
    foo: bar
";
    // The duplicate's body is not parsed, so its unexpected key is not
    // reported.
    assert_eq!(
        check(text),
        [
            r#"6:3: key "TEST" is duplicated in "jobs" section. previously defined at line:3,col:3. note that this key is case insensitive [duplicate-key]"#
        ]
    );
}

#[test]
fn jobs_must_be_a_mapping_with_entries() {
    let cases: [(&str, &str); 4] = [
        (
            "on: push\njobs:\n",
            r#"2:6: "jobs" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
        ),
        (
            "on: push\njobs: {}\n",
            r#"2:7: "jobs" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
        ),
        (
            "on: push\njobs: []\n",
            r#"2:7: "jobs" section is sequence node but mapping node is expected [unexpected-node-kind]"#,
        ),
        (
            "on: push\njobs: x\n",
            r#"2:7: "jobs" section is scalar node but mapping node is expected [unexpected-node-kind]"#,
        ),
    ];
    // `jobs` is written in each case, so it is not missing.
    for (text, want) in cases {
        assert_eq!(check(text), [want], "{text:?}");
    }
}

#[test]
fn a_job_that_is_not_a_mapping_misses_runs_on_and_steps() {
    let text = "\
on: push
jobs:
  null:
  sequence: []
  scalar: x
  ? [key]
  : {runs-on: x}
";
    assert_eq!(
        check(text),
        [
            r#"3:3: "runs-on" section is missing in job "null" [missing-required-key]"#,
            r#"3:3: "steps" section is missing in job "null" [missing-required-key]"#,
            r#"3:8: "null" job should not be empty. please remove this section if it's unnecessary [empty-section]"#,
            r#"4:3: "runs-on" section is missing in job "sequence" [missing-required-key]"#,
            r#"4:3: "steps" section is missing in job "sequence" [missing-required-key]"#,
            r#"4:13: "sequence" job is sequence node but mapping node is expected [unexpected-node-kind]"#,
            r#"5:3: "runs-on" section is missing in job "scalar" [missing-required-key]"#,
            r#"5:3: "steps" section is missing in job "scalar" [missing-required-key]"#,
            r#"5:11: "scalar" job is scalar node but mapping node is expected [unexpected-node-kind]"#,
            r#"6:5: "steps" section is missing in job "" [missing-required-key]"#,
            r#"6:5: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
        ]
    );
}

#[test]
fn a_job_whose_id_is_not_a_scalar_is_parsed_but_not_kept() {
    let file = SourceFile::new(
        FileId::new(0),
        "test.yml",
        "on: push\njobs:\n  ? [key]\n  : {runs-on: x, steps: [], foo: 1}\n",
    )
    .unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    let parsed = parse(&file, &document);
    assert!(jobs(&parsed.workflow).is_empty());
    let found = render(&file, parsed.diagnostics);
    assert!(
        found
            .iter()
            .any(|line| line.starts_with(r#"4:29: unexpected key "foo" for "job" section"#)),
        "{found:?}"
    );
}

#[test]
fn job_keys_are_case_sensitive_and_listed_when_unexpected() {
    let text = "\
on: push
jobs:
  test:
    Runs-on: x
    runs-on: y
    steps: [{run: a}]
    foo: bar
";
    assert_eq!(
        check(text),
        [
            format!(
                r#"4:5: unexpected key "Runs-on" for "job" section. expected one of {ALL_JOB_KEYS} [unexpected-key]"#
            ),
            format!(
                r#"7:5: unexpected key "foo" for "job" section. expected one of {ALL_JOB_KEYS} [unexpected-key]"#
            ),
        ]
    );
}

#[test]
fn a_duplicate_job_key_is_reported_in_the_job() {
    let text = "\
on: push
jobs:
  test:
    runs-on: x
    runs-on: y
    steps: [{run: a}]
";
    assert_eq!(
        check(text),
        [
            r#"5:5: key "runs-on" is duplicated in "test" job. previously defined at line:4,col:5 [duplicate-key]"#
        ]
    );
}

#[test]
fn the_keys_parsed_by_later_tasks_are_accepted() {
    let text = "\
on: push
jobs:
  test:
    runs-on: x
    steps: [{run: a}]
    strategy: {matrix: {a: [b]}}
    container: c
    services: {redis: {image: redis}}
";
    assert_eq!(check(text), Vec::<String>::new());
}

#[test]
fn name_if_and_needs_are_parsed() {
    let text = "\
on: push
jobs:
  build:
    runs-on: x
    steps: [{run: a}]
  test:
    name: ''
    if: github.event_name == 'push'
    needs: build
    runs-on: x
    steps: [{run: a}]
  deploy:
    name: Deploy
    needs: [build, test]
    runs-on: x
    steps: [{run: a}]
";
    parsed(text, |workflow| {
        let test_job = job(workflow, "test");
        assert_eq!(test_job.name.value().map(Template::value), Some(""));
        assert_eq!(
            test_job.condition.value().map(Template::value),
            Some("github.event_name == 'push'")
        );
        assert_eq!(strings(test_job.needs.value().unwrap()), ["build"]);
        assert_eq!(test_job.needs.key().map(Str::value), Some("needs"));

        let deploy = job(workflow, "deploy");
        assert_eq!(deploy.name.value().map(Template::value), Some("Deploy"));
        assert_eq!(strings(deploy.needs.value().unwrap()), ["build", "test"]);
        assert!(!deploy.condition.is_present());
    });
}

#[test]
fn invalid_name_if_and_needs_are_reported() {
    let text = "\
on: push
jobs:
  a:
    runs-on: x
    steps: [{run: a}]
    needs:
  b:
    runs-on: x
    steps: [{run: a}]
    needs: []
  c:
    runs-on: x
    steps: [{run: a}]
    needs: [a, '', [x]]
  d:
    runs-on: x
    steps: [{run: a}]
    needs: {a: b}
  e:
    name: [x]
    if: ''
    runs-on: x
    steps: [{run: a}]
";
    assert_eq!(
        check(text),
        [
            "6:11: string should not be empty [empty-string]",
            r#"10:12: "needs" section should not be empty [empty-section]"#,
            "14:16: string should not be empty [empty-string]",
            r#"14:20: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            r#"18:12: "needs" section must be sequence node but got mapping node with "!!map" tag [unexpected-node-kind]"#,
            r#"20:11: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            "21:9: string should not be empty [empty-string]",
        ]
    );
}

#[test]
fn runs_on_takes_labels_an_expression_or_a_mapping() {
    let text = "\
on: push
jobs:
  string:
    runs-on: ubuntu-latest
    steps: [{run: a}]
  sequence:
    runs-on: [self-hosted, 'linux-${{ matrix.arch }}']
    steps: [{run: a}]
  expression:
    runs-on: ${{ inputs.runner }}
    steps: [{run: a}]
  mapping:
    runs-on:
      group: ubuntu-runners
      labels: x64
    steps: [{run: a}]
  mapping-expression:
    runs-on:
      labels: ${{ inputs.labels }}
    steps: [{run: a}]
  tagged:
    runs-on: !!int 1
    steps: [{run: a}]
";
    parsed(text, |workflow| {
        let string = &job(workflow, "string").runs_on;
        assert_eq!(string.key().map(Str::value), Some("runs-on"));
        let runner = string.value().unwrap();
        assert_eq!(runner.labels.key(), None);
        assert_eq!(strings(runner.labels.literal().unwrap()), ["ubuntu-latest"]);
        assert!(!runner.group.is_present());

        let sequence = job(workflow, "sequence").runs_on.value().unwrap();
        assert_eq!(
            strings(sequence.labels.literal().unwrap()),
            ["self-hosted", "linux-${{ matrix.arch }}"]
        );

        let expression = job(workflow, "expression").runs_on.value().unwrap();
        assert_eq!(
            expression.labels.expression().map(|e| e.source.value()),
            Some("${{ inputs.runner }}")
        );

        let mapping = job(workflow, "mapping").runs_on.value().unwrap();
        assert_eq!(
            mapping.group.value().map(Template::value),
            Some("ubuntu-runners")
        );
        assert_eq!(mapping.labels.key().map(Str::value), Some("labels"));
        assert_eq!(strings(mapping.labels.literal().unwrap()), ["x64"]);

        let labels = &job(workflow, "mapping-expression")
            .runs_on
            .value()
            .unwrap()
            .labels;
        assert_eq!(
            labels.expression().map(|e| e.source.value()),
            Some("${{ inputs.labels }}")
        );

        // A scalar with another tag is a label, as in the reference.
        let tagged = job(workflow, "tagged").runs_on.value().unwrap();
        assert_eq!(strings(tagged.labels.literal().unwrap()), ["1"]);
    });
}

#[test]
fn invalid_runs_on_is_reported() {
    let text = "\
on: push
jobs:
  null:
    runs-on:
    steps: [{run: a}]
  empty-sequence:
    runs-on: []
    steps: [{run: a}]
  items:
    runs-on: ['x64', '', [b]]
    steps: [{run: a}]
  empty-mapping:
    runs-on: {}
    steps: [{run: a}]
  keys:
    runs-on: {Labels: x, labels: [], group: [g], labels: y}
    steps: [{run: a}]
  labels-mapping:
    runs-on:
      labels: {a: b}
    steps: [{run: a}]
";
    assert_eq!(
        check(text),
        [
            "4:13: string should not be empty [empty-string]",
            r#"7:14: "runs-on" section should not be empty [empty-section]"#,
            "10:22: string should not be empty [empty-string]",
            r#"10:26: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            r#"13:14: "runs-on" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
            r#"16:15: unexpected key "Labels" for "runs-on" section. expected one of "group", "labels" [unexpected-key]"#,
            r#"16:34: "labels" section should not be empty [empty-section]"#,
            r#"16:45: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            r#"16:50: key "labels" is duplicated in "runs-on" section. previously defined at line:16,col:26 [duplicate-key]"#,
            r#"20:15: "labels" section must be sequence node but got mapping node with "!!map" tag [unexpected-node-kind]"#,
        ]
    );
}

#[test]
fn an_empty_or_non_scalar_label_is_kept_as_an_invalid_entry() {
    let file = SourceFile::new(
        FileId::new(0),
        "test.yml",
        "on: push\njobs:\n  test:\n    runs-on: [x64, '', [y]]\n    steps: [{run: a}]\n",
    )
    .unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    let parsed = parse(&file, &document);
    let runner = job(&parsed.workflow, "test").runs_on.value().unwrap();
    // The reference keeps both as `""`, and its `runner-label` rule reports
    // them again as unknown labels.
    assert_eq!(strings(runner.labels.literal().unwrap()), ["x64", "-", "-"]);
    assert_eq!(
        render(&file, parsed.diagnostics),
        [
            "4:20: string should not be empty [empty-string]",
            r#"4:24: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#
        ]
    );
}

#[test]
fn outputs_are_keyed_strings_that_may_be_empty() {
    let text = "\
on: push
jobs:
  test:
    runs-on: x
    steps: [{run: a}]
    outputs:
      Version: ${{ steps.v.outputs.version }}
      empty: ''
";
    parsed(text, |workflow| {
        let outputs = &job(workflow, "test").outputs;
        assert_eq!(outputs.key().map(Str::value), Some("outputs"));
        assert_eq!(
            keyed_strings(outputs.value().unwrap()),
            ["Version=${{ steps.v.outputs.version }}", "empty="]
        );
        assert!(outputs.value().unwrap().contains_key("version"));
    });
}

#[test]
fn invalid_outputs_are_reported_twice_when_empty() {
    let text = "\
on: push
jobs:
  test:
    runs-on: x
    steps: [{run: a}]
    outputs:
  b:
    runs-on: x
    steps: [{run: a}]
    outputs: {}
  c:
    runs-on: x
    steps: [{run: a}]
    outputs: [a]
  d:
    runs-on: x
    steps: [{run: a}]
    outputs:
      a: [x]
      A: c
";
    assert_eq!(
        check(text),
        [
            r#"6:13: "outputs" section should not be empty [empty-section]"#,
            r#"6:13: "outputs" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
            r#"10:14: "outputs" section should not be empty [empty-section]"#,
            r#"10:14: "outputs" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
            r#"14:14: "outputs" section should not be empty [empty-section]"#,
            r#"14:14: "outputs" section is sequence node but mapping node is expected [unexpected-node-kind]"#,
            r#"19:10: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            r#"20:7: key "A" is duplicated in "outputs" section. previously defined at line:19,col:7. note that this key is case insensitive [duplicate-key]"#,
        ]
    );
}

#[test]
fn timeout_minutes_and_continue_on_error_are_parsed() {
    let text = "\
on: push
jobs:
  literal:
    runs-on: x
    steps: [{run: a}]
    timeout-minutes: 3.5
    continue-on-error: true
  expression:
    runs-on: x
    steps: [{run: a}]
    timeout-minutes: ${{ inputs.timeout }}
    continue-on-error: ${{ matrix.experimental }}
";
    parsed(text, |workflow| {
        let literal = job(workflow, "literal");
        assert_eq!(literal.timeout_minutes.literal(), Some(&3.5));
        assert_eq!(
            literal.timeout_minutes.key().map(Str::value),
            Some("timeout-minutes")
        );
        assert_eq!(literal.continue_on_error.literal(), Some(&true));

        let expression = job(workflow, "expression");
        assert!(expression.timeout_minutes.expression().is_some());
        assert!(expression.continue_on_error.expression().is_some());
    });
}

#[test]
fn timeout_minutes_must_be_greater_than_zero() {
    let text = "\
on: push
jobs:
  test:
    runs-on: x
    steps: [{run: a}]
    timeout-minutes: 0
  b:
    runs-on: x
    steps: [{run: a}]
    timeout-minutes: -1e6
  c:
    runs-on: x
    steps: [{run: a}]
    timeout-minutes: -0
  d:
    runs-on: x
    steps: [{run: a}]
    timeout-minutes: -1234567
  e:
    runs-on: x
    steps: [{run: a}]
    timeout-minutes: -3.5
";
    assert_eq!(
        check(text),
        [
            r#"6:22: value at "timeout-minutes" must be greater than zero: 0 [non-positive-number]"#,
            r#"10:22: value at "timeout-minutes" must be greater than zero: -1e+06 [non-positive-number]"#,
            r#"14:22: value at "timeout-minutes" must be greater than zero: -0 [non-positive-number]"#,
            r#"18:22: value at "timeout-minutes" must be greater than zero: -1.234567e+06 [non-positive-number]"#,
            r#"22:22: value at "timeout-minutes" must be greater than zero: -3.5 [non-positive-number]"#,
        ]
    );
}

#[test]
fn a_non_positive_timeout_stays_a_literal() {
    let file = SourceFile::new(
        FileId::new(0),
        "test.yml",
        "on: push\njobs:\n  test:\n    runs-on: x\n    steps: [{run: a}]\n    timeout-minutes: 0\n",
    )
    .unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    let parsed = parse(&file, &document);
    assert_eq!(
        job(&parsed.workflow, "test").timeout_minutes.literal(),
        Some(&0.0)
    );
}

#[test]
fn invalid_timeout_minutes_and_continue_on_error_are_reported() {
    let text = "\
on: push
jobs:
  test:
    runs-on: x
    steps: [{run: a}]
    timeout-minutes: true
    continue-on-error: yes
  b:
    runs-on: x
    steps: [{run: a}]
    timeout-minutes: '3'
    continue-on-error: [x]
  c:
    runs-on: x
    steps: [{run: a}]
    timeout-minutes: -.inf
  d:
    runs-on: x
    steps: [{run: a}]
    timeout-minutes: [1]
";
    assert_eq!(
        check(text),
        [
            r#"6:22: expected scalar node for float value but found scalar node with "!!bool" tag [invalid-literal-value]"#,
            r#"7:24: expecting a single ${{...}} expression or boolean literal "true" or "false", but found plain text node [expected-single-expression]"#,
            "11:22: expecting a single ${{...}} expression or float number literal, but found plain text node [expected-single-expression]",
            r#"12:24: expected bool value but found sequence node with "!!seq" tag [invalid-literal-value]"#,
            r#"16:22: invalid float value: "-.inf": strconv.ParseFloat: parsing "-.inf": invalid syntax [invalid-literal-value]"#,
            r#"20:22: expected scalar node for float value but found sequence node with "!!seq" tag [invalid-literal-value]"#,
        ]
    );
}

#[test]
fn the_shared_sections_are_parsed_in_a_job() {
    let text = "\
on: push
jobs:
  test:
    runs-on: x
    steps: [{run: a}]
    permissions: {contents: read}
    environment: production
    concurrency: {group: ci, cancel-in-progress: true}
    env:
      CI: 'true'
    defaults:
      run:
        shell: bash
";
    parsed(text, |workflow| {
        let test_job = job(workflow, "test");
        assert!(matches!(
            test_job.permissions.value(),
            Some(Permissions::Scopes(scopes)) if scopes.contains_key("contents")
        ));
        assert_eq!(
            test_job.permissions.key().map(Str::value),
            Some("permissions")
        );
        let environment = test_job.environment.value().unwrap();
        assert_eq!(
            environment.name.value().map(Template::value),
            Some("production")
        );
        assert_eq!(
            test_job.environment.key().map(Str::value),
            Some("environment")
        );
        let concurrency = test_job.concurrency.value().unwrap();
        assert_eq!(concurrency.group.value().map(Template::value), Some("ci"));
        assert_eq!(concurrency.cancel_in_progress.literal(), Some(&true));
        assert_eq!(keyed_strings(test_job.env.literal().unwrap()), ["CI=true"]);
        let run = test_job.defaults.value().unwrap().run.value().unwrap();
        assert_eq!(run.shell.value().map(Template::value), Some("bash"));
    });
}

#[test]
fn errors_in_the_shared_sections_are_reported_in_a_job() {
    let text = "\
on: push
jobs:
  test:
    runs-on: x
    steps: [{run: a}]
    permissions:
    environment:
      url: https://example.com
    concurrency: {}
    env:
    defaults: {}
";
    assert_eq!(
        check(text),
        [
            "6:17: string should not be empty [empty-string]",
            r#"7:5: name is missing in "environment" section [missing-required-key]"#,
            r#"9:5: group name is missing in "concurrency" section [missing-required-key]"#,
            r#"9:18: "concurrency" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
            r#"10:9: expecting a single ${{...}} expression or mapping value for "env" section, but found plain text node [expected-single-expression]"#,
            r#"11:15: "defaults" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
            r#"11:15: "defaults" section should have "run" section [missing-required-key]"#,
        ]
    );
}

#[test]
fn a_job_without_uses_needs_runs_on_and_steps() {
    let text = "\
on: push
jobs:
  none:
    name: x
  no-steps:
    runs-on: x
  no-runs-on:
    steps: [{run: a}]
";
    assert_eq!(
        check(text),
        [
            r#"3:3: "runs-on" section is missing in job "none" [missing-required-key]"#,
            r#"3:3: "steps" section is missing in job "none" [missing-required-key]"#,
            r#"5:3: "steps" section is missing in job "no-steps" [missing-required-key]"#,
            r#"7:3: "runs-on" section is missing in job "no-runs-on" [missing-required-key]"#,
        ]
    );
}

#[test]
fn an_invalid_runs_on_counts_as_written() {
    let text = "\
on: push
jobs:
  test:
    runs-on: {}
    steps: [{run: a}]
";
    assert_eq!(
        check(text),
        [
            r#"4:14: "runs-on" section should not be empty. please remove this section if it's unnecessary [empty-section]"#
        ]
    );
}

#[test]
fn a_reusable_workflow_call_has_uses_with_and_secrets() {
    let text = "\
on: push
jobs:
  call:
    name: Call
    needs: [build]
    if: always()
    permissions: read-all
    uses: octo-org/repo/.github/workflows/ci.yml@v1
    with:
      Mode: fast
      empty: ''
    secrets:
      TOKEN: ${{ secrets.TOKEN }}
    strategy: {matrix: {a: [b]}}
    concurrency: ci
    snapshot: image
  inherit:
    uses: ./.github/workflows/ci.yml
    secrets: inherit
  build:
    runs-on: x
    steps: [{run: a}]
";
    parsed(text, |workflow| {
        let call = job(workflow, "call");
        let workflow_call = call.workflow_call.as_ref().unwrap();
        assert_eq!(
            workflow_call.uses.value().map(JobUses::value),
            Some("octo-org/repo/.github/workflows/ci.yml@v1")
        );
        assert_eq!(workflow_call.uses.key().map(Str::value), Some("uses"));
        let inputs = workflow_call.inputs.value().unwrap();
        assert_eq!(keyed_strings(inputs), ["Mode=fast", "empty="]);
        assert!(inputs.contains_key("mode"));
        assert_eq!(workflow_call.inputs.key().map(Str::value), Some("with"));
        let Some(WorkflowCallSecrets::Map(secrets)) = workflow_call.secrets.value() else {
            panic!("`secrets` is not a valid mapping");
        };
        assert_eq!(keyed_strings(secrets), ["TOKEN=${{ secrets.TOKEN }}"]);
        assert!(!call.runs_on.is_present());

        let inherit = job(workflow, "inherit").workflow_call.as_ref().unwrap();
        assert!(matches!(
            inherit.secrets.value(),
            Some(WorkflowCallSecrets::Inherit(secret)) if secret.value() == "inherit"
        ));
        assert!(!inherit.inputs.is_present());
        assert!(matches!(
            inherit.uses.value().map(|uses| uses.kind),
            Some(JobUsesKind::Local { path }) if path.value == "./.github/workflows/ci.yml"
        ));
        assert!(matches!(
            workflow_call.uses.value().map(|uses| uses.kind),
            Some(JobUsesKind::Repository { owner, repo, path, git_ref })
                if [owner.value, repo.value, path.value, git_ref.value]
                    == ["octo-org", "repo", ".github/workflows/ci.yml", "v1"]
        ));

        assert!(job(workflow, "build").workflow_call.is_none());
    });
}

#[test]
fn quoted_inherit_is_inherit() {
    parsed(
        "on: push\njobs:\n  call:\n    uses: ./w.yml\n    secrets: 'inherit'\n",
        |workflow| {
            let call = job(workflow, "call").workflow_call.as_ref().unwrap();
            assert!(matches!(
                call.secrets.value(),
                Some(WorkflowCallSecrets::Inherit(_))
            ));
        },
    );
}

#[test]
fn invalid_uses_with_and_secrets_are_reported() {
    let text = "\
on: push
jobs:
  a:
    uses: ''
    with:
    secrets:
  b:
    uses: [x]
    with: [x]
    secrets: Inherit
  c:
    uses: ./w.yml
    with: {a: [x], A: c}
    secrets: {a: [x], A: c}
  d:
    uses: ./w.yml
    with: {}
    secrets: {}
  e:
    uses: ./w.yml
    secrets: [x]
";
    assert_eq!(
        check(text),
        [
            "4:11: string should not be empty [empty-string]",
            r#"5:10: "with" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
            r#"6:13: expected mapping node for secrets or "inherit" string node but found "" node [unexpected-node-kind]"#,
            r#"8:11: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            r#"9:11: "with" section is sequence node but mapping node is expected [unexpected-node-kind]"#,
            r#"10:14: expected mapping node for secrets or "inherit" string node but found "Inherit" node [unexpected-node-kind]"#,
            r#"13:15: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            r#"13:20: key "A" is duplicated in "with" section. previously defined at line:13,col:12. note that this key is case insensitive [duplicate-key]"#,
            r#"14:18: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            r#"14:23: key "A" is duplicated in "secrets" section. previously defined at line:14,col:15. note that this key is case insensitive [duplicate-key]"#,
            r#"17:11: "with" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
            r#"18:14: "secrets" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
            r#"21:14: "secrets" section is sequence node but mapping node is expected [unexpected-node-kind]"#,
        ]
    );
}

#[test]
fn an_invalid_uses_still_makes_the_job_a_call() {
    parsed_with_errors("on: push\njobs:\n  call:\n    uses: ''\n", |workflow| {
        let call = job(workflow, "call").workflow_call.as_ref().unwrap();
        assert!(matches!(call.uses, Field::Invalid { .. }));
    });
}

/// Parses the workflow `text` and runs `f` on its workflow, whatever its
/// diagnostics.
fn parsed_with_errors(text: &str, f: impl FnOnce(&Workflow<'_>)) {
    let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    f(&parse(&file, &document).workflow);
}

#[test]
fn a_call_with_a_key_of_a_job_with_steps_is_reported_and_is_no_call() {
    let text = "\
on: push
jobs:
  runs-on:
    uses: ./w.yml
    runs-on: x
  steps:
    uses:
    steps: [{run: a}]
  container:
    uses: ./w.yml
    name: n
    container: c
";
    assert_eq!(
        check(text),
        [
            format!(
                r#"5:5: when a reusable workflow is called with "uses", "runs-on" is not available. {ALLOWED_CALL_KEYS} in job "runs-on" [invalid-reusable-call-key]"#
            ),
            "7:10: string should not be empty [empty-string]".to_owned(),
            format!(
                r#"8:5: when a reusable workflow is called with "uses", "steps" is not available. {ALLOWED_CALL_KEYS} in job "steps" [invalid-reusable-call-key]"#
            ),
            format!(
                r#"12:5: when a reusable workflow is called with "uses", "container" is not available. {ALLOWED_CALL_KEYS} in job "container" [invalid-reusable-call-key]"#
            ),
        ]
    );
    parsed_with_errors(text, |workflow| {
        for entry in jobs(workflow) {
            assert!(entry.value.workflow_call.is_none(), "{}", entry.key.value());
        }
    });
}

#[test]
fn a_call_with_services_is_reported_and_keeps_its_call() {
    let text = "\
on: push
jobs:
  services:
    uses: ./w.yml
    services: {redis: {image: redis}}
    with: {a: b}
  both:
    uses: ./w.yml
    services: {redis: {image: redis}}
    runs-on: x
";
    assert_eq!(
        check(text),
        [
            format!(
                r#"5:5: when a reusable workflow is called with "uses", "services" is not available. {ALLOWED_CALL_KEYS} in job "services" [invalid-reusable-call-key]"#
            ),
            format!(
                r#"9:5: when a reusable workflow is called with "uses", "services" is not available. {ALLOWED_CALL_KEYS} in job "both" [invalid-reusable-call-key]"#
            ),
            format!(
                r#"10:5: when a reusable workflow is called with "uses", "runs-on" is not available. {ALLOWED_CALL_KEYS} in job "both" [invalid-reusable-call-key]"#
            ),
        ]
    );
    parsed_with_errors(text, |workflow| {
        let call = job(workflow, "services").workflow_call.as_ref().unwrap();
        assert_eq!(keyed_strings(call.inputs.value().unwrap()), ["a=b"]);
        assert!(job(workflow, "services").services.literal().is_some());
        assert!(job(workflow, "both").workflow_call.is_none());
    });
}

#[test]
fn every_key_of_a_job_with_steps_is_reported_in_a_call() {
    let text = "\
on: push
jobs:
  test:
    uses: ./w.yml
    environment: e
    name: n
    runs-on: x
";
    // The reference reports only the last key.
    assert_eq!(
        check(text),
        [
            format!(
                r#"5:5: when a reusable workflow is called with "uses", "environment" is not available. {ALLOWED_CALL_KEYS} in job "test" [invalid-reusable-call-key]"#
            ),
            format!(
                r#"7:5: when a reusable workflow is called with "uses", "runs-on" is not available. {ALLOWED_CALL_KEYS} in job "test" [invalid-reusable-call-key]"#
            ),
        ]
    );
}

#[test]
fn each_key_of_a_job_with_steps_is_not_available_in_a_call() {
    for key in [
        "runs-on: x",
        "environment: e",
        "outputs: {a: b}",
        "env: {a: b}",
        "defaults: {run: {shell: bash}}",
        "steps: [{run: a}]",
        "timeout-minutes: 1",
        "continue-on-error: true",
        "container: c",
        "services: {redis: {image: redis}}",
    ] {
        let text = format!("on: push\njobs:\n  test:\n    uses: ./w.yml\n    {key}\n");
        let name = key.split(':').next().unwrap();
        assert_eq!(
            check(&text),
            [format!(
                r#"5:5: when a reusable workflow is called with "uses", "{name}" is not available. {ALLOWED_CALL_KEYS} in job "test" [invalid-reusable-call-key]"#
            )],
            "{key}"
        );
    }
}

#[test]
fn with_and_secrets_without_uses_are_each_reported() {
    let text = "\
on: push
jobs:
  with:
    with: {a: b}
    runs-on: x
    steps: [{run: a}]
  both:
    with: {a: b}
    secrets: inherit
";
    // The reference reports only the last key.
    assert_eq!(
        check(text),
        [
            r#"4:5: "with" is only available for a reusable workflow call with "uses" but "uses" is not found in job "with" [reusable-call-key-without-uses]"#,
            r#"7:3: "runs-on" section is missing in job "both" [missing-required-key]"#,
            r#"7:3: "steps" section is missing in job "both" [missing-required-key]"#,
            r#"8:5: "with" is only available for a reusable workflow call with "uses" but "uses" is not found in job "both" [reusable-call-key-without-uses]"#,
            r#"9:5: "secrets" is only available for a reusable workflow call with "uses" but "uses" is not found in job "both" [reusable-call-key-without-uses]"#,
        ]
    );
}

#[test]
fn snapshot_takes_an_image_name_or_a_mapping() {
    let text = "\
on: push
jobs:
  string:
    runs-on: x
    steps: [{run: a}]
    snapshot: my-image
  mapping:
    runs-on: x
    steps: [{run: a}]
    snapshot:
      image-name: my-image
      version: 2.*
      if: ${{ github.ref == 'refs/heads/main' }}
";
    parsed(text, |workflow| {
        let string = &job(workflow, "string").snapshot;
        assert_eq!(string.key().map(Str::value), Some("snapshot"));
        let snapshot = string.value().unwrap();
        assert_eq!(
            snapshot.image_name.value().map(Str::value),
            Some("my-image")
        );
        assert_eq!(snapshot.image_name.key(), None);

        let mapping = job(workflow, "mapping").snapshot.value().unwrap();
        assert_eq!(mapping.image_name.key().map(Str::value), Some("image-name"));
        assert_eq!(mapping.version.value().map(Str::value), Some("2.*"));
        assert_eq!(
            mapping.condition.value().map(Template::value),
            Some("${{ github.ref == 'refs/heads/main' }}")
        );
    });
}

#[test]
fn invalid_snapshot_is_reported_in_the_snapshot_section() {
    let text = "\
on: push
jobs:
  a:
    runs-on: x
    steps: [{run: a}]
    snapshot: ''
  b:
    runs-on: x
    steps: [{run: a}]
    snapshot:
  c:
    runs-on: x
    steps: [{run: a}]
    snapshot: {}
  d:
    runs-on: x
    steps: [{run: a}]
    snapshot: []
  e:
    runs-on: x
    steps: [{run: a}]
    snapshot: {version: x}
  f:
    runs-on: x
    steps: [{run: a}]
    snapshot: {image-name: [x], if: ''}
  g:
    runs-on: x
    steps: [{run: a}]
    snapshot: {Image-name: x}
";
    assert_eq!(
        check(text),
        [
            "6:15: string should not be empty [empty-string]",
            "10:14: string should not be empty [empty-string]",
            r#"14:5: "snapshot" section must have "image-name" configuration [missing-required-key]"#,
            r#"14:15: "snapshot" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
            r#"18:15: "snapshot" section value must be string or mapping but found sequence node [unexpected-node-kind]"#,
            r#"22:5: "snapshot" section must have "image-name" configuration [missing-required-key]"#,
            r#"26:28: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            "26:37: string should not be empty [empty-string]",
            r#"30:5: "snapshot" section must have "image-name" configuration [missing-required-key]"#,
            r#"30:16: unexpected key "Image-name" for "snapshot" section. expected one of "if", "image-name", "version" [unexpected-key]"#,
        ]
    );
}

#[test]
fn a_snapshot_that_is_not_a_mapping_or_string_is_invalid() {
    parsed_with_errors(
        "on: push\njobs:\n  test:\n    runs-on: x\n    steps: [{run: a}]\n    snapshot: []\n",
        |workflow| {
            assert!(matches!(
                *job(workflow, "test").snapshot,
                Field::Invalid { .. }
            ));
        },
    );
}

#[test]
fn an_absent_key_is_absent() {
    parsed(
        "on: push\njobs:\n  test:\n    runs-on: x\n    steps: [{run: a}]\n",
        |workflow| {
            let test_job = job(workflow, "test");
            assert!(!test_job.name.is_present());
            assert!(!test_job.needs.is_present());
            assert!(!test_job.outputs.is_present());
            assert!(matches!(*test_job.env, Value::Absent));
            assert!(!test_job.timeout_minutes.is_present());
            assert!(!test_job.snapshot.is_present());
            assert!(test_job.workflow_call.is_none());
        },
    );
}

#[test]
fn job_sections_keep_an_insert_expression_and_need_no_required_key() {
    let text = "\
on: push
concurrency:
  ${{ insert }}: ${{ fromJSON(vars.A) }}
jobs:
  test:
    runs-on:
      ${{ insert }}: ${{ fromJSON(vars.B) }}
    strategy:
      ${{ insert }}: ${{ fromJSON(vars.C) }}
    concurrency:
      ${{ insert }}: ${{ fromJSON(vars.D) }}
    environment:
      ${{ insert }}: ${{ fromJSON(vars.E) }}
    defaults:
      run:
        ${{ insert }}: ${{ fromJSON(vars.F) }}
    steps: [{run: a}]
";
    parsed(text, |workflow| {
        let inserts = |exprs: &[crate::Expr<'_>]| -> Vec<String> {
            exprs.iter().map(|e| e.source.value().to_owned()).collect()
        };
        let concurrency = workflow.concurrency.value().unwrap();
        assert_eq!(inserts(&concurrency.inserts), ["${{ fromJSON(vars.A) }}"]);
        let body = job(workflow, "test");
        let runner = body.runs_on.value().unwrap();
        assert!(!runner.labels.is_present());
        assert_eq!(inserts(&runner.inserts), ["${{ fromJSON(vars.B) }}"]);
        let strategy = body.strategy.value().unwrap();
        assert_eq!(inserts(&strategy.inserts), ["${{ fromJSON(vars.C) }}"]);
        let concurrency = body.concurrency.value().unwrap();
        assert_eq!(inserts(&concurrency.inserts), ["${{ fromJSON(vars.D) }}"]);
        let environment = body.environment.value().unwrap();
        assert_eq!(inserts(&environment.inserts), ["${{ fromJSON(vars.E) }}"]);
        let run = body.defaults.value().unwrap().run.value().unwrap();
        assert_eq!(inserts(&run.inserts), ["${{ fromJSON(vars.F) }}"]);
    });
}

#[test]
fn the_entries_of_an_inserted_mapping_join_the_job_sections() {
    let text = "\
on: push
jobs:
  test:
    runs-on:
      ${{ insert }}: {labels: [linux, x64]}
      group: g
    strategy:
      ${{ insert }}: {fail-fast: false}
    concurrency:
      ${{ insert }}: {group: ci}
    environment:
      ${{ insert }}: {name: production}
      url: https://example.com
    defaults:
      run:
        ${{ insert }}: {shell: bash}
    steps: [{run: a}]
";
    parsed(text, |workflow| {
        let body = job(workflow, "test");
        let runner = body.runs_on.value().unwrap();
        let labels: Vec<_> = runner.labels.literal().unwrap().iter().flatten().collect();
        assert_eq!(
            labels.iter().map(|l| l.value()).collect::<Vec<_>>(),
            ["linux", "x64"]
        );
        assert_eq!(runner.group.value().map(Template::value), Some("g"));
        assert_eq!(
            body.strategy.value().unwrap().fail_fast.literal(),
            Some(&false)
        );
        let group = &body.concurrency.value().unwrap().group;
        assert_eq!(group.value().map(Template::value), Some("ci"));
        let environment = body.environment.value().unwrap();
        assert_eq!(
            environment.name.value().map(Template::value),
            Some("production")
        );
        let run = body.defaults.value().unwrap().run.value().unwrap();
        assert_eq!(run.shell.value().map(Template::value), Some("bash"));
    });
}

#[test]
fn an_insert_key_in_the_workflow_defaults_run_or_the_job_defaults_is_unexpected() {
    let text = "\
on: push
defaults:
  run:
    ${{ insert }}: ${{ fromJSON(vars.A) }}
jobs:
  test:
    runs-on: x
    defaults:
      ${{ insert }}: ${{ fromJSON(vars.B) }}
    steps: [{run: a}]
";
    assert_eq!(
        check(text),
        [
            r#"4:5: unexpected key "${{ insert }}" for "run" section. expected one of "shell", "working-directory" [unexpected-key]"#,
            r#"9:7: "defaults" section should have "run" section [missing-required-key]"#,
            r#"9:7: expected "run" key for "defaults" section but got "${{ insert }}" [unexpected-key]"#,
        ]
    );
}
