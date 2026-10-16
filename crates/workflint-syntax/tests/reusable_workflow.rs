//! Decoding of the `on.workflow_call` section of local reusable workflows.
//!
//! The expected texts come from the reference: actionlint at `011a6d1`,
//! decoding with go-yaml v4 as `LocalReusableWorkflowCache.FindMetadata`
//! does. A metadata is printed as [`dump`] prints it, and an error as the
//! text that follows `error while parsing reusable workflow "<spec>": ` in
//! the reference's message.

#![allow(clippy::unwrap_used)]

use std::fmt::Write as _;

use workflint_diagnostics::quote::quote;
use workflint_diagnostics::{FileId, SourceFile};
use workflint_syntax::{ReusableWorkflowMetadata, WorkflowCallInputType};

/// Decodes `text` and prints the metadata or `error: <message>`.
fn decode(text: &str) -> String {
    let file = SourceFile::new(FileId::new(0), "reusable.yml", text).unwrap();
    match ReusableWorkflowMetadata::decode(&file) {
        Ok(metadata) => dump(&metadata),
        Err(err) => format!("error: {}\n", err.message()),
    }
}

/// Prints the inputs, outputs, and secrets of `metadata` in key order. The
/// input types are named as the reference's expression types print.
fn dump(metadata: &ReusableWorkflowMetadata) -> String {
    let mut out = String::new();
    for (id, input) in &metadata.inputs {
        let input_type = match input.input_type {
            None => "any",
            Some(WorkflowCallInputType::Boolean) => "bool",
            Some(WorkflowCallInputType::Number) => "number",
            Some(WorkflowCallInputType::String) => "string",
        };
        let _ = writeln!(
            out,
            "input {}: name={} required={} type={input_type}",
            quote(id),
            quote(&input.name),
            input.required,
        );
    }
    for (id, output) in &metadata.outputs {
        let _ = writeln!(out, "output {}: name={}", quote(id), quote(&output.name));
    }
    for (id, secret) in &metadata.secrets {
        let _ = writeln!(
            out,
            "secret {}: name={} required={}",
            quote(id),
            quote(&secret.name),
            secret.required,
        );
    }
    out
}

macro_rules! fixture {
    ($path:literal) => {
        include_str!(concat!(
            "../../../tests/conformance/actionlint/testdata/",
            $path
        ))
    };
}

// `TestReusableWorkflowUnmarshalOK` in `reusable_workflow_test.go`.

#[test]
fn inputs_outputs_and_secrets_are_decoded() {
    let text = "on:
  workflow_call:
    inputs:
      i:
        type: string
    outputs:
      o:
        value: foo
    secrets:
      x:
";
    assert_eq!(
        decode(text),
        "input \"i\": name=\"i\" required=false type=string\noutput \"o\": name=\"o\"\nsecret \"x\": name=\"x\" required=false\n",
    );
}

#[test]
fn an_empty_workflow_call_has_no_metadata() {
    assert_eq!(decode("on:\n  workflow_call:\n"), "");
    assert_eq!(
        decode("on:\n  workflow_call:\n    inputs:\n    outputs:\n    secrets:\n"),
        "",
    );
}

#[test]
fn input_types_and_required() {
    let text = "on:
  workflow_call:
    inputs:
      a:
        type: string
      b:
        type: number
      c:
        type: boolean
      d:
      e:
        type: string
        required: false
      f:
        type: string
        required: true
      g:
        type: string
        default: abc
      h:
        type: string
        default: abc
        required: true
      i:
        required: true
";
    assert_eq!(
        decode(text),
        r#"input "a": name="a" required=false type=string
input "b": name="b" required=false type=number
input "c": name="c" required=false type=bool
input "d": name="d" required=false type=any
input "e": name="e" required=false type=string
input "f": name="f" required=true type=string
input "g": name="g" required=false type=string
input "h": name="h" required=false type=string
input "i": name="i" required=true type=any
"#,
    );
}

#[test]
fn secrets_may_be_required() {
    let text = "on:
  workflow_call:
    secrets:
      x:
      y:
        required: false
      z:
        required: true
";
    assert_eq!(
        decode(text),
        "secret \"x\": name=\"x\" required=false\nsecret \"y\": name=\"y\" required=false\nsecret \"z\": name=\"z\" required=true\n",
    );
}

#[test]
fn workflow_call_as_a_scalar_or_in_a_sequence_has_no_metadata() {
    assert_eq!(decode("on: workflow_call"), "");
    assert_eq!(decode("on: [workflow_call]"), "");
    assert_eq!(decode("on: [pull_request, workflow_call, push]"), "");
}

