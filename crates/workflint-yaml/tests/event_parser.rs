//! Behaviour of the event parser that workflint relies on or fixed in the
//! vendored copy. Expected values come from go-yaml v4.0.0-rc.3, the YAML
//! library of the reference.

#![allow(clippy::unwrap_used)]

use workflint_yaml::{ErrorKind, EventData, EventParser, Mark, ScalarStyle};

struct Scalar {
    value: String,
    style: ScalarStyle,
    start: Mark,
}

/// Parses `input` and returns its scalars in order, or the first error.
fn scalars(input: &str) -> Result<Vec<Scalar>, workflint_yaml::Error> {
    let mut scalars = Vec::new();
    for event in EventParser::new(input) {
        let event = event?;
        if let EventData::Scalar { value, style, .. } = event.data {
            scalars.push(Scalar {
                value,
                style,
                start: event.start_mark,
            });
        }
    }
    Ok(scalars)
}

/// Parses `input`, which must fail, and returns the error.
fn parse_error(input: &str) -> workflint_yaml::Error {
    scalars(input).err().unwrap()
}

/// Returns the value and style of the scalar after the key `a`.
fn value_of_a(input: &str) -> (String, ScalarStyle) {
    let scalars = scalars(input).unwrap();
    assert_eq!(scalars.len(), 2, "expected one key and one value");
    let value = &scalars[1];
    (value.value.clone(), value.style)
}

#[test]
fn literal_block_scalar_at_end_of_file_without_newline() {
    let scalars = scalars("a: |\n  x").unwrap();
    let value = &scalars[1];
    assert_eq!(value.value, "x");
    assert_eq!(value.style, ScalarStyle::Literal);
    assert_eq!((value.start.line, value.start.column), (0, 3));
    assert_eq!(value.start.index, 3);
}

#[test]
fn strip_chomping_block_scalar_at_end_of_file_without_newline() {
    assert_eq!(
        value_of_a("a: |-\n  x"),
        ("x".to_owned(), ScalarStyle::Literal)
    );
}

#[test]
fn keep_chomping_block_scalar_at_end_of_file_without_newline() {
    assert_eq!(
        value_of_a("a: |+\n  x"),
        ("x".to_owned(), ScalarStyle::Literal)
    );
}

#[test]
fn folded_block_scalar_at_end_of_file_without_newline() {
    assert_eq!(
        value_of_a("a: >\n  x"),
        ("x".to_owned(), ScalarStyle::Folded)
    );
}

#[test]
fn literal_block_scalar_with_crlf_at_end_of_file_without_newline() {
    assert_eq!(
        value_of_a("a: |\r\n  x"),
        ("x".to_owned(), ScalarStyle::Literal)
    );
}

#[test]
fn literal_block_scalar_with_crlf_keeps_one_newline() {
    assert_eq!(
        value_of_a("a: |\r\n  x\r\n"),
        ("x\n".to_owned(), ScalarStyle::Literal)
    );
}

#[test]
fn literal_block_scalar_followed_by_comment_line_at_end_of_file() {
    assert_eq!(
        value_of_a("a: |\n  x\n# comment"),
        ("x\n".to_owned(), ScalarStyle::Literal)
    );
}

#[test]
fn multi_line_literal_block_scalar_at_end_of_file_without_newline() {
    assert_eq!(
        value_of_a("a: |\n  x\n  y"),
        ("x\ny".to_owned(), ScalarStyle::Literal)
    );
}

#[test]
fn keep_chomping_block_scalar_keeps_trailing_empty_lines() {
    assert_eq!(
        value_of_a("a: |+\n  x\n\n"),
        ("x\n\n".to_owned(), ScalarStyle::Literal)
    );
}

#[test]
fn tag_followed_by_comma_in_flow_sequence_ends_at_the_comma() {
    let values: Vec<_> = scalars("[!!str, x]")
        .unwrap()
        .into_iter()
        .map(|scalar| scalar.value)
        .collect();
    assert_eq!(values, ["", "x"]);
}

#[test]
fn backslash_at_end_of_file_in_double_quoted_scalar_is_an_unknown_escape() {
    let err = parse_error("a: \"x\\");
    assert_eq!(err.kind(), ErrorKind::Scanner);
    assert_eq!(err.problem(), "found unknown escape character");
}

#[test]
fn byte_order_mark_counts_in_the_byte_index_but_not_the_column() {
    let scalars = scalars("\u{feff}a: b").unwrap();
    let key = &scalars[0];
    assert_eq!(key.value, "a");
    assert_eq!((key.start.line, key.start.column), (0, 0));
    assert_eq!(key.start.index, 3);
    let value = &scalars[1];
    assert_eq!((value.start.line, value.start.column), (0, 3));
    assert_eq!(value.start.index, 6);
}

#[test]
fn control_character_is_a_reader_error() {
    let err = parse_error("a: \u{1}");
    assert_eq!(err.kind(), ErrorKind::Reader);
    assert_eq!(err.problem(), "control characters are not allowed");
}

// Ported from upstream `tests/test_mark_display.rs`.

#[test]
fn error_at_the_first_character_displays_as_line_1_column_1() {
    let err = parse_error("\t");
    assert_eq!(err.problem_mark().unwrap().to_string(), "line 1 column 1");
}

#[test]
fn error_marks_display_1_based_lines_and_columns() {
    let err = parse_error("---\nkey: \"missing closing quote");
    assert_eq!(err.problem_mark().unwrap().to_string(), "line 2 column 28");
}
