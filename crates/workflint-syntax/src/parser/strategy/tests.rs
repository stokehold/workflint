#![allow(clippy::unwrap_used, clippy::panic)]

use std::fmt::Write as _;

use workflint_diagnostics::{FileId, SourceFile, Span};
use workflint_yaml::ScalarStyle;

use crate::testing::render;
use crate::{
    Field, Keyed, Matrix, MatrixCombination, MatrixRow, RawValue, Str, Strategy, Value, Workflow,
    parse,
};

/// Wraps `strategy`, the lines of a `strategy:` mapping indented by six
/// spaces, in a workflow with one job `test`.
fn workflow(strategy: &str) -> String {
    format!(
        "on: push\njobs:\n  test:\n    runs-on: x\n    steps: [{{run: a}}]\n    strategy:\n{strategy}"
    )
}

/// Parses `text` and returns its diagnostics as [`render`] prints them,
/// followed by the lines that `f` returns for the workflow.
fn parse_text(
    text: &str,
    f: impl FnOnce(&SourceFile, &Workflow<'_>) -> Vec<String>,
) -> Vec<String> {
    let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    let parsed = parse(&file, &document);
    let mut lines = render(&file, parsed.diagnostics);
    lines.extend(f(&file, &parsed.workflow));
    lines
}

/// Parses the strategy lines `strategy` with [`workflow`], and returns the
/// diagnostics followed by the strategy as [`strategy_lines`] prints it.
fn check(strategy: &str) -> Vec<String> {
    parse_text(&workflow(strategy), |file, workflow| {
        let job = &workflow.jobs.value().unwrap().get("test").unwrap().value;
        strategy_lines(file, &job.strategy)
    })
}

/// Returns `line:column` of the start of `span`.
fn at(file: &SourceFile, span: Span) -> String {
    let pos = file.line_col(span.start);
    format!("{}:{}", pos.line, pos.column)
}

fn key_at(file: &SourceFile, key: Option<&Str<'_>>) -> String {
    key.map_or_else(|| "-".to_owned(), |key| at(file, key.span()))
}

/// Prints `strategy` as one line per field that is written and one per
/// matrix row, `include`, and `exclude`.
fn strategy_lines(file: &SourceFile, strategy: &Field<'_, Strategy<'_>>) -> Vec<String> {
    let Field::Valid { value, key, span } = strategy else {
        return vec![format!("strategy {}", field_state(strategy))];
    };
    let mut lines = vec![format!(
        "strategy key {} value {}",
        key_at(file, key.as_ref()),
        at(file, *span)
    )];
    if value.fail_fast.is_present() {
        lines.push(format!("fail-fast {}", scalar_value(&value.fail_fast)));
    }
    if value.max_parallel.is_present() {
        lines.push(format!(
            "max-parallel {}",
            scalar_value(&value.max_parallel)
        ));
    }
    match &value.matrix {
        Value::Absent => {}
        Value::Literal { key, span, value } => {
            lines.push(format!(
                "matrix key {} value {}",
                key_at(file, key.as_ref()),
                at(file, *span)
            ));
            lines.extend(matrix_lines(file, value));
        }
        Value::Expression { key, expr } => lines.push(format!(
            "matrix expression {:?} key {} value {}",
            expr.source.value(),
            key_at(file, key.as_ref()),
            at(file, expr.span())
        )),
        Value::Invalid { key, span } => lines.push(format!(
            "matrix invalid key {} value {}",
            key_at(file, key.as_ref()),
            at(file, *span)
        )),
    }
    lines
}

fn field_state<T>(field: &Field<'_, T>) -> &'static str {
    match field {
        Field::Absent => "absent",
        Field::Invalid { .. } => "invalid",
        Field::Valid { .. } => "valid",
    }
}

fn scalar_value<T: std::fmt::Debug>(value: &Value<'_, T>) -> String {
    match value {
        Value::Literal { value, .. } => format!("literal {value:?}"),
        Value::Expression { expr, .. } => format!("expression {:?}", expr.source.value()),
        Value::Invalid { .. } => "invalid".to_owned(),
        Value::Absent => "absent".to_owned(),
    }
}

fn matrix_lines(file: &SourceFile, matrix: &Matrix<'_>) -> Vec<String> {
    let mut lines = Vec::new();
    for entry in &matrix.rows {
        let row = match &entry.value {
            MatrixRow::Values(values) => {
                let values: Vec<_> = values.iter().map(|value| raw(file, value)).collect();
                values.join(", ")
            }
            MatrixRow::Expression(expr) => format!("expression {:?}", expr.source.value()),
            MatrixRow::Invalid => "invalid".to_owned(),
        };
        lines.push(format!(
            "row {} at {}: {row}",
            entry.key.value(),
            at(file, entry.key.span())
        ));
    }
    for (name, value) in [("include", &matrix.include), ("exclude", &matrix.exclude)] {
        match value {
            Value::Absent => {}
            Value::Literal { key, value, .. } => {
                lines.push(format!("{name} key {}", key_at(file, key.as_ref())));
                for combination in value.iter() {
                    lines.push(format!("  {}", combination_line(file, combination)));
                }
            }
            Value::Expression { key, expr } => lines.push(format!(
                "{name} expression {:?} key {}",
                expr.source.value(),
                key_at(file, key.as_ref())
            )),
            Value::Invalid { .. } => lines.push(format!("{name} invalid")),
        }
    }
    lines
}

fn combination_line(file: &SourceFile, combination: &MatrixCombination<'_>) -> String {
    match combination {
        MatrixCombination::Assigns { span, assigns } => {
            format!("assigns at {}: {}", at(file, *span), keyed(file, assigns))
        }
        MatrixCombination::Expression(expr) => format!(
            "expression {:?} at {}",
            expr.source.value(),
            at(file, expr.span())
        ),
        MatrixCombination::Invalid(span) => format!("invalid at {}", at(file, *span)),
    }
}

fn keyed(file: &SourceFile, entries: &Keyed<'_, RawValue<'_>>) -> String {
    let entries: Vec<_> = entries
        .iter()
        .map(|entry| format!("{}: {}", entry.key.value(), raw(file, &entry.value)))
        .collect();
    format!("{{{}}}", entries.join(", "))
}

/// Prints a raw value with the tag and style of each scalar, as
/// `!!int plain "1"`, and the position of each collection.
fn raw(file: &SourceFile, value: &RawValue<'_>) -> String {
    match value {
        RawValue::Scalar(scalar) => {
            let style = match scalar.style() {
                ScalarStyle::Plain => "plain",
                ScalarStyle::SingleQuoted => "single",
                ScalarStyle::DoubleQuoted => "double",
                ScalarStyle::Literal(_) => "literal",
                ScalarStyle::Folded(_) => "folded",
            };
            format!("{} {style} {:?}", scalar.tag().as_str(), scalar.value())
        }
        RawValue::Sequence { span, items } => {
            let items: Vec<_> = items.iter().map(|item| raw(file, item)).collect();
            format!("[{}]@{}", items.join(", "), at(file, *span))
        }
        RawValue::Mapping { span, entries } => {
            format!("{}@{}", keyed(file, entries), at(file, *span))
        }
    }
}

#[test]
fn a_strategy_has_matrix_fail_fast_and_max_parallel() {
    let lines =
        check("      fail-fast: false\n      max-parallel: 3\n      matrix:\n        os: [a]\n");
    assert_eq!(
        lines,
        [
            "strategy key 6:5 value 7:7",
            "fail-fast literal false",
            "max-parallel literal 3",
            "matrix key 9:7 value 10:9",
            r#"row os at 10:9: !!str plain "a""#,
        ]
    );
}

#[test]
fn strategy_keys_are_case_sensitive_and_listed_when_unexpected() {
    assert_eq!(
        check("      Matrix: {a: [b]}\n      fail-fast: true\n      fail-fast: false\n"),
        [
            r#"7:7: unexpected key "Matrix" for "strategy" section. expected one of "fail-fast", "matrix", "max-parallel" [unexpected-key]"#,
            r#"9:7: key "fail-fast" is duplicated in "strategy" section. previously defined at line:8,col:7 [duplicate-key]"#,
            "strategy key 6:5 value 7:7",
            "fail-fast literal true",
        ]
    );
}

#[test]
fn a_strategy_that_is_not_a_mapping_with_entries_is_invalid() {
    let text = |value: &str| {
        format!(
            "on: push\njobs:\n  test:\n    runs-on: x\n    steps: [{{run: a}}]\n    strategy:{value}\n"
        )
    };
    let cases = [
        (
            "",
            r#"6:14: "strategy" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
        ),
        (
            " {}",
            r#"6:15: "strategy" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
        ),
        (
            " [a]",
            r#"6:15: "strategy" section is sequence node but mapping node is expected [unexpected-node-kind]"#,
        ),
    ];
    for (value, want) in cases {
        let lines = parse_text(&text(value), |file, workflow| {
            let job = &workflow.jobs.value().unwrap().get("test").unwrap().value;
            strategy_lines(file, &job.strategy)
        });
        let invalid = if value == " {}" {
            "strategy key 6:5 value 6:15"
        } else {
            "strategy invalid"
        };
        assert_eq!(lines, [want, invalid], "{value:?}");
    }
}

#[test]
fn a_reusable_workflow_call_may_have_a_strategy() {
    let text = "\
on: push
jobs:
  call:
    uses: ./.github/workflows/x.yml
    strategy:
      matrix:
        os: [a, b]
";
    let lines = parse_text(text, |_, workflow| {
        let job = &workflow.jobs.value().unwrap().get("call").unwrap().value;
        vec![format!(
            "call {} strategy {}",
            job.workflow_call.is_some(),
            field_state(&job.strategy)
        )]
    });
    assert_eq!(lines, ["call true strategy valid"]);
}

#[test]
fn max_parallel_is_an_integer_greater_than_zero_or_an_expression() {
    let cases: [(&str, &[&str]); 6] = [
        ("1", &["max-parallel literal 1"]),
        (
            "${{ inputs.n }}",
            &[r#"max-parallel expression "${{ inputs.n }}""#],
        ),
        (
            "0",
            &[
                r#"7:21: value at "max-parallel" must be greater than zero: 0 [non-positive-number]"#,
                "max-parallel literal 0",
            ],
        ),
        (
            "-1",
            &[
                r#"7:21: value at "max-parallel" must be greater than zero: -1 [non-positive-number]"#,
                "max-parallel literal -1",
            ],
        ),
        (
            "1.5",
            &[
                r#"7:21: expected scalar node for integer value but found scalar node with "!!float" tag [invalid-literal-value]"#,
                "max-parallel invalid",
            ],
        ),
        (
            "'3'",
            &[
                "7:21: expecting a single ${{...}} expression or integer literal, but found plain text node [expected-single-expression]",
                "max-parallel invalid",
            ],
        ),
    ];
    for (value, want) in cases {
        let mut lines = check(&format!("      max-parallel: {value}\n"));
        lines.retain(|line| !line.starts_with("strategy key"));
        assert_eq!(lines, want, "{value:?}");
    }
}

#[test]
fn fail_fast_is_a_bool_or_an_expression() {
    let mut lines = check("      fail-fast: yes\n");
    lines.retain(|line| !line.starts_with("strategy key"));
    assert_eq!(
        lines,
        [
            r#"7:18: expecting a single ${{...}} expression or boolean literal "true" or "false", but found plain text node [expected-single-expression]"#,
            "fail-fast invalid",
        ]
    );
}

/// Parses `matrix`, the lines of a `matrix:` mapping indented by eight
/// spaces, and returns the diagnostics and the matrix lines.
fn check_matrix(matrix: &str) -> Vec<String> {
    let mut lines = check(&format!("      matrix:\n{matrix}"));
    lines.retain(|line| !line.starts_with("strategy key"));
    lines
}

#[test]
fn a_whole_matrix_expression_keeps_the_key_and_the_value_positions() {
    assert_eq!(
        check("      matrix: ${{ fromJSON(inputs.m) }}\n"),
        [
            "strategy key 6:5 value 7:7",
            r#"matrix expression "${{ fromJSON(inputs.m) }}" key 7:7 value 7:15"#,
        ]
    );
}

#[test]
fn a_matrix_that_is_neither_a_mapping_nor_an_expression_is_invalid() {
    let cases = [
        (
            " x",
            "7:15: expecting a single ${{...}} expression or matrix, but found plain text node [expected-single-expression]",
            "matrix invalid key 7:7 value 7:15",
        ),
        (
            "",
            "7:14: expecting a single ${{...}} expression or matrix, but found plain text node [expected-single-expression]",
            "matrix invalid key 7:7 value 7:14",
        ),
        (
            " [a]",
            r#"7:15: "matrix" section is sequence node but mapping node is expected [unexpected-node-kind]"#,
            "matrix invalid key 7:7 value 7:15",
        ),
        (
            " {}",
            r#"7:15: "matrix" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
            "matrix key 7:7 value 7:15",
        ),
    ];
    for (value, error, matrix) in cases {
        let mut lines = check(&format!("      matrix:{value}\n"));
        lines.retain(|line| !line.starts_with("strategy key"));
        assert_eq!(lines, [error, matrix], "{value:?}");
    }
}

#[test]
fn matrix_rows_are_case_insensitive_and_keep_source_order() {
    assert_eq!(
        check_matrix("        zeta: [1]\n        OS: [a]\n        os: [b]\n        alpha: [2]\n"),
        [
            r#"10:9: key "os" is duplicated in "matrix" section. previously defined at line:9,col:9. note that this key is case insensitive [duplicate-key]"#,
            "matrix key 7:7 value 8:9",
            r#"row zeta at 8:9: !!int plain "1""#,
            r#"row OS at 9:9: !!str plain "a""#,
            r#"row alpha at 11:9: !!int plain "2""#,
        ]
    );
}

#[test]
fn a_matrix_row_is_a_sequence_or_an_expression() {
    assert_eq!(
        check_matrix(
            "        a: [x, y]\n        b: ${{ fromJSON(inputs.b) }}\n        c: '${{ inputs.c }}'\n"
        ),
        [
            "matrix key 7:7 value 8:9",
            r#"row a at 8:9: !!str plain "x", !!str plain "y""#,
            r#"row b at 9:9: expression "${{ fromJSON(inputs.b) }}""#,
            r#"row c at 10:9: expression "${{ inputs.c }}""#,
        ]
    );
}

#[test]
fn a_scalar_row_that_is_not_an_expression_is_kept_as_invalid() {
    // As in the reference, which keeps the row with no values.
    assert_eq!(
        check_matrix("        a: x\n        b:\n"),
        [
            "8:12: expecting a single ${{...}} expression or array value for matrix variations, but found plain text node [expected-single-expression]",
            "9:11: expecting a single ${{...}} expression or array value for matrix variations, but found plain text node [expected-single-expression]",
            "matrix key 7:7 value 8:9",
            "row a at 8:9: invalid",
            "row b at 9:9: invalid",
        ]
    );
}

#[test]
fn a_row_that_is_not_a_sequence_with_items_is_left_out() {
    // As in the reference, which does not keep the row.
    assert_eq!(
        check_matrix("        a: []\n        b: {x: y}\n        c: [z]\n"),
        [
            r#"8:12: "matrix values" section should not be empty [empty-section]"#,
            r#"9:12: "matrix values" section must be sequence node but got mapping node with "!!map" tag [unexpected-node-kind]"#,
            "matrix key 7:7 value 8:9",
            r#"row c at 10:9: !!str plain "z""#,
        ]
    );
}

#[test]
fn a_row_with_a_key_that_is_not_a_scalar_is_parsed_and_left_out() {
    assert_eq!(
        check_matrix("        ? [k]\n        : []\n        a: [z]\n"),
        [
            r#"8:11: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
            r#"9:11: "matrix values" section should not be empty [empty-section]"#,
            "matrix key 7:7 value 8:9",
            r#"row a at 10:9: !!str plain "z""#,
        ]
    );
}

#[test]
fn raw_values_keep_the_tag_and_style_of_each_scalar() {
    let matrix = "        v:
          - 1
          - 1.5
          - true
          - null
          - ~
          - '1'
          - \"true\"
          - !!str 3
          - !custom x
          - |
            block
          - >-
            folded
          -
";
    assert_eq!(
        check_matrix(matrix),
        [
            "matrix key 7:7 value 8:9",
            concat!(
                r#"row v at 8:9: !!int plain "1", !!float plain "1.5", !!bool plain "true", "#,
                r#"!!null plain "null", !!null plain "~", !!str single "1", !!str double "true", "#,
                r#"!!str plain "3", !custom plain "x", !!str literal "block\n", "#,
                r#"!!str folded "folded", !!null plain """#
            ),
        ]
    );
}

#[test]
fn raw_values_nest_sequences_and_case_insensitive_mappings() {
    let matrix = "        v:
          - [a, [b], []]
          - {Name: x, Sub: {k: [1]}, empty: {}}
          - Name: y
            NAME: z
";
    assert_eq!(
        check_matrix(matrix),
        [
            r#"12:13: key "NAME" is duplicated in matrix row value. previously defined at line:11,col:13. note that this key is case insensitive [duplicate-key]"#,
            "matrix key 7:7 value 8:9",
            concat!(
                r#"row v at 8:9: [!!str plain "a", [!!str plain "b"]@9:17, []@9:22]@9:13, "#,
                r#"{Name: !!str plain "x", Sub: {k: [!!int plain "1"]@10:32}@10:28, empty: {}@10:45}@10:13, "#,
                r#"{Name: !!str plain "y"}@11:13"#
            ),
        ]
    );
}

#[test]
fn raw_mapping_keys_are_looked_up_case_insensitively() {
    let text = workflow("      matrix:\n        v: [{Name: x}]\n");
    parse_text(&text, |_, workflow| {
        let job = &workflow.jobs.value().unwrap().get("test").unwrap().value;
        let matrix = job.strategy.value().unwrap().matrix.literal().unwrap();
        let MatrixRow::Values(values) = &matrix.rows.get("V").unwrap().value else {
            panic!("not values");
        };
        let RawValue::Mapping { entries, .. } = &values[0] else {
            panic!("not a mapping");
        };
        assert_eq!(entries.get("name").unwrap().key.value(), "Name");
        Vec::new()
    });
}

#[test]
fn a_merge_key_in_a_raw_mapping_is_reported() {
    assert_eq!(
        check_matrix("        v:\n          - <<: {a: b}\n            c: d\n"),
        [
            r#"9:13: GitHub Actions does not support YAML merge key "<<" [merge-key-unsupported]"#,
            "matrix key 7:7 value 8:9",
            r#"row v at 8:9: {c: !!str plain "d"}@9:13"#,
        ]
    );
}

#[test]
fn a_recursive_alias_in_a_raw_value_is_reported_and_left_out() {
    let text = "\
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        include: &recursive_include
          - os: ubuntu-latest
            nested: *recursive_include
        row: &row [a, *row]
    steps:
      - run: echo
";
    let lines = parse_text(text, |file, workflow| {
        let job = &workflow.jobs.value().unwrap().get("test").unwrap().value;
        strategy_lines(file, &job.strategy)
    });
    assert_eq!(
        lines,
        [
            "9:21: unexpected alias node on parsing value in matrix row [unexpected-node-kind]",
            "10:23: unexpected alias node on parsing value in matrix row [unexpected-node-kind]",
            "strategy key 5:5 value 6:7",
            "matrix key 6:7 value 7:9",
            r#"row row at 10:9: !!str plain "a""#,
            "include key 7:9",
            r#"  assigns at 8:13: {os: !!str plain "ubuntu-latest"}"#,
        ]
    );
}

#[test]
fn include_and_exclude_take_mappings_and_expressions() {
    let matrix = "        os: [a]
        Include:
          - os: b
            Extra: 1
          - ${{ fromJSON(inputs.include) }}
        exclude: ${{ fromJSON(inputs.exclude) }}
";
    assert_eq!(
        check_matrix(matrix),
        [
            "matrix key 7:7 value 8:9",
            r#"row os at 8:9: !!str plain "a""#,
            "include key 9:9",
            r#"  assigns at 10:13: {os: !!str plain "b", Extra: !!int plain "1"}"#,
            r#"  expression "${{ fromJSON(inputs.include) }}" at 12:13"#,
            r#"exclude expression "${{ fromJSON(inputs.exclude) }}" key 13:9"#,
        ]
    );
}

#[test]
fn combination_keys_are_case_insensitive() {
    let text = workflow("      matrix:\n        include:\n          - OS: a\n            os: b\n");
    let lines = parse_text(&text, |file, workflow| {
        let job = &workflow.jobs.value().unwrap().get("test").unwrap().value;
        let matrix = job.strategy.value().unwrap().matrix.literal().unwrap();
        let Value::Literal { value, .. } = &matrix.include else {
            panic!("no include");
        };
        let MatrixCombination::Assigns { assigns, .. } = &value[0] else {
            panic!("not assigns");
        };
        vec![
            assigns.get("os").unwrap().key.value().to_string(),
            combination_line(file, &value[0]),
        ]
    });
    assert_eq!(
        lines,
        [
            r#"10:13: key "os" is duplicated in element in "include" section. previously defined at line:9,col:13. note that this key is case insensitive [duplicate-key]"#,
            "OS",
            r#"assigns at 9:13: {OS: !!str plain "a"}"#,
        ]
    );
}

#[test]
fn invalid_include_and_exclude_are_reported() {
    let cases: [(&str, &[&str]); 4] = [
        (
            "include: x",
            &[
                "8:18: expecting a single ${{...}} expression or array of matrix combination, but found plain text node [expected-single-expression]",
                "include invalid",
            ],
        ),
        (
            "exclude: {os: a}",
            &[
                r#"8:18: "exclude" section must be sequence node but got mapping node with "!!map" tag [unexpected-node-kind]"#,
                "exclude invalid",
            ],
        ),
        (
            "exclude: []",
            &[
                r#"8:18: "exclude" section should not be empty [empty-section]"#,
                "exclude invalid",
            ],
        ),
        (
            "include:",
            &[
                "8:17: expecting a single ${{...}} expression or array of matrix combination, but found plain text node [expected-single-expression]",
                "include invalid",
            ],
        ),
    ];
    for (line, want) in cases {
        let mut lines = check_matrix(&format!("        {line}\n"));
        lines.retain(|line| line != "matrix key 7:7 value 8:9");
        assert_eq!(lines, want, "{line:?}");
    }
}

#[test]
fn invalid_combinations_are_reported() {
    // A scalar that is not an expression is left out, and a collection that
    // is not a mapping is kept as invalid, as in the reference.
    let matrix = "        include:
          - x
          - [os, a]
          - {}
          - &a {os: *a}
";
    assert_eq!(
        check_matrix(matrix),
        [
            "9:13: expecting a single ${{...}} expression or mapping of matrix combination, but found plain text node [expected-single-expression]",
            r#"10:13: element in "include" section is sequence node but mapping node is expected [unexpected-node-kind]"#,
            r#"11:13: element in "include" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
            "12:21: unexpected alias node on parsing value in matrix row [unexpected-node-kind]",
            "matrix key 7:7 value 8:9",
            "include key 8:9",
            "  invalid at 10:13",
            "  assigns at 11:13: {}",
            "  assigns at 12:13: {}",
        ]
    );
}

#[test]
fn deeply_nested_raw_values_do_not_overflow_the_stack() {
    // The YAML layer accepts 10,000 levels of flow nesting.
    let depth = 9_990;
    let mut text = workflow("      matrix:\n        v: [");
    text.push_str(&"[".repeat(depth));
    text.push_str(&"]".repeat(depth));
    let _ = writeln!(text, "]");
    let lines = parse_text(&text, |_, workflow| {
        let job = &workflow.jobs.value().unwrap().get("test").unwrap().value;
        let matrix = job.strategy.value().unwrap().matrix.literal().unwrap();
        let MatrixRow::Values(values) = &matrix.rows.get("v").unwrap().value else {
            panic!("not values");
        };
        let mut levels = 0;
        let mut value = values.first();
        while let Some(RawValue::Sequence { items, .. }) = value {
            levels += 1;
            value = items.first();
        }
        vec![levels.to_string()]
    });
    assert_eq!(lines, [depth.to_string()]);
}

#[test]
fn deeply_nested_raw_mappings_do_not_overflow_the_stack() {
    let depth = 9_990;
    let mut text = workflow("      matrix:\n        v: [");
    text.push_str(&"{a: ".repeat(depth));
    text.push('1');
    text.push_str(&"}".repeat(depth));
    let _ = writeln!(text, "]");
    let lines = parse_text(&text, |_, workflow| {
        let job = &workflow.jobs.value().unwrap().get("test").unwrap().value;
        let matrix = job.strategy.value().unwrap().matrix.literal().unwrap();
        let MatrixRow::Values(values) = &matrix.rows.get("v").unwrap().value else {
            panic!("not values");
        };
        let mut levels = 0;
        let mut value = values.first();
        while let Some(RawValue::Mapping { entries, .. }) = value {
            levels += 1;
            value = entries.get("A").map(|entry| &entry.value);
        }
        vec![levels.to_string()]
    });
    assert_eq!(lines, [depth.to_string()]);
}