#[test]
fn ids_are_lowercase_and_names_keep_their_case() {
    let text = "on:
  workflow_call:
    inputs:
      MY_INPUT1:
        type: string
      MY_INPUT2:
        type: number
    outputs:
      MY_OUTPUT1:
        value: foo
      MY_OUTPUT2:
        value: foo
    secrets:
      MY_SECRET1:
      MY_SECRET2:
        required: true
";
    assert_eq!(
        decode(text),
        r#"input "my_input1": name="MY_INPUT1" required=false type=string
input "my_input2": name="MY_INPUT2" required=false type=number
output "my_output1": name="MY_OUTPUT1"
output "my_output2": name="MY_OUTPUT2"
secret "my_secret1": name="MY_SECRET1" required=false
secret "my_secret2": name="MY_SECRET2" required=true
"#,
    );
}

// `TestReusableWorkflowUnmarshalOnNodeNotFound`.

#[test]
fn a_file_without_on_is_an_error() {
    assert_eq!(decode("hello: world"), "error: \"on:\" is not found\n");
    assert_eq!(decode(""), "error: \"on:\" is not found\n");
    // The key is case-sensitive.
    assert_eq!(
        decode("On: workflow_call\n"),
        "error: \"on:\" is not found\n"
    );
}

// `TestReusableWorkflowUnmarshalEventNotFound`.

#[test]
fn a_workflow_without_workflow_call_is_an_error() {
    for (text, position) in [
        ("on: push", "line:1, column:5"),
        ("on: [push, pull_request]", "line:1, column:5"),
        ("on: []", "line:1, column:5"),
        ("on:\n  push:\n  pull_request:\n", "line:2, column:3"),
        ("on: {}", "line:1, column:5"),
        ("on:", "line:1, column:4"),
    ] {
        assert_eq!(
            decode(text),
            format!(
                "error: \"workflow_call\" event trigger is not found in \"on:\" at {position}\n"
            ),
            "{text}",
        );
    }
}

// `TestReusableWorkflowCacheFindMetadataOK` and
// `TestReusableWorkflowCacheFindMetadataError`.

#[test]
fn fixture_ok() {
    assert_eq!(
        decode(fixture!("reusable_workflow_metadata/ok.yaml")),
        r#"input "input1": name="input1" required=false type=string
input "input2": name="input2" required=true type=bool
output "output1": name="output1"
secret "secret1": name="secret1" required=false
secret "secret2": name="secret2" required=true
"#,
    );
}

#[test]
fn fixture_errors() {
    assert_eq!(
        decode(fixture!("reusable_workflow_metadata/broken.yaml")),
        "error: yaml: unmarshal errors:   line 3: yaml: on.workflow_call.outputs must be mapping node but scalar node was found at line:3, col:14\n",
    );
    assert_eq!(
        decode(fixture!("reusable_workflow_metadata/no_hook.yaml")),
        "error: \"workflow_call\" event trigger is not found in \"on:\" at line:1, column:5\n",
    );
    assert_eq!(
        decode(fixture!("reusable_workflow_metadata/no_on.yaml")),
        "error: \"on:\" is not found\n",
    );
    assert_eq!(
        decode(fixture!("reusable_workflow_metadata/broken_inputs.yaml")),
        "error: yaml: unmarshal errors:   line 4: cannot unmarshal !!str `validat...` into actionlint.metadata\n",
    );
    assert_eq!(
        decode(fixture!("reusable_workflow_metadata/broken_secrets.yaml")),
        "error: yaml: unmarshal errors:   line 4: cannot unmarshal !!str `my secret` into actionlint.ReusableWorkflowMetadataSecret\n",
    );
}

// The fixture of the reference's project with broken reusable workflows.
// `broken_reusable_workflow.out` has:
//
// - `error while parsing reusable workflow "\./reusable/broken\.yaml": yaml: .+`
// - `error while parsing reusable workflow "./reusable/no_hook.yaml":
//   "workflow_call" event trigger is not found in "on:" at line:1, column:5`
// - `error while parsing reusable workflow "./reusable/no_on.yaml": "on:" is
//   not found`
// - `error while parsing reusable workflow
//   "\./reusable/broken_secrets\.yaml": yaml: .+`
//
// `broken_input.yaml` has an input with a null value, which decodes.
#[test]
fn fixture_broken_reusable_workflow() {
    assert_eq!(
        decode(fixture!(
            "projects/broken_reusable_workflow/reusable/broken.yaml"
        )),
        "error: yaml: unmarshal errors:   line 3: yaml: on.workflow_call.outputs must be mapping node but scalar node was found at line:3, col:14\n",
    );
    assert_eq!(
        decode(fixture!(
            "projects/broken_reusable_workflow/reusable/no_hook.yaml"
        )),
        "error: \"workflow_call\" event trigger is not found in \"on:\" at line:1, column:5\n",
    );
    assert_eq!(
        decode(fixture!(
            "projects/broken_reusable_workflow/reusable/no_on.yaml"
        )),
        "error: \"on:\" is not found\n",
    );
    assert_eq!(
        decode(fixture!(
            "projects/broken_reusable_workflow/reusable/broken_secrets.yaml"
        )),
        "error: yaml: unmarshal errors:   line 3: yaml: on.workflow_call.secrets must be mapping node but scalar node was found at line:3, col:14\n",
    );
    assert_eq!(
        decode(fixture!(
            "projects/broken_reusable_workflow/reusable/broken_input.yaml"
        )),
        "input \"empty_input\": name=\"empty_input\" required=false type=any\n",
    );
}

