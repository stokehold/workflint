//! The `action` rule on local actions: the `with:` inputs of each use, and the metadata of each
//! action on its first use in the run.

#![allow(clippy::unwrap_used, clippy::panic)]

use workflint_diagnostics::{Color, FileId, Severity, SourceFile, render_oneline, sort_and_dedup};
use workflint_rules::local::{LocalMetadata, MemoryProject};
use workflint_rules::registry;

const NONE: [&str; 0] = [];

/// Parses `text` as the workflow at `path`, runs the rules over it in `project`, and returns the
/// diagnostics of the parser and the rules in the one-line format.
fn lint_file(project: &dyn LocalMetadata, path: &str, text: &str) -> Vec<String> {
    let file = SourceFile::new(FileId::new(0), path, text).unwrap();
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

/// Lints `test.yaml`, whose job has one step for each of `steps`, a YAML mapping indented for a
/// step. The first step's `uses:` value is at line 6, column 15.
fn lint_steps(project: &dyn LocalMetadata, steps: &[&str]) -> Vec<String> {
    let mut text = "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n".to_owned();
    for step in steps {
        text.push_str("      - ");
        text.push_str(step);
        text.push('\n');
    }
    lint_file(project, "test.yaml", &text)
}

/// Lints `test.yaml` with one step that uses `./a`, the action whose metadata is `metadata`.
fn lint_action(metadata: &str) -> Vec<String> {
    let project = MemoryProject::new().with_file("a/action.yml", metadata);
    lint_steps(&project, &["uses: ./a"])
}

/// A valid JavaScript action with the inputs `who` (required) and `greeting` (optional).
const GREET: &str = "name: Greet
description: Says hello
inputs:
  who:
    required: true
  greeting:
    default: hello
runs:
  using: node24
  main: index.js
";

fn greet() -> MemoryProject {
    MemoryProject::new()
        .with_file("greet/action.yml", GREET)
        .with_file("greet/index.js", "")
}

#[test]
fn a_valid_local_action_with_its_required_inputs_is_valid() {
    assert_eq!(
        lint_steps(
            &greet(),
            &["{uses: ./greet, with: {who: me, Greeting: hi}}"]
        ),
        NONE
    );
}

#[test]
fn inputs_of_a_local_action_are_checked_at_every_use() {
    assert_eq!(
        lint_steps(
            &greet(),
            &[
                "{uses: ./greet, with: {who: me, whom: you}}",
                "{uses: ./greet}",
            ]
        ),
        [
            "test.yaml:6:41: input \"whom\" is not defined in action \"Greet\" defined at \"./greet\". available inputs are \"greeting\", \"who\" [undefined-action-input]",
            "test.yaml:7:16: missing input \"who\" which is required by action \"Greet\" defined at \"./greet\". all required inputs are \"who\" [missing-required-action-input]",
        ]
    );
}

#[test]
fn a_deprecated_input_of_a_local_action_is_reported_with_its_message() {
    let metadata = "name: A
description: d
inputs:
  old:
    deprecationMessage: |
      use new.
      it is better.
  new: {}
runs:
  using: composite
  steps: []
";
    let project = MemoryProject::new().with_file("a/action.yml", metadata);
    assert_eq!(
        lint_steps(&project, &["{uses: ./a, with: {old: x}}"]),
        [
            "test.yaml:6:28: avoid using deprecated input \"old\" in action \"A\" defined at \"./a\": use new. it is better [deprecated-action-input]"
        ]
    );
}

#[test]
fn a_missing_local_action_and_an_expression_are_not_checked() {
    let project = MemoryProject::new().with_file("a/action.yml", "runs: {}\n");
    assert_eq!(
        lint_steps(&project, &["uses: ./missing", "uses: ./${{ 'a' }}"]),
        NONE
    );
}

#[test]
fn metadata_is_checked_at_the_first_use_and_inputs_at_every_use() {
    let project = MemoryProject::new().with_file(
        "a/action.yml",
        "name: A\nruns:\n  using: composite\n  steps: []\n",
    );
    assert_eq!(
        lint_steps(
            &project,
            &["{uses: ./a, with: {x: 1}}", "{uses: ./a, with: {x: 1}}"]
        ),
        [
            "test.yaml:6:16: description is required in metadata of \"A\" action at \"a/action.yml\" [missing-action-description]",
            "test.yaml:6:28: input \"x\" is not defined in action \"A\" defined at \"./a\". available inputs are  [undefined-action-input]",
            "test.yaml:7:28: input \"x\" is not defined in action \"A\" defined at \"./a\". available inputs are  [undefined-action-input]",
        ]
    );
}

#[test]
fn broken_metadata_is_reported_at_the_first_use_without_input_checks() {
    let project = MemoryProject::new().with_file("a/action.yml", "outputs: x\n");
    assert_eq!(
        lint_steps(&project, &["{uses: ./a, with: {x: 1}}", "uses: ./a"]),
        [
            "test.yaml:6:16: could not parse action metadata in \"a\": line 1: yaml: outputs must be mapping node but scalar node was found at line:1, col:10 [broken-action-metadata]"
        ]
    );
}

#[test]
fn broken_metadata_used_from_two_files_is_reported_once_at_the_first_file() {
    let project = MemoryProject::new().with_file("a/action.yml", "outputs: x\n");
    let workflow =
        "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: ./a\n";
    assert_eq!(
        lint_file(&project, "workflows/a.yaml", workflow),
        [
            "workflows/a.yaml:6:15: could not parse action metadata in \"a\": line 1: yaml: outputs must be mapping node but scalar node was found at line:1, col:10 [broken-action-metadata]"
        ]
    );
    assert_eq!(lint_file(&project, "workflows/b.yaml", workflow), NONE);
}

#[test]
fn name_and_description_are_required() {
    assert_eq!(
        lint_action("runs:\n  using: composite\n  steps: []\n"),
        [
            "test.yaml:6:15: description is required in metadata of \"\" action at \"a/action.yml\" [missing-action-description]",
            "test.yaml:6:15: name is required in action metadata \"a/action.yml\" [missing-action-name]",
        ]
    );
}

#[test]
fn branding_icon_and_color_must_be_known_in_any_case() {
    let ok = "name: A\ndescription: d\nbranding: {icon: Git-Branch, color: Gray-Dark}\nruns:\n  using: composite\n  steps: []\n";
    assert_eq!(lint_action(ok), NONE);
    let bad = "name: A\ndescription: d\nbranding: {icon: dog, color: pink}\nruns:\n  using: composite\n  steps: []\n";
    assert_eq!(
        lint_action(bad),
        [
            "test.yaml:6:15: incorrect color \"pink\" at branding.color in metadata of \"A\" action at \"a/action.yml\". see the official document to know the exhaustive list of supported colors: https://docs.github.com/en/actions/creating-actions/metadata-syntax-for-github-actions#brandingcolor [invalid-branding-color]",
            "test.yaml:6:15: incorrect icon name \"dog\" at branding.icon in metadata of \"A\" action at \"a/action.yml\". see the official document to know the exhaustive list of supported icons: https://docs.github.com/en/actions/creating-actions/metadata-syntax-for-github-actions#brandingicon [invalid-branding-icon]",
        ]
    );
}

#[test]
fn a_deprecated_input_needs_a_message() {
    let metadata = "name: A\ndescription: d\ninputs:\n  b: {deprecationMessage: ' '}\n  a: {deprecationMessage: }\n  c: {deprecationMessage: why}\nruns:\n  using: composite\n  steps: []\n";
    assert_eq!(
        lint_action(metadata),
        [
            "test.yaml:6:15: input \"a\" is deprecated but \"deprecationMessage\" is empty in metadata of \"A\" action at \"a/action.yml\" [empty-deprecation-message]",
            "test.yaml:6:15: input \"b\" is deprecated but \"deprecationMessage\" is empty in metadata of \"A\" action at \"a/action.yml\" [empty-deprecation-message]",
        ]
    );
}

#[test]
fn runs_using_must_be_set_and_known() {
    assert_eq!(
        lint_action("name: A\ndescription: d\nruns: {}\n"),
        [
            "test.yaml:6:15: \"runs.using\" is missing in local action \"A\" defined at \"a\" [missing-runs-using]"
        ]
    );
    assert_eq!(
        lint_action("name: A\ndescription: d\nruns: {using: python}\n"),
        [
            "test.yaml:6:15: invalid runner name \"python\" at runs.using in \"A\" action defined at \"a\". valid runners are \"composite\", \"docker\", \"node20\", and \"node24\". see https://docs.github.com/en/actions/creating-actions/metadata-syntax-for-github-actions#runs [invalid-runs-using]"
        ]
    );
}

#[test]
fn an_unknown_node_runner_is_also_checked_as_a_javascript_action() {
    assert_eq!(
        lint_action("name: A\ndescription: d\nruns: {using: node16}\n"),
        [
            "test.yaml:6:15: invalid runner name \"node16\" at runs.using in \"A\" action defined at \"a\". valid runners are \"composite\", \"docker\", \"node20\", and \"node24\". see https://docs.github.com/en/actions/creating-actions/metadata-syntax-for-github-actions#runs [invalid-runs-using]",
            "test.yaml:6:15: \"main\" is required in \"runs\" section because \"A\" is a JavaScript action. the action is defined at \"a\" [missing-runs-key]",
        ]
    );
}

#[test]
fn a_javascript_action_needs_main_and_its_files() {
    let metadata = "name: A
description: d
runs:
  using: node20
  main: dist/index.js
  pre: pre.js
  post-if: always()
  steps: []
  env: {}
";
    assert_eq!(
        lint_action(metadata),
        [
            "test.yaml:6:15: file \"dist/index.js\" does not exist in \"a\". it is specified at \"main\" key in \"runs\" section in \"A\" action [action-file-not-found]",
            "test.yaml:6:15: file \"pre.js\" does not exist in \"a\". it is specified at \"pre\" key in \"runs\" section in \"A\" action [action-file-not-found]",
            "test.yaml:6:15: \"post\" is required when \"post-if\" is specified in \"runs\" section in \"A\" action. the action is defined at \"a\" [missing-pre-post-script]",
            "test.yaml:6:15: \"env\" is not allowed in \"runs\" section because \"A\" is a JavaScript action. the action is defined at \"a\" [runs-key-not-allowed]",
        ]
    );
}

#[test]
fn a_javascript_action_with_its_files_is_valid() {
    let project = MemoryProject::new()
        .with_file(
            "a/action.yml",
            "name: A\ndescription: d\nruns: {using: node24, main: index.js, pre: setup/pre.js, pre-if: always()}\n",
        )
        .with_file("a/index.js", "")
        .with_file("a/setup/pre.js", "");
    assert_eq!(lint_steps(&project, &["uses: ./a"]), NONE);
}

#[test]
fn a_composite_action_needs_steps_and_no_other_keys() {
    assert_eq!(
        lint_action("name: A\ndescription: d\nruns: {using: composite}\n"),
        [
            "test.yaml:6:15: \"steps\" is required in \"runs\" section because \"A\" is a Composite action. the action is defined at \"a\" [missing-runs-key]"
        ]
    );
    assert_eq!(
        lint_action(
            "name: A\ndescription: d\nruns: {using: composite, steps: [], args: [], main: x}\n"
        ),
        [
            "test.yaml:6:15: \"args\" is not allowed in \"runs\" section because \"A\" is a Composite action. the action is defined at \"a\" [runs-key-not-allowed]",
            "test.yaml:6:15: \"main\" is not allowed in \"runs\" section because \"A\" is a Composite action. the action is defined at \"a\" [runs-key-not-allowed]",
        ]
    );
}

#[test]
fn a_null_args_or_env_counts_as_unset() {
    assert_eq!(
        lint_action(
            "name: A\ndescription: d\nruns: {using: composite, steps: [], args: , env: }\n"
        ),
        NONE
    );
}

#[test]
fn a_docker_action_needs_an_image_named_dockerfile() {
    assert_eq!(
        lint_action("name: A\ndescription: d\nruns: {using: docker, steps: [x]}\n"),
        [
            "test.yaml:6:15: \"image\" is required in \"runs\" section because \"A\" is a Docker action. the action is defined at \"a\" [missing-runs-key]",
            "test.yaml:6:15: \"steps\" is not allowed in \"runs\" section because \"A\" is a Docker action. the action is defined at \"a\" [runs-key-not-allowed]",
        ]
    );
    let project = MemoryProject::new()
        .with_file(
            "a/action.yml",
            "name: A\ndescription: d\nruns: {using: docker, image: build/Dockerfile.dev, entrypoint: run.sh}\n",
        )
        .with_file("a/build/Dockerfile.dev", "");
    assert_eq!(
        lint_steps(&project, &["uses: ./a"]),
        [
            "test.yaml:6:15: file \"run.sh\" does not exist in \"a\". it is specified at \"entrypoint\" key in \"runs\" section in \"A\" action [action-file-not-found]",
            "test.yaml:6:15: the local file \"build/Dockerfile.dev\" referenced from \"image\" key must be named \"Dockerfile\" in \"A\" action. the action is defined at \"a\" [invalid-dockerfile-name]",
        ]
    );
}

#[test]
fn a_docker_image_on_a_registry_is_not_a_local_file() {
    assert_eq!(
        lint_action("name: A\ndescription: d\nruns: {using: docker, image: 'docker://alpine:3'}\n"),
        NONE
    );
}

#[test]
fn a_missing_action_file_is_a_warning() {
    let rule = registry::find("action-file-not-found").unwrap();
    assert_eq!(rule.default_severity, Severity::Warning);
}
