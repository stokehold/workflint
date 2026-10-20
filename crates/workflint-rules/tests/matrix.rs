//! The `matrix` rule: duplicate row values and `exclude` entries that match no row.

#![allow(clippy::unwrap_used, clippy::panic)]

use workflint_diagnostics::{Color, FileId, SourceFile, render_oneline, sort_and_dedup};

/// Lints `workflow` as the binary does, without the file system, and returns the diagnostics of
/// the `matrix` kind in the one-line format.
fn lint(workflow: &str) -> Vec<String> {
    let file = SourceFile::new(FileId::new(0), "test.yaml", workflow).unwrap();
    let parsed = workflint_yaml::parse(&file);
    let document = parsed.document.unwrap();
    let syntax = workflint_syntax::parse(&file, &document);
    let mut diagnostics = workflint_rules::run(&file, &syntax.workflow);
    sort_and_dedup(&file, &mut diagnostics);
    let mut out = Vec::new();
    render_oneline(&mut out, &file, &diagnostics, Color::Off).unwrap();
    String::from_utf8(out)
        .unwrap()
        .lines()
        .filter(|line| {
            let id = line.rsplit_once(" [").unwrap().1.trim_end_matches(']');
            workflint_rules::registry::find(id).unwrap().kind == "matrix"
        })
        .map(str::to_owned)
        .collect()
}

/// Returns a workflow with one job whose `strategy.matrix` has the lines `matrix`, each indented
/// under `matrix:`.
fn workflow(matrix: &[&str]) -> String {
    let mut text = String::from("on: push\njobs:\n  test:\n    strategy:\n      matrix:\n");
    for line in matrix {
        text.push_str("        ");
        text.push_str(line);
        text.push('\n');
    }
    text.push_str("    runs-on: ubuntu-latest\n    steps:\n      - run: echo\n");
    text
}

/// Lints the matrix of [`workflow`] and returns the messages without positions and rule IDs.
fn messages(matrix: &[&str]) -> Vec<String> {
    lint(&workflow(matrix))
        .into_iter()
        .map(|line| {
            let message = line.splitn(4, ':').nth(3).unwrap();
            message.rsplit_once(" [").unwrap().0.trim().to_owned()
        })
        .collect()
}

#[test]
fn a_duplicate_value_is_reported_with_the_position_of_the_first() {
    assert_eq!(
        lint(&workflow(&["node: [10, 12, 10, 12, 10]"])),
        [
            r#"test.yaml:6:24: duplicate value "10" is found in matrix "node". the same value is at line:6,col:16 [duplicate-matrix-value]"#,
            r#"test.yaml:6:28: duplicate value "12" is found in matrix "node". the same value is at line:6,col:20 [duplicate-matrix-value]"#,
            r#"test.yaml:6:32: duplicate value "10" is found in matrix "node". the same value is at line:6,col:16 [duplicate-matrix-value]"#,
        ]
    );
}

#[test]
fn the_message_names_the_row_as_written() {
    assert_eq!(
        messages(&["Node-Version: [1, 1]"]),
        [
            r#"duplicate value "1" is found in matrix "Node-Version". the same value is at line:6,col:24"#
        ]
    );
}

#[test]
fn mappings_are_duplicates_whatever_the_order_and_case_of_their_keys() {
    assert_eq!(
        messages(&["os: [{name: a, Arch: x}, {arch: x, name: a}, {name: a}]"]),
        [
            r#"duplicate value {"arch": "x", "name": "a"} is found in matrix "os". the same value is at line:6,col:14"#
        ]
    );
}

#[test]
fn sequences_are_duplicates_when_their_items_are_equal_in_order() {
    assert_eq!(
        messages(&["v: [[a, b], [b, a], [a, b], [a]]"]),
        [
            r#"duplicate value ["a", "b"] is found in matrix "v". the same value is at line:6,col:13"#
        ]
    );
}