// Typed decoding rules.

#[test]
fn an_input_must_be_a_mapping() {
    assert_eq!(
        decode("on:\n  workflow_call:\n    inputs:\n      a: foo\n"),
        "error: yaml: unmarshal errors:   line 4: cannot unmarshal !!str `foo` into actionlint.metadata\n",
    );
    assert_eq!(
        decode("on:\n  workflow_call: {inputs: [a]}\n"),
        "error: yaml: unmarshal errors:   line 2: yaml: on.workflow_call.inputs must be mapping node but sequence node was found at line:2, col:27\n",
    );
}

#[test]
fn required_accepts_yaml_1_1_booleans_and_rejects_a_quoted_true() {
    assert_eq!(
        decode(
            "on:\n  workflow_call:\n    inputs:\n      a: {required: on}\n    secrets:\n      b: {required: yes}\n"
        ),
        "input \"a\": name=\"a\" required=true type=any\nsecret \"b\": name=\"b\" required=true\n",
    );
    assert_eq!(
        decode("on:\n  workflow_call:\n    inputs:\n      a:\n        required: \"true\"\n"),
        "error: yaml: unmarshal errors:   line 5: cannot unmarshal !!str `true` into bool\n",
    );
}

#[test]
fn later_inputs_replace_earlier_ones_with_the_same_id() {
    let text = "on:
  workflow_call:
    inputs:
      a:
        type: number
        required: true
        default: 1
      A:
        type: Boolean
    outputs:
      o: [weird]
      O:
";
    assert_eq!(
        decode(text),
        "input \"a\": name=\"A\" required=false type=any\noutput \"o\": name=\"O\"\n",
    );
}

#[test]
fn exact_duplicate_keys_are_rejected() {
    assert_eq!(
        decode("jobs: 1\njobs: 2\non: workflow_call\n"),
        "error: yaml: unmarshal errors:   line 2: mapping key \"jobs\" already defined at line 1\n",
    );
    assert_eq!(
        decode("on: {}\non: push\n"),
        "error: yaml: unmarshal errors:   line 2: mapping key \"on\" already defined at line 1\n",
    );
    assert_eq!(
        decode("on:\n  workflow_call:\n    inputs:\n      a: {type: \"a\\nb\", type: c}\n"),
        "error: yaml: unmarshal errors:   line 4: mapping key \"type\" already defined at line 4\n",
    );
}

#[test]
fn the_root_must_be_a_mapping() {
    assert_eq!(
        decode("foo\n"),
        "error: yaml: unmarshal errors:   line 1: cannot unmarshal !!str `foo` into actionlint.workflow\n",
    );
}

#[test]
fn workflow_call_must_be_a_mapping() {
    assert_eq!(
        decode("on:\n  WORKFLOW_CALL: foo\n"),
        "error: yaml: unmarshal errors:   line 2: cannot unmarshal !!str `foo` into actionlint.ReusableWorkflowMetadata\n",
    );
}

#[test]
fn on_may_be_quoted_or_merged() {
    assert_eq!(decode("\"on\": workflow_call\n"), "");
    assert_eq!(decode("<<: {on: workflow_call}\n"), "");
}

// The reference does not resolve an alias as `on:` or as an item of it.
#[test]
fn an_alias_as_on_is_not_workflow_call() {
    assert_eq!(
        decode("x: &x workflow_call\non: *x\n"),
        "error: \"workflow_call\" event trigger is not found in \"on:\" at line:2, column:5\n",
    );
    assert_eq!(
        decode("x: &x workflow_call\non: [*x]\n"),
        "error: \"workflow_call\" event trigger is not found in \"on:\" at line:2, column:5\n",
    );
}

#[test]
fn an_alias_inside_workflow_call_is_resolved() {
    let text =
        "on:\n  workflow_call:\n    inputs: &i\n      a: {type: string}\n  push:\n    inputs: *i\n";
    assert_eq!(
        decode(text),
        "input \"a\": name=\"a\" required=false type=string\n"
    );
}

#[test]
fn an_explicit_tag_must_match_the_value() {
    assert_eq!(
        decode("on:\n  workflow_call:\n    inputs:\n      a: {type: !!int x}\n"),
        "error: yaml: unmarshal errors:   line 4: yaml: cannot decode !!str `x` as a !!int\n",
    );
}

#[test]
fn syntax_errors_give_go_yaml_texts() {
    assert_eq!(
        decode("on: [push, workflow_call\n"),
        "error: yaml: line 1: did not find expected ',' or ']'\n",
    );
    assert_eq!(
        decode("on: *x\n"),
        "error: yaml: unknown anchor 'x' referenced\n"
    );
}
