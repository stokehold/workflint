//! Decoding of local `action.yml` files.
//!
//! The expected texts come from the reference: actionlint at `011a6d1`,
//! decoding with go-yaml v4 as `LocalActionsCache.FindMetadata` does. A
//! metadata is printed as [`dump`] prints it, and an error as the text that
//! follows `could not parse action metadata in "<dir>": ` in the reference's
//! message. Where `docs/differences.md` changes the result, the test says so.

#![allow(clippy::unwrap_used)]

use std::fmt::Write as _;

use workflint_diagnostics::quote::quote;
use workflint_diagnostics::{FileId, SourceFile};
use workflint_syntax::ActionMetadata;

/// Decodes `text` and prints the metadata or `error: <message>`.
fn decode(text: &str) -> String {
    let file = SourceFile::new(FileId::new(0), "action.yml", text).unwrap();
    match ActionMetadata::decode(&file) {
        Ok(metadata) => dump(&metadata),
        Err(err) => format!("error: {}\n", err.message()),
    }
}

/// Prints every field of `metadata`, inputs and outputs in key order.
fn dump(metadata: &ActionMetadata) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "name: {}", quote(&metadata.name));
    let _ = writeln!(out, "description: {}", quote(&metadata.description));
    for (id, input) in &metadata.inputs {
        let _ = writeln!(
            out,
            "input {}: name={} required={} deprecated={} message={}",
            quote(id),
            quote(&input.name),
            input.required,
            input.deprecation_message.is_some(),
            quote(input.deprecation_message.as_deref().unwrap_or_default()),
        );
    }
    for (id, output) in &metadata.outputs {
        let _ = writeln!(out, "output {}: name={}", quote(id), quote(&output.name));
    }
    let runs = &metadata.runs;
    let _ = writeln!(
        out,
        "runs: using={} main={} pre={} pre-if={} post={} post-if={} image={} pre-entrypoint={} entrypoint={} post-entrypoint={} steps={}",
        quote(&runs.using),
        quote(&runs.main),
        quote(&runs.pre),
        quote(&runs.pre_if),
        quote(&runs.post),
        quote(&runs.post_if),
        quote(&runs.image),
        quote(&runs.pre_entrypoint),
        quote(&runs.entrypoint),
        quote(&runs.post_entrypoint),
        runs.steps.is_some(),
    );
    let _ = writeln!(
        out,
        "branding: icon={} color={}",
        quote(&metadata.branding.icon),
        quote(&metadata.branding.color),
    );
    out
}

const NO_RUNS: &str = r#"runs: using="" main="" pre="" pre-if="" post="" post-if="" image="" pre-entrypoint="" entrypoint="" post-entrypoint="" steps=false
branding: icon="" color=""
"#;

const NODE20_RUNS: &str = r#"runs: using="node20" main="index.js" pre="" pre-if="" post="" post-if="" image="" pre-entrypoint="" entrypoint="" post-entrypoint="" steps=false
branding: icon="" color=""
"#;

macro_rules! fixture {
    ($path:literal) => {
        include_str!(concat!(
            "../../../tests/conformance/actionlint/testdata/",
            $path
        ))
    };
}

// `TestLocalActionsFindMetadataOK` in `action_metadata_test.go`.

const MY_ACTION: &str = r#"name: "My action"
description: "my action"
input "addition": name="addition" required=false deprecated=false message=""
input "message": name="message" required=true deprecated=false message=""
input "name": name="name" required=false deprecated=false message=""
output "user_id": name="user_id"
"#;

#[test]
fn action_yml_and_action_yaml_give_the_same_metadata() {
    let want = format!("{MY_ACTION}{NODE20_RUNS}");
    assert_eq!(
        decode(fixture!("action_metadata/action-yml/action.yml")),
        want
    );
    assert_eq!(
        decode(fixture!("action_metadata/action-yaml/action.yaml")),
        want
    );
}

#[test]
fn an_action_without_inputs_and_outputs_has_none() {
    assert_eq!(
        decode(fixture!("action_metadata/empty/action.yml")),
        format!("name: \"My action\"\ndescription: \"my action\"\n{NODE20_RUNS}"),
    );
}

