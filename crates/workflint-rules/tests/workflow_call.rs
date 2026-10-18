//! The `workflow-call` rule: the format of a job's `uses:`, and the inputs and secrets of calls
//! to local reusable workflows.
//!
//! The tests named after the reference come from `rule_workflow_call_test.go` at `011a6d1`.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fmt::Write as _;

use workflint_diagnostics::{Color, FileId, SourceFile, render_oneline, sort_and_dedup};
use workflint_rules::local::{LocalMetadata, Lookup, MemoryProject, NoProject};

/// Parses `text` as the workflow `test.yaml`, runs the rules over it in `project`, and returns
/// the diagnostics of the parser and the rules in the one-line format.
fn lint_in(project: &dyn LocalMetadata, text: &str) -> Vec<String> {
    let file = SourceFile::new(FileId::new(0), "test.yaml", text).unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    let parsed = workflint_syntax::parse(&file, &document);
    let mut diagnostics = parsed.diagnostics.clone();
    diagnostics.extend(workflint_rules::run_in_project(
        &file,
        &parsed.workflow,
        project,
    ));
    sort_and_dedup(&file, &mut diagnostics);
    let mut out = Vec::new();
    render_oneline(&mut out, &file, &diagnostics, Color::Off).unwrap();
    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Lints a workflow whose only job calls `uses`, outside any project.
fn lint_uses(uses: &str) -> Vec<String> {
    lint_in(
        &NoProject,
        &format!("on: push\njobs:\n  call:\n    uses: {uses:?}\n"),
    )
}

const NONE: [&str; 0] = [];

/// The message of `invalid-reusable-workflow-ref` for `uses` at line 4.
fn invalid_ref(uses: &str) -> String {
    format!(
        "test.yaml:4:11: reusable workflow call {uses:?} at \"uses\" is not following the format \"owner/repo/path/to/workflow.yml@ref\" nor \"./path/to/workflow.yml\". see https://docs.github.com/en/actions/learn-github-actions/reusing-workflows for more details [invalid-reusable-workflow-ref]"
    )
}

// `TestRuleWorkflowCallCheckWorkflowCallUsesFormat`.

#[test]
fn local_and_repository_workflows_and_expressions_are_valid_references() {
    for uses in [
        "owner/repo/x.yml@ref",
        "owner/repo/x.yml@@",
        "owner/repo/x.yml@release/v1",
        "./path/to/x.yml",
    ] {
        assert_eq!(lint_uses(uses), NONE, "{uses}");
    }
    // No context is available in a job's `uses`, so the expression rule reports the expression.
    let expression = lint_uses("${{ env.FOO }}");
    assert!(
        expression
            .iter()
            .all(|line| line.ends_with("[context-not-allowed]")),
        "{expression:?}"
    );
}

#[test]
fn references_in_neither_format_are_reported() {
    for uses in [
        "./path/to/x.yml@ref",
        "/path/to/x.yml@ref",
        "./",
        ".",
        "owner/x.yml@ref",
        "owner/repo@ref",
        "owner/repo/x.yml",
        "/repo/x.yml@ref",
        "owner//x.yml@ref",
        "owner/repo/@ref",
        "owner/repo/x.yml@",
    ] {
        assert_eq!(lint_uses(uses), [invalid_ref(uses)], "{uses}");
    }
}

// `TestRuleWorkflowCallNestedWorkflowCalls`.

#[test]
fn a_reusable_workflow_may_call_a_repository_workflow() {
    let text = "on: workflow_call\njobs:\n  call:\n    uses: o/r/w.yaml@r\n";
    let project = MemoryProject::new().linting("test.yaml");
    assert_eq!(lint_in(&project, text), NONE);
}

// `TestRuleWorkflowCallWriteEventNodeToMetadataCache`.

#[test]
fn a_linted_reusable_workflow_is_registered_under_its_path() {
    let text = "on:
  workflow_call:
    inputs:
      input1:
        type: string
    outputs:
      output1:
        value: x
    secrets:
      secret1:
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo
";
    let project = MemoryProject::new().linting(".github/workflows/test-workflow.yaml");
    assert_eq!(lint_in(&project, text), NONE);
    let Lookup::Found { metadata, first } =
        project.reusable_workflow("./.github/workflows/test-workflow.yaml")
    else {
        panic!("the linted workflow is registered");
    };
    assert!(!first);
    assert_eq!(metadata.inputs["input1"].name, "input1");
    assert!(!metadata.inputs["input1"].required);
    assert_eq!(metadata.outputs["output1"].name, "output1");
    assert_eq!(metadata.secrets["secret1"].name, "secret1");
}

#[test]
fn a_reusable_registered_from_a_linted_workflow_is_used_without_reading_its_file() {
    // The project has no file for the reusable, so reading it would report `could not read`.
    let project = MemoryProject::new().linting(".github/workflows/reusable.yaml");
    let reusable = "on:
  workflow_call:
    inputs:
      name:
        type: string
        required: true
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo
";
    assert_eq!(lint_in(&project, reusable), NONE);
    let workflow = "on: push
jobs:
  call:
    uses: ./.github/workflows/reusable.yaml
";
    assert_eq!(
        lint_in(&project, workflow),
        [
            "test.yaml:4:11: input \"name\" is required by \"./.github/workflows/reusable.yaml\" reusable workflow [missing-required-workflow-input]"
        ]
    );
}

// `TestRuleWorkflowCallCheckReusableWorkflowCall`.

/// `workflow0.yaml` of the reference's test: optional and required inputs and secrets.
const WORKFLOW0: &str = "on:
  workflow_call:
    inputs:
      optional_input:
        type: string
      required_input:
        type: string
        required: true
    outputs:
      output:
        value: x
    secrets:
      optional_secret:
      required_secret:
        required: true
";

/// `workflow1.yaml`: the same names in upper case (rhysd/actionlint#216).
const WORKFLOW1: &str = "on:
  workflow_call:
    inputs:
      OPTIONAL_INPUT:
        type: string
      REQUIRED_INPUT:
        type: string
        required: true
    outputs:
      OUTPUT:
        value: x
    secrets:
      OPTIONAL_SECRET:
      REQUIRED_SECRET:
        required: true
";

/// `workflow2.yaml`: no inputs and no secrets.
const WORKFLOW2: &str = "on:
  workflow_call:
";

/// `ok.yaml` of `testdata/reusable_workflow_metadata`.
const OK: &str = "name: minimal

on:
  workflow_call:
    inputs:
      input1:
        description: input
        type: string
      input2:
        type: boolean
        required: true
    outputs:
      output1:
        description: output
        value: foo
    secrets:
      secret1:
        description: secret
      secret2:
        required: true
";

/// `broken.yaml` of `testdata/reusable_workflow_metadata`.
const BROKEN: &str = "on:
  workflow_call:
    outputs: my cool outputs
";

fn project() -> MemoryProject {
    MemoryProject::new()
        .with_file("workflow0.yaml", WORKFLOW0)
        .with_file("workflow1.yaml", WORKFLOW1)
        .with_file("workflow2.yaml", WORKFLOW2)
        .with_file("ok.yaml", OK)
        .with_file("broken.yaml", BROKEN)
        .linting("this-workflow.yaml")
}

/// Lints a reusable workflow whose job `call` calls `uses` with `with` and `secrets`, each a
/// list of names whose value is `x`. `inherit` sets `secrets: inherit` instead. Returns the
/// messages and rule IDs, without the positions, sorted.
fn call(uses: &str, inputs: &[&str], secrets: &[&str], inherit: bool) -> Vec<String> {
    let mut text = format!("on: workflow_call\njobs:\n  call:\n    uses: {uses}\n");
    if !inputs.is_empty() {
        text.push_str("    with:\n");
        for input in inputs {
            writeln!(text, "      {input}: x").unwrap();
        }
    }
    if inherit {
        text.push_str("    secrets: inherit\n");
    } else if !secrets.is_empty() {
        text.push_str("    secrets:\n");
        for secret in secrets {
            writeln!(text, "      {secret}: x").unwrap();
        }
    }
    let mut messages: Vec<_> = lint_in(&project(), &text)
        .into_iter()
        .map(|line| {
            let mut parts = line.splitn(4, ':');
            parts.nth(3).unwrap().trim_start().to_owned()
        })
        .collect();
    messages.sort();
    messages
}

#[test]
fn a_call_that_sets_every_input_and_secret_is_valid() {
    assert_eq!(
        call(
            "./workflow0.yaml",
            &["optional_input", "required_input"],
            &["optional_secret", "required_secret"],
            false,
        ),
        NONE
    );
}

#[test]
fn a_call_that_sets_only_the_required_inputs_and_secrets_is_valid() {
    assert_eq!(
        call(
            "./workflow0.yaml",
            &["required_input"],
            &["required_secret"],
            false
        ),
        NONE
    );
}

#[test]
fn a_call_to_a_missing_workflow_is_reported_once_without_input_checks() {
    assert_eq!(
        call(
            "./unknown-workflow.yaml",
            &["aaa", "bbb"],
            &["xxx", "yyy"],
            false
        ),
        [
            "could not read reusable workflow file for \"./unknown-workflow.yaml\": open unknown-workflow.yaml: no such file or directory [reusable-workflow-not-found]"
        ]
    );
}

#[test]
fn missing_required_inputs_and_secrets_are_reported_at_uses() {
    assert_eq!(
        call(
            "./workflow0.yaml",
            &["optional_input"],
            &["optional_secret"],
            false
        ),
        [
            "input \"required_input\" is required by \"./workflow0.yaml\" reusable workflow [missing-required-workflow-input]",
            "secret \"required_secret\" is required by \"./workflow0.yaml\" reusable workflow [missing-required-workflow-secret]",
        ]
    );
}

#[test]
fn undefined_inputs_and_secrets_list_the_defined_ones() {
    assert_eq!(
        call(
            "./workflow0.yaml",
            &["required_input", "unknown_input"],
            &["required_secret", "unknown_secret"],
            false,
        ),
        [
            "input \"unknown_input\" is not defined in \"./workflow0.yaml\" reusable workflow. defined inputs are \"optional_input\", \"required_input\" [undefined-workflow-input]",
            "secret \"unknown_secret\" is not defined in \"./workflow0.yaml\" reusable workflow. defined secrets are \"optional_secret\", \"required_secret\" [undefined-workflow-secret]",
        ]
    );
}

#[test]
fn inherited_secrets_are_not_checked() {
    assert_eq!(
        call(
            "./workflow0.yaml",
            &["required_input"],
            &["unknown_secret", "optional_secret"],
            true,
        ),
        NONE
    );
}

#[test]
fn a_workflow_file_is_read_and_checked() {
    assert_eq!(call("./ok.yaml", &["input2"], &["secret2"], false), NONE);
}

#[test]
fn a_broken_workflow_file_is_reported() {
    assert_eq!(
        call("./broken.yaml", &[], &[], false),
        [
            "error while parsing reusable workflow \"./broken.yaml\": yaml: unmarshal errors:   line 3: yaml: on.workflow_call.outputs must be mapping node but scalar node was found at line:3, col:14 [broken-reusable-workflow]"
        ]
    );
}

#[test]
fn repository_workflows_are_not_checked() {
    let uses = "owner/repo/path/to/workflow@main";
    assert_eq!(call(uses, &[], &[], false), NONE);
    assert_eq!(call(uses, &["aaa", "bbb"], &["xxx", "yyy"], false), NONE);
}

#[test]
fn input_and_secret_names_are_case_insensitive() {
    let upper = (
        ["OPTIONAL_INPUT", "REQUIRED_INPUT"],
        ["OPTIONAL_SECRET", "REQUIRED_SECRET"],
    );
    let lower = (
        ["optional_input", "required_input"],
        ["optional_secret", "required_secret"],
    );
    assert_eq!(call("./workflow0.yaml", &upper.0, &upper.1, false), NONE);
    assert_eq!(call("./workflow1.yaml", &lower.0, &lower.1, false), NONE);
    assert_eq!(call("./workflow1.yaml", &upper.0, &upper.1, false), NONE);
}

#[test]
fn undefined_names_keep_their_case_in_the_message() {
    assert_eq!(
        call(
            "./workflow0.yaml",
            &["required_input", "UNKNOWN_INPUT"],
            &["required_secret", "UNKNOWN_SECRET"],
            false,
        ),
        [
            "input \"UNKNOWN_INPUT\" is not defined in \"./workflow0.yaml\" reusable workflow. defined inputs are \"optional_input\", \"required_input\" [undefined-workflow-input]",
            "secret \"UNKNOWN_SECRET\" is not defined in \"./workflow0.yaml\" reusable workflow. defined secrets are \"optional_secret\", \"required_secret\" [undefined-workflow-secret]",
        ]
    );
}

#[test]
fn a_workflow_without_inputs_and_secrets_says_none_is_defined() {
    assert_eq!(
        call(
            "./workflow2.yaml",
            &["unknown_input"],
            &["unknown_secret"],
            false
        ),
        [
            "input \"unknown_input\" is not defined in \"./workflow2.yaml\" reusable workflow. no input is defined [undefined-workflow-input]",
            "secret \"unknown_secret\" is not defined in \"./workflow2.yaml\" reusable workflow. no secret is defined [undefined-workflow-secret]",
        ]
    );
}

// Cases beyond the reference's tests.

#[test]
fn a_single_defined_input_or_secret_is_named_in_the_singular() {
    assert_eq!(
        call("./ok.yaml", &["input2", "x"], &["secret2", "y"], false),
        [
            "input \"x\" is not defined in \"./ok.yaml\" reusable workflow. defined inputs are \"input1\", \"input2\" [undefined-workflow-input]",
            "secret \"y\" is not defined in \"./ok.yaml\" reusable workflow. defined secrets are \"secret1\", \"secret2\" [undefined-workflow-secret]",
        ]
    );
    let project = MemoryProject::new().with_file(
        "one.yaml",
        "on:\n  workflow_call:\n    inputs:\n      a:\n        type: string\n    secrets:\n      s:\n",
    );
    let text = "on: push\njobs:\n  call:\n    uses: ./one.yaml\n    with:\n      b: x\n    secrets:\n      t: x\n";
    assert_eq!(
        lint_in(&project, text),
        [
            "test.yaml:6:7: input \"b\" is not defined in \"./one.yaml\" reusable workflow. defined input is \"a\" [undefined-workflow-input]",
            "test.yaml:8:7: secret \"t\" is not defined in \"./one.yaml\" reusable workflow. defined secret is \"s\" [undefined-workflow-secret]",
        ]
    );
}

#[test]
fn a_missing_workflow_is_reported_at_its_first_call_only() {
    let project = MemoryProject::new();
    let text = "on: push
jobs:
  a:
    uses: ./missing.yaml
  b:
    uses: ./missing.yaml
";
    assert_eq!(
        lint_in(&project, text),
        [
            "test.yaml:4:11: could not read reusable workflow file for \"./missing.yaml\": open missing.yaml: no such file or directory [reusable-workflow-not-found]"
        ]
    );
}

#[test]
fn an_invalid_local_reference_is_not_read_later() {
    let project = MemoryProject::new();
    let text = "on: push\njobs:\n  call:\n    uses: ./x.yaml@main\n";
    assert_eq!(lint_in(&project, text), [invalid_ref("./x.yaml@main")]);
    assert_eq!(project.reusable_workflow("./x.yaml@main"), Lookup::None);
}

#[test]
fn an_input_of_workflow_dispatch_gets_a_hint() {
    // rhysd/actionlint#336: the callee declares the input for `workflow_dispatch` only.
    let callee = "on:
  workflow_dispatch:
    inputs:
      Environment:
        type: string
  workflow_call:
    inputs:
      region:
        type: string
";
    let project = MemoryProject::new().with_file("deploy.yaml", callee);
    let text =
        "on: push\njobs:\n  call:\n    uses: ./deploy.yaml\n    with:\n      environment: prod\n";
    assert_eq!(
        lint_in(&project, text),
        [
            "test.yaml:6:7: input \"environment\" is not defined in \"./deploy.yaml\" reusable workflow. defined input is \"region\". \"environment\" is defined in \"workflow_dispatch\" inputs, but a reusable workflow call sets only \"workflow_call\" inputs [undefined-workflow-input]"
        ]
    );
}

#[test]
fn a_registered_workflow_keeps_its_workflow_dispatch_inputs_for_the_hint() {
    let project = MemoryProject::new().linting("deploy.yaml");
    let callee = "on:
  workflow_dispatch:
    inputs:
      environment:
        type: string
  workflow_call:
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo
";
    assert_eq!(lint_in(&project, callee), NONE);
    let text =
        "on: push\njobs:\n  call:\n    uses: ./deploy.yaml\n    with:\n      environment: prod\n";
    assert_eq!(
        lint_in(&project, text),
        [
            "test.yaml:6:7: input \"environment\" is not defined in \"./deploy.yaml\" reusable workflow. no input is defined. \"environment\" is defined in \"workflow_dispatch\" inputs, but a reusable workflow call sets only \"workflow_call\" inputs [undefined-workflow-input]"
        ]
    );
}

#[test]
fn a_repeated_with_key_is_checked_once() {
    let project = MemoryProject::new().with_file("workflow2.yaml", WORKFLOW2);
    let text = "on: push
jobs:
  call:
    uses: ./workflow2.yaml
    with:
      a: x
      A: y
";
    assert_eq!(
        lint_in(&project, text),
        [
            "test.yaml:6:7: input \"a\" is not defined in \"./workflow2.yaml\" reusable workflow. no input is defined [undefined-workflow-input]",
            "test.yaml:7:7: key \"A\" is duplicated in \"with\" section. previously defined at line:6,col:7. note that this key is case insensitive [duplicate-key]",
        ]
    );
}