#[test]
fn scalars_compare_by_their_text_whatever_their_quotes() {
    // The reference compares the text of scalars, so `1` and `"1"` are the same value.
    assert_eq!(
        messages(&[r#"node: [1, "1", '1']"#]),
        [
            r#"duplicate value "1" is found in matrix "node". the same value is at line:6,col:16"#,
            r#"duplicate value "1" is found in matrix "node". the same value is at line:6,col:16"#,
        ]
    );
}

#[test]
fn a_value_of_another_kind_is_not_a_duplicate() {
    assert_eq!(
        messages(&["v: [a, [a], {a: a}, {}, []]"]),
        Vec::<String>::new()
    );
}

#[test]
fn rows_written_as_expressions_are_not_checked() {
    assert_eq!(
        messages(&[
            "os: ${{ fromJSON(vars.OSES) }}",
            "exclude:",
            "  - os: anything",
        ]),
        Vec::<String>::new()
    );
}

#[test]
fn a_whole_matrix_expression_is_not_checked() {
    let text = "on: push\njobs:\n  test:\n    strategy:\n      matrix: ${{ fromJSON(vars.MATRIX) }}\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo\n";
    assert_eq!(lint(text), Vec::<String>::new());
}

#[test]
fn an_exclude_value_that_matches_no_row_value_lists_the_row_values() {
    assert_eq!(
        messages(&[
            "os: [ubuntu-latest, macos-latest]",
            "exclude:",
            "  - os: windows-latest",
        ]),
        [
            r#"value "windows-latest" in "exclude" does not match in matrix "os" combinations. possible values are "ubuntu-latest", "macos-latest""#
        ]
    );
}

#[test]
fn an_exclude_key_that_is_no_row_lists_the_rows_sorted_and_lowercased() {
    assert_eq!(
        lint(&workflow(&[
            "OS: [ubuntu-latest]",
            "Arch: [x64]",
            "exclude:",
            "  - Platform: linux",
        ])),
        [
            r#"test.yaml:9:13: "platform" in "exclude" section does not exist in matrix. available matrix configurations are "arch", "os" [unknown-exclude-key]"#
        ]
    );
}

#[test]
fn an_exclude_key_that_is_no_row_lists_only_the_rows_written_as_sequences() {
    assert_eq!(
        messages(&[
            "os: [a]",
            "node: ${{ fromJSON('[1]') }}",
            "exclude:",
            "  - foo: x",
        ]),
        [
            r#""foo" in "exclude" section does not exist in matrix. available matrix configurations are "os""#
        ]
    );
}

#[test]
fn exclude_keys_match_rows_without_case() {
    assert_eq!(
        messages(&["OS: [ubuntu-latest]", "exclude:", "  - os: ubuntu-latest"]),
        Vec::<String>::new()
    );
}

#[test]
fn an_exclude_object_matches_a_row_object_that_has_its_properties() {
    assert_eq!(
        messages(&[
            "os: [{name: Ubuntu, runner: ubuntu-latest}]",
            "exclude:",
            "  - os: {runner: ubuntu-latest}",
            "  - os: {RUNNER: ubuntu-latest}",
            "  - os: {}",
            "  - os: {runner: windows-latest}",
        ]),
        [
            r#"value {"runner": "windows-latest"} in "exclude" does not match in matrix "os" combinations. possible values are {"name": "Ubuntu", "runner": "ubuntu-latest"}"#
        ]
    );
}

#[test]
fn an_exclude_sequence_matches_a_row_sequence_of_the_same_length_item_by_item() {
    assert_eq!(
        messages(&[
            "v: [[{a: 1, b: 2}, c]]",
            "exclude:",
            "  - v: [{a: 1}, c]",
            "  - v: [{a: 1}]",
        ]),
        [
            r#"value [{"a": "1"}] in "exclude" does not match in matrix "v" combinations. possible values are [{"a": "1", "b": "2"}, "c"]"#
        ]
    );
}

#[test]
fn an_expression_matches_any_value_on_either_side() {
    assert_eq!(
        messages(&[
            "os: [ubuntu-latest, '${{ vars.OS }}-latest', {name: '${{ vars.NAME }}'}]",
            "arch: [x64]",
            "exclude:",
            "  - os: windows-latest",
            "  - os: {name: anything}",
            "  - arch: ${{ vars.ARCH }}",
            "  - arch: ['${{ vars.ARCH }}']",
        ]),
        [
            r#"value ["${{ vars.ARCH }}"] in "exclude" does not match in matrix "arch" combinations. possible values are "x64""#
        ]
    );
}

#[test]
fn exclude_is_matched_against_the_rows_only() {
    // GitHub applies `include` after `exclude`, so `include` values and keys are not candidates.
    assert_eq!(
        messages(&[
            "os: [ubuntu-latest]",
            "include:",
            "  - os: windows-latest",
            "    gui: gnome",
            "exclude:",
            "  - os: windows-latest",
            "  - gui: gnome",
        ]),
        [
            r#"value "windows-latest" in "exclude" does not match in matrix "os" combinations. possible values are "ubuntu-latest""#,
            r#""gui" in "exclude" section does not exist in matrix. available matrix configurations are "os""#,
        ]
    );
}

#[test]
fn an_include_expression_does_not_stop_the_exclude_check() {
    assert_eq!(
        messages(&[
            "os: [ubuntu-latest]",
            "include: ${{ fromJSON(vars.INCLUDE) }}",
            "exclude:",
            "  - os: windows-latest",
        ])
        .len(),
        1
    );
    assert_eq!(
        messages(&[
            "os: [ubuntu-latest]",
            "include:",
            "  - ${{ fromJSON(vars.INCLUDE) }}",
            "exclude:",
            "  - os: windows-latest",
        ])
        .len(),
        1
    );
}

#[test]
fn exclude_without_rows_is_reported_at_the_matrix_key() {
    let text = "on: push\njobs:\n  test:\n    strategy:\n      matrix:\n        include:\n          - os: a\n        exclude:\n          - os: a\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo\n";
    assert_eq!(
        lint(text),
        [
            r#"test.yaml:5:7: "exclude" section exists but no matrix variation exists [exclude-without-matrix]"#
        ]
    );
}

#[test]
fn exclude_written_as_an_expression_or_empty_is_not_checked() {
    assert_eq!(
        messages(&[
            "include: [{os: a}]",
            "exclude: ${{ fromJSON(vars.EXCLUDE) }}"
        ]),
        Vec::<String>::new()
    );
    assert_eq!(
        messages(&["include: [{os: a}]", "exclude: []"]),
        Vec::<String>::new()
    );
}

#[test]
fn an_exclude_element_written_as_an_expression_is_skipped() {
    assert_eq!(
        messages(&[
            "os: [a]",
            "exclude:",
            "  - ${{ fromJSON(vars.EXCLUDE) }}",
            "  - os: b",
        ])
        .len(),
        1
    );
}

/// The levels of a flow sequence inside a matrix row. With the row, a value has the 10,000 flow
/// levels that the YAML layer accepts (ADR 0022).
const DEPTH: usize = 9_999;

/// Returns `leaf` inside [`DEPTH`] flow sequences.
fn deep(leaf: &str) -> String {
    format!("{}{leaf}{}", "[".repeat(DEPTH), "]".repeat(DEPTH))
}

/// Lints the matrix lines that `matrix` returns on a thread with a 2 MiB stack, and returns the
/// messages.
fn messages_on_a_small_stack(matrix: fn() -> Vec<String>) -> Vec<String> {
    std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(move || {
            let lines = matrix();
            let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
            messages(&lines)
        })
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn values_nested_10000_levels_deep_are_compared_and_printed_on_a_small_stack() {
    let lines = messages_on_a_small_stack(|| vec![format!("deep: [{}, {}]", deep("a"), deep("a"))]);
    assert_eq!(
        lines,
        [format!(
            "duplicate value {} is found in matrix \"deep\". the same value is at line:6,col:16",
            deep("\"a\"")
        )]
    );
}

#[test]
fn exclude_values_nested_10000_levels_deep_are_matched_and_printed_on_a_small_stack() {
    let lines = messages_on_a_small_stack(|| {
        vec![
            format!("deep: [{}]", deep("a")),
            "exclude:".to_owned(),
            format!("  - deep: {}", deep("b")),
        ]
    });
    assert_eq!(
        lines,
        [format!(
            "value {} in \"exclude\" does not match in matrix \"deep\" combinations. possible values are {}",
            deep("\"b\""),
            deep("\"a\"")
        )]
    );
}