#[test]
fn input_and_output_ids_are_lowercase_and_names_keep_their_case() {
    let want = r#"name: "My action"
description: "my action"
input "addition": name="ADDITION" required=false deprecated=false message=""
input "message": name="MESSAGE" required=true deprecated=false message=""
input "name": name="NAME" required=false deprecated=false message=""
output "user_id": name="USER_ID"
"#;
    assert_eq!(
        decode(fixture!("action_metadata/uppercase/action.yml")),
        format!("{want}{NODE20_RUNS}"),
    );
}

#[test]
fn a_docker_action_has_its_image() {
    assert_eq!(
        decode(fixture!("action_metadata/docker/action.yaml")),
        r#"name: "My action"
description: "my action"
runs: using="docker" main="" pre="" pre-if="" post="" post-if="" image="Dockerfile" pre-entrypoint="" entrypoint="" post-entrypoint="" steps=false
branding: icon="" color=""
"#,
    );
}

#[test]
fn a_composite_action_has_steps() {
    assert_eq!(
        decode(fixture!("action_metadata/composite/action.yaml")),
        r#"name: "My action"
description: "my action"
runs: using="composite" main="" pre="" pre-if="" post="" post-if="" image="" pre-entrypoint="" entrypoint="" post-entrypoint="" steps=true
branding: icon="" color=""
"#,
    );
}

#[test]
fn branding_is_decoded() {
    assert_eq!(
        decode(fixture!("action_metadata/branding/action.yml")),
        MY_ACTION.to_owned()
            + &NODE20_RUNS.replace(r#"icon="" color="""#, r#"icon="edit" color="white""#),
    );
}

#[test]
fn a_node24_action_has_its_runner() {
    assert_eq!(
        decode(fixture!("action_metadata/node24/action.yaml")),
        MY_ACTION.to_owned() + &NODE20_RUNS.replace("node20", "node24"),
    );
}

#[test]
fn deprecation_messages_are_decoded() {
    let want = r#"name: "My action"
description: "my action"
input "addition": name="addition" required=false deprecated=true message="This is deprecated"
input "message": name="message" required=true deprecated=true message="This is deprecated"
input "name": name="name" required=false deprecated=true message="This is deprecated"
output "user_id": name="user_id"
"#;
    assert_eq!(
        decode(fixture!("action_metadata/deprecated/action.yml")),
        format!("{want}{NODE20_RUNS}"),
    );
}

// `TestLocalActionsBrokenMetadata` and
// `TestLocalActionsDuplicateInputsOutputs`.

#[test]
fn a_yaml_syntax_error_gives_go_yaml_text() {
    assert_eq!(
        decode(fixture!("action_metadata/broken/action.yaml")),
        "error: yaml: line 2: mapping values are not allowed in this context\n",
    );
}

#[test]
fn inputs_and_outputs_that_differ_only_in_case_are_duplicates() {
    assert_eq!(
        decode(fixture!("action_metadata/input-duplicate/action.yml")),
        "error: line 6: input \"FOO\" is duplicated\n",
    );
    assert_eq!(
        decode(fixture!("action_metadata/output-duplicate/action.yml")),
        "error: line 6: output \"FOO\" is duplicated\n",
    );
}

// `TestActionMetadataYAMLUnmarshalOK`.

#[test]
fn an_action_with_only_a_name() {
    assert_eq!(
        decode("name: Test"),
        format!("name: \"Test\"\ndescription: \"\"\n{NO_RUNS}"),
    );
}

#[test]
fn required_holds_only_without_a_default() {
    let text = "name: Test
inputs:
  input1:
    description: test
  input2:
    description: test
    required: false
  input3:
    description: test
    required: true
    default: 'default'
  input4:
    description: test
    required: false
    default: 'default'
  input5:
    description: test
    required: true
  input_snake-case:
    description: test
  camelCaseInput:
    description: test
";
    let want = r#"name: "Test"
description: ""
input "camelcaseinput": name="camelCaseInput" required=false deprecated=false message=""
input "input1": name="input1" required=false deprecated=false message=""
input "input2": name="input2" required=false deprecated=false message=""
input "input3": name="input3" required=false deprecated=false message=""
input "input4": name="input4" required=false deprecated=false message=""
input "input5": name="input5" required=true deprecated=false message=""
input "input_snake-case": name="input_snake-case" required=false deprecated=false message=""
"#;
    assert_eq!(decode(text), format!("{want}{NO_RUNS}"));
}

#[test]
fn outputs_keep_only_their_names() {
    let text = "name: Test
outputs:
  output1:
    description: test
  output2:
    description: test
";
    let want = r#"name: "Test"
description: ""
output "output1": name="output1"
output "output2": name="output2"
"#;
    assert_eq!(decode(text), format!("{want}{NO_RUNS}"));
}

#[test]
fn a_deprecation_message_key_marks_an_input_deprecated_even_if_empty() {
    let text = "name: Test
inputs:
  input1:
    description: test
    deprecationMessage: foo
  input2:
    description: test
    deprecationMessage: ' foo bar '
    required: true
  input3:
    description: test
    deprecationMessage:
  input4:
    description: test
    deprecationMessage: ''
";
    let want = r#"name: "Test"
description: ""
input "input1": name="input1" required=false deprecated=true message="foo"
input "input2": name="input2" required=true deprecated=true message="foo bar"
input "input3": name="input3" required=false deprecated=true message=""
input "input4": name="input4" required=false deprecated=true message=""
"#;
    assert_eq!(decode(text), format!("{want}{NO_RUNS}"));
}

// `TestActionMetadataYAMLUnmarshalError`.

#[test]
fn inputs_must_be_a_mapping() {
    assert_eq!(
        decode("name: Test\ninputs: \"foo\""),
        "error: line 2: yaml: inputs must be mapping node but scalar node was found at line:2, col:9\n",
    );
}

#[test]
fn an_input_must_be_a_mapping() {
    assert_eq!(
        decode("name: Test\ninputs:\n  input1: \"foo\""),
        "error: line 3: cannot unmarshal !!str `foo` into actionlint.actionInputMetadata\n",
    );
    assert_eq!(
        decode("inputs:\n  a: [x]\n"),
        "error: line 2: cannot unmarshal !!seq into actionlint.actionInputMetadata\n",
    );
}

#[test]
fn outputs_must_be_a_mapping() {
    assert_eq!(
        decode("name: Test\noutputs: \"foo\""),
        "error: line 2: yaml: outputs must be mapping node but scalar node was found at line:2, col:10\n",
    );
}

#[test]
fn an_unknown_key_in_an_input_is_an_error() {
    assert_eq!(
        decode("name: Test\ninputs:\n  input1:\n    foo: bar"),
        "error: line 3: unexpected key \"foo\" for definition of input \"input1\"\n",
    );
}

#[test]
fn the_last_unknown_key_is_reported() {
    assert_eq!(
        decode("inputs:\n  a:\n    x: 1\n  b:\n    y: 1\n"),
        "error: line 2: unexpected key \"y\" for definition of input \"b\"\n",
    );
}

#[test]
fn a_decoding_error_in_a_later_input_wins_over_an_unknown_key() {
    assert_eq!(
        decode("inputs:\n  a:\n    x: 1\n  b: foo\n"),
        "error: line 4: cannot unmarshal !!str `foo` into actionlint.actionInputMetadata\n",
    );
}

// The fixtures of the reference's projects with local actions. The
// `.out` lines are quoted above each case.

// `broken_local_action.out`: `could not parse action metadata in
// ".+broken_input": line 7: .+`.
#[test]
fn fixture_broken_input() {
    assert_eq!(
        decode(fixture!(
            "projects/broken_local_action/action/broken_input/action.yml"
        )),
        "error: line 7: cannot unmarshal !!map into bool\n",
    );
}

// `broken_local_action.out`: `could not parse action metadata in
// ".+broken_output": line 4: yaml: outputs must be mapping node but scalar
// node was found at line:4, col:10`.
#[test]
fn fixture_broken_output() {
    assert_eq!(
        decode(fixture!(
            "projects/broken_local_action/action/broken_output/action.yml"
        )),
        "error: line 4: yaml: outputs must be mapping node but scalar node was found at line:4, col:10\n",
    );
}

// `broken_local_action.out`: `could not parse action metadata in
// ".+duplicate_input": line 5: input "FOO" is duplicated`, and the same for
// `duplicate_output`.
#[test]
fn fixture_duplicate_input_and_output() {
    assert_eq!(
        decode(fixture!(
            "projects/broken_local_action/action/duplicate_input/action.yml"
        )),
        "error: line 5: input \"FOO\" is duplicated\n",
    );
    assert_eq!(
        decode(fixture!(
            "projects/broken_local_action/action/duplicate_output/action.yml"
        )),
        "error: line 5: output \"FOO\" is duplicated\n",
    );
}

// `broken_local_action.out`: `could not parse action metadata in
// ".+multiple_unmarshal_errors": yaml: unmarshal errors:  line 4: yaml: .+
// line 5: yaml: .+`. The reference removes the line breaks of the joined
// errors.
#[test]
fn fixture_multiple_unmarshal_errors() {
    assert_eq!(
        decode(fixture!(
            "projects/broken_local_action/action/multiple_unmarshal_errors/action.yml"
        )),
        "error: yaml: unmarshal errors:  line 4: yaml: inputs must be mapping node but scalar node was found at line:4, col:9  line 5: yaml: outputs must be mapping node but scalar node was found at line:5, col:10\n",
    );
}

// `local_action_invalid.out` reports the missing name and description, the
// branding, and the empty deprecation messages from the metadata, which
// decodes. The unknown key is a decoding error: `could not parse action
// metadata in "...unknown_key": line 6: unexpected key "foo" for definition
// of input "input1"`.
#[test]
fn fixture_local_action_invalid() {
    assert_eq!(
        decode(fixture!(
            "projects/local_action_invalid/no_name/action.yaml"
        )),
        format!("name: \"\"\ndescription: \"my action\"\n{NODE20_RUNS}"),
    );
    assert_eq!(
        decode(fixture!(
            "projects/local_action_invalid/no_desc/action.yaml"
        )),
        format!("name: \"My action\"\ndescription: \"\"\n{NODE20_RUNS}"),
    );
    assert_eq!(
        decode(fixture!(
            "projects/local_action_invalid/branding/action.yaml"
        )),
        "name: \"Incorrect branding\"\ndescription: \"my action\"\n".to_owned()
            + &NODE20_RUNS.replace(
                r#"icon="" color="""#,
                r#"icon="does-not-exist" color="no-color""#
            ),
    );
    assert_eq!(
        decode(fixture!(
            "projects/local_action_invalid/unknown_key/action.yaml"
        )),
        "error: line 6: unexpected key \"foo\" for definition of input \"input1\"\n",
    );
    let want = r#"name: "My action"
description: "Test"
input "empty1": name="empty1" required=false deprecated=true message=""
input "empty2": name="empty2" required=false deprecated=true message=""
input "empty3": name="empty3" required=false deprecated=true message=""
input "empty4": name="empty4" required=false deprecated=true message=""
"#;
    assert_eq!(
        decode(fixture!(
            "projects/local_action_invalid/empty_deprecation_message/action.yaml"
        )),
        format!("{want}{NODE20_RUNS}"),
    );
}

// `local_action_yaml_1.1_boolean.out` is empty: `yes` and `on` make inputs
// required, so the call that sets them is valid.
#[test]
fn fixture_yaml_1_1_booleans() {
    let want = r#"name: "My action"
description: "This is test"
input "input1": name="input1" required=true deprecated=false message=""
input "input2": name="input2" required=true deprecated=false message=""
input "input3": name="input3" required=false deprecated=false message=""
input "input4": name="input4" required=false deprecated=false message=""
"#;
    assert_eq!(
        decode(fixture!(
            "projects/local_action_yaml_1.1_boolean/action/action.yaml"
        )),
        format!("{want}{NODE20_RUNS}"),
    );
}

// `yaml_anchor_in_action.out` is empty. `bar: *input` is an alias of `foo`.
#[test]
fn fixture_yaml_anchor_in_action() {
    assert_eq!(
        decode(fixture!(
            "projects/yaml_anchor_in_action/action/action.yaml"
        )),
        r#"name: "Docker action"
description: "This is test"
input "bar": name="bar" required=false deprecated=false message=""
input "foo": name="foo" required=false deprecated=false message=""
output "foo": name="foo"
runs: using="docker" main="" pre="" pre-if="" post="" post-if="" image="ghcr.io/rhysd/actionlint:latest" pre-entrypoint="" entrypoint="" post-entrypoint="" steps=false
branding: icon="" color=""
"#,
    );
}

// Typed decoding rules.

#[test]
fn required_accepts_yaml_1_1_booleans() {
    for (value, required) in [
        ("yes", true),
        ("Yes", true),
        ("YES", true),
        ("y", true),
        ("Y", true),
        ("on", true),
        ("On", true),
        ("ON", true),
        ("true", true),
        ("True", true),
        ("no", false),
        ("N", false),
        ("off", false),
        ("OFF", false),
        ("false", false),
        ("FALSE", false),
        ("~", false),
        // go-yaml reads the YAML 1.1 forms from the decoded text, so quotes
        // do not matter.
        ("'yes'", true),
        ("\"off\"", false),
    ] {
        let text = format!("inputs:\n  a:\n    required: {value}\n");
        assert_eq!(
            decode(&text),
            format!(
                "name: \"\"\ndescription: \"\"\ninput \"a\": name=\"a\" required={required} deprecated=false message=\"\"\n{NO_RUNS}"
            ),
            "{value}",
        );
    }
}

#[test]
fn required_rejects_other_values() {
    assert_eq!(
        decode("inputs:\n  a:\n    required: 1\n"),
        "error: line 3: cannot unmarshal !!int `1` into bool\n",
    );
    // A quoted `true` is a string, and the YAML 1.1 forms do not include
    // `true`.
    assert_eq!(
        decode("inputs:\n  a:\n    required: \"yes\"\n  b:\n    required: \"true\"\n"),
        "error: line 5: cannot unmarshal !!str `true` into bool\n",
    );
    assert_eq!(
        decode("inputs:\n  a:\n    required: !!bool yes\n"),
        "error: line 2: yaml: cannot decode !!str `yes` as a !!bool\n",
    );
}

#[test]
fn a_null_default_counts_as_no_default() {
    assert_eq!(
        decode("inputs:\n  a:\n    required: yes\n    default:\n"),
        format!(
            "name: \"\"\ndescription: \"\"\ninput \"a\": name=\"a\" required=true deprecated=false message=\"\"\n{NO_RUNS}"
        ),
    );
}

#[test]
fn deprecation_messages_are_trimmed() {
    assert_eq!(
        decode("inputs:\n  a:\n    required: ~\n    deprecationMessage: \"  hi\\n \"\n"),
        format!(
            "name: \"\"\ndescription: \"\"\ninput \"a\": name=\"a\" required=false deprecated=true message=\"hi\"\n{NO_RUNS}"
        ),
    );
}

#[test]
fn exact_duplicate_input_names_are_duplicates() {
    // The reference decodes `inputs` by hand, so go-yaml's duplicate key
    // check does not run there.
    assert_eq!(
        decode("inputs:\n  foo:\n    description: a\n  foo:\n    description: b\n"),
        "error: line 2: input \"foo\" is duplicated\n",
    );
}

#[test]
fn case_only_duplicate_input_names_are_duplicates() {
    assert_eq!(
        decode("inputs:\n  Foo:\n    description: a\n  fOO:\n    description: b\n"),
        "error: line 2: input \"fOO\" is duplicated\n",
    );
}

#[test]
fn exact_duplicate_keys_are_rejected() {
    assert_eq!(
        decode("name: a\nname: b\n"),
        "error: line 2: mapping key \"name\" already defined at line 1\n",
    );
    assert_eq!(
        decode("runs:\n  steps:\n    - {run: a, run: b}\n"),
        "error: line 3: mapping key \"run\" already defined at line 3\n",
    );
    // Keys that differ in case are different keys.
    assert_eq!(
        decode("name: a\nName: b\n"),
        format!("name: \"a\"\ndescription: \"\"\n{NO_RUNS}"),
    );
}

#[test]
fn unknown_keys_are_ignored_with_their_values() {
    assert_eq!(
        decode("author: [x, {a: 1, a: 2}]\nname: a\n"),
        format!("name: \"a\"\ndescription: \"\"\n{NO_RUNS}"),
    );
}

#[test]
fn values_of_the_wrong_type_are_type_errors() {
    assert_eq!(
        decode(
            "runs:\n  using: [a]\n  main: {a: 1}\n  steps: foo\n  env: bar\n  args: {a: 1}\nbranding: 12345678901\n"
        ),
        "error: yaml: unmarshal errors:  line 2: cannot unmarshal !!seq into string  line 3: cannot unmarshal !!map into string  line 4: cannot unmarshal !!str `foo` into []interface {}  line 5: cannot unmarshal !!str `bar` into map[string]interface {}  line 6: cannot unmarshal !!map into []interface {}  line 7: cannot unmarshal !!int `1234567...` into actionlint.ActionMetadataBranding\n",
    );
    assert_eq!(
        decode("foo\n"),
        "error: line 1: cannot unmarshal !!str `foo` into actionlint.ActionMetadata\n",
    );
    assert_eq!(
        decode("runs:\n  env:\n    [a]: 1\n"),
        "error: line 3: cannot unmarshal !!seq into string\n",
    );
}

#[test]
fn an_empty_file_has_empty_metadata() {
    assert_eq!(
        decode(""),
        format!("name: \"\"\ndescription: \"\"\n{NO_RUNS}")
    );
    assert_eq!(
        decode("# comment\n"),
        format!("name: \"\"\ndescription: \"\"\n{NO_RUNS}")
    );
}

#[test]
fn merge_keys_apply_outside_inputs() {
    assert_eq!(
        decode("base: &b\n  name: merged\n  description: d\n<<: *b\nname: explicit\n"),
        format!("name: \"explicit\"\ndescription: \"d\"\n{NO_RUNS}"),
    );
    assert_eq!(
        decode("<<: foo\n"),
        "error: yaml: map merge requires map or sequence of maps as the value\n",
    );
    assert_eq!(
        decode("runs:\n  steps:\n    - <<: [{a: 1}, foo]\n"),
        "error: yaml: map merge requires map or sequence of maps as the value\n",
    );
    assert_eq!(
        decode("{[a]: 1, <<: {name: x}}\n"),
        "error: yaml: runtime error: hash of unhashable type []interface {}\n",
    );
}

#[test]
fn an_alias_key_can_set_a_field_twice() {
    assert_eq!(
        decode("k: &k name\n*k : foo\nname: bar\n"),
        "error: line 3: field name already set in type actionlint.ActionMetadata\n",
    );
}

#[test]
fn an_alias_that_contains_itself_ends_decoding() {
    assert_eq!(
        decode("x: &x\n  y: *x\nruns:\n  steps: [*x]\n"),
        "error: yaml: anchor 'x' value contains itself\n",
    );
    // Fields that are not decoded do not reach the alias.
    assert_eq!(
        decode("x: &x\n  y: *x\nname: a\n"),
        format!("name: \"a\"\ndescription: \"\"\n{NO_RUNS}"),
    );
}

#[test]
fn collection_keys_inside_steps_end_decoding() {
    assert_eq!(
        decode("runs:\n  steps:\n    - {[a]: 1}\n"),
        "error: yaml: cannot use '[]interface {}{\"a\"}' as a map key; try decoding into yaml.Node\n",
    );
    assert_eq!(
        decode("runs:\n  steps:\n    - {{a: 1, b: [1.5, 2, true, ~]}: 1}\n"),
        "error: yaml: cannot use 'map[string]interface {}{\"a\":1, \"b\":[]interface {}{1.5, 2, true, interface {}(nil)}}' as a map key; try decoding into yaml.Node\n",
    );
}

#[test]
fn binary_scalars_are_decoded() {
    assert_eq!(
        decode("name: !!binary aGVsbG8=\n"),
        format!("name: \"hello\"\ndescription: \"\"\n{NO_RUNS}"),
    );
    assert_eq!(
        decode("name: !!binary aGVsbG8\n"),
        "error: yaml: !!binary value contains invalid base64 data\n",
    );
}

#[test]
fn an_explicit_tag_must_match_the_value() {
    assert_eq!(
        decode("inputs:\n  a:\n    default: !!int x\n"),
        "error: line 2: yaml: cannot decode !!str `x` as a !!int\n",
    );
}

#[test]
fn syntax_errors_give_go_yaml_texts() {
    assert_eq!(
        decode("a: [\n"),
        "error: yaml: line 1: did not find expected node content\n",
    );
    assert_eq!(
        decode("name: x\n  bad: indent\n"),
        "error: yaml: line 2: mapping values are not allowed in this context\n",
    );
    assert_eq!(
        decode("a: *nope\n"),
        "error: yaml: unknown anchor 'nope' referenced\n",
    );
    assert_eq!(
        decode("name: \u{1}\n"),
        "error: yaml: control characters are not allowed\n",
    );
}

// docs/differences.md#action-metadata-aliases: the reference skips the
// `deprecationMessage` scan for an aliased input, so `bar` is not
// deprecated there.
#[test]
fn an_aliased_input_with_a_deprecation_message_is_deprecated() {
    let text = "inputs:\n  foo: &input\n    deprecationMessage: old\n  bar: *input\n";
    assert_eq!(
        decode(text),
        format!(
            "name: \"\"\ndescription: \"\"\ninput \"bar\": name=\"bar\" required=false deprecated=true message=\"old\"\ninput \"foo\": name=\"foo\" required=false deprecated=true message=\"old\"\n{NO_RUNS}"
        ),
    );
}

// docs/differences.md#action-metadata-aliases: the reference skips the
// unknown key scan for an aliased input.
#[test]
fn an_aliased_input_with_an_unknown_key_is_an_error() {
    let text = "x: &x\n  foo: 1\ninputs:\n  a: *x\n";
    assert_eq!(
        decode(text),
        "error: line 4: unexpected key \"foo\" for definition of input \"a\"\n",
    );
}

// docs/differences.md#action-metadata-aliases: the reference reads an
// alias key under `inputs` as the anchor name.
#[test]
fn an_alias_key_under_inputs_is_its_value() {
    let text = "k: &k foo\ninputs:\n  *k : {required: true}\n";
    assert_eq!(
        decode(text),
        format!(
            "name: \"\"\ndescription: \"\"\ninput \"foo\": name=\"foo\" required=true deprecated=false message=\"\"\n{NO_RUNS}"
        ),
    );
}

// docs/differences.md#action-metadata-aliases: the reference makes `<<` an
// input and reports `unexpected key "x" for definition of input "<<"`.
#[test]
fn a_merge_key_under_inputs_merges_the_inputs() {
    let text = "common: &common
  token:
    required: true
  debug:
    deprecationMessage: unused
inputs:
  <<: *common
  name: {}
  debug: {}
";
    assert_eq!(
        decode(text),
        format!(
            "name: \"\"\ndescription: \"\"\ninput \"debug\": name=\"debug\" required=false deprecated=false message=\"\"\ninput \"name\": name=\"name\" required=false deprecated=false message=\"\"\ninput \"token\": name=\"token\" required=true deprecated=false message=\"\"\n{NO_RUNS}"
        ),
    );
    // A sequence of mappings merges each, and earlier mappings win.
    let text = "inputs:\n  <<: [{a: {required: true}}, {a: {}, b: {}}]\n";
    assert_eq!(
        decode(text),
        format!(
            "name: \"\"\ndescription: \"\"\ninput \"a\": name=\"a\" required=true deprecated=false message=\"\"\ninput \"b\": name=\"b\" required=false deprecated=false message=\"\"\n{NO_RUNS}"
        ),
    );
    // A merged input that differs from another only in case is a
    // duplicate.
    assert_eq!(
        decode("inputs:\n  <<: {FOO: {}}\n  foo: {}\n"),
        "error: line 2: input \"FOO\" is duplicated\n",
    );
    assert_eq!(
        decode("inputs:\n  <<: foo\n"),
        "error: line 2: yaml: map merge requires map or sequence of maps as the value\n",
    );
    assert_eq!(
        decode("inputs: &i\n  <<: *i\n"),
        "error: line 1: yaml: anchor 'i' value contains itself\n",
    );
}

#[test]
fn excessive_aliasing_ends_decoding() {
    let mut text = String::from("a0: &a0 [x, x, x, x, x, x, x, x, x, x]\n");
    for i in 1..10 {
        let prev = i - 1;
        let _ = writeln!(
            text,
            "a{i}: &a{i} [*a{prev}, *a{prev}, *a{prev}, *a{prev}, *a{prev}, *a{prev}, *a{prev}, *a{prev}, *a{prev}, *a{prev}]"
        );
    }
    text.push_str("runs:\n  steps: *a9\n");
    assert_eq!(
        decode(&text),
        "error: yaml: document contains excessive aliasing\n"
    );
}

/// Decodes `text` on a thread with a 2 MiB stack, the default size of a
/// spawned thread.
fn decode_on_a_small_stack(text: String) -> String {
    std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(move || decode(&text))
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn deeply_nested_steps_decode_on_a_small_stack() {
    let text = format!(
        "name: x\nruns:\n  steps: {}{}\n",
        "[".repeat(9999),
        "]".repeat(9999)
    );
    assert_eq!(
        decode_on_a_small_stack(text),
        format!(
            "name: \"x\"\ndescription: \"\"\n{}",
            NO_RUNS.replace("steps=false", "steps=true")
        ),
    );
}

#[test]
fn a_deeply_nested_map_key_is_printed_in_the_error_on_a_small_stack() {
    let text = format!(
        "name: x\nruns:\n  steps: [{{? {}x{} : y}}]\n",
        "[".repeat(6000),
        "]".repeat(6000)
    );
    let key = format!(
        "{}\"x\"{}",
        "[]interface {}{".repeat(6000),
        "}".repeat(6000)
    );
    assert_eq!(
        decode_on_a_small_stack(text),
        format!("error: yaml: cannot use '{key}' as a map key; try decoding into yaml.Node\n"),
    );
}

#[test]
fn deeply_nested_merges_decode_on_a_small_stack() {
    let merges = |inner: &str| format!("{}{inner}{}", "{<<: ".repeat(5000), "}".repeat(5000));
    assert_eq!(
        decode_on_a_small_stack(format!("name: x\nbranding: {}\n", merges("{icon: x}"))),
        format!(
            "name: \"x\"\ndescription: \"\"\n{}",
            NO_RUNS.replace("icon=\"\"", "icon=\"x\"")
        ),
    );
    assert_eq!(
        decode_on_a_small_stack(format!("runs:\n  env: {}\n", merges("{A: x}"))),
        format!("name: \"\"\ndescription: \"\"\n{NO_RUNS}"),
    );
    // The inputs list merged entries as docs/differences.md#action-metadata-aliases
    // describes.
    assert_eq!(
        decode_on_a_small_stack(format!("inputs: {}\n", merges("{A: {required: true}}"))),
        format!(
            "name: \"\"\ndescription: \"\"\ninput \"a\": name=\"A\" required=true deprecated=false message=\"\"\n{NO_RUNS}"
        ),
    );
}

#[test]
fn steps_args_and_env_are_set_only_by_non_null_values() {
    let runs = |text: &str| {
        let file = SourceFile::new(FileId::new(0), "action.yml", text).unwrap();
        let runs = ActionMetadata::decode(&file).unwrap().runs;
        (runs.steps, runs.has_args, runs.has_env)
    };
    assert_eq!(runs("runs: {}\n"), (None, false, false));
    assert_eq!(
        runs("runs: {steps: , args: , env: }\n"),
        (None, false, false)
    );
    assert_eq!(
        runs("runs: {steps: [], args: [], env: {}}\n"),
        (Some(0), true, true)
    );
    assert_eq!(
        runs("x: &s [a]\nruns: {steps: *s}\n"),
        (Some(1), false, false)
    );
}
