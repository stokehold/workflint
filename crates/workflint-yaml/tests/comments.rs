//! Comments in every position the YAML grammar allows them.
//!
//! Each test checks two snapshots. `comments__<case>` lists the comment
//! events with their marks, kinds, and text. `events__<case>` holds the other
//! events. It was recorded before the scanner reported comments, so it shows
//! that comments change no other event.

mod support;

use insta::assert_snapshot;
use support::{dump_comments, dump_events};
use workflint_yaml::event::EventData;

/// Checks the snapshots of the case `name`.
fn check(name: &str, input: &str) {
    let mut settings = insta::Settings::clone_current();
    settings.set_description(input);
    settings.set_omit_expression(true);
    settings.bind(|| {
        assert_snapshot!(format!("comments__{name}"), dump_comments(input));
        assert_snapshot!(format!("events__{name}"), dump_events(input));
    });
}

/// Checks the snapshots of the case `name`, whose input has a `#` that does
/// not start a comment, and that the parser reports no comment.
fn check_without_comments(name: &str, input: &str) {
    assert_eq!(dump_comments(input), "");
    check(name, input);
}

#[test]
fn comments_before_the_first_document() {
    check(
        "before_first_document",
        "# first\n  # indented\n\n# after an empty line\na: 1\n",
    );
}

#[test]
fn comments_between_documents() {
    check(
        "between_documents",
        "a: 1\n# between\n---\nb: 2\n---\n# after the start line\nc: 3\n",
    );
}

#[test]
fn comment_after_document_start_marker() {
    check("after_document_start", "--- # after start\na: 1\n");
}

#[test]
fn comment_after_document_start_marker_of_a_scalar_document() {
    check("after_document_start_scalar", "--- x # after the scalar\n");
}

#[test]
fn comments_after_document_end_marker() {
    check(
        "after_document_end",
        "a: 1\n... # after end\n# own line after end\n---\nb: 2\n...\n# last\n",
    );
}

#[test]
fn comments_after_directives() {
    check(
        "after_directives",
        "%YAML 1.1 # after version\n%TAG !e! tag:example.com,2000: # after tag\n# before start\n---\na: 1\n",
    );
}

#[test]
fn own_line_comment_at_end_of_file_without_newline() {
    check("own_line_at_end_of_file", "a: 1\n# last");
}

#[test]
fn trailing_comment_at_end_of_file_without_newline() {
    check("trailing_at_end_of_file", "a: 1 # last");
}

#[test]
fn comment_only_file() {
    check("comment_only_file", "# one\n# two\n");
}

#[test]
fn comment_only_file_with_byte_order_mark() {
    check(
        "comment_only_file_with_bom",
        "\u{feff}# after the BOM\na: 1\n",
    );
}

#[test]
fn trailing_comment_after_a_key() {
    check("trailing_after_key", "a: # after key\n  b: 1\n");
}

#[test]
fn trailing_comment_after_a_value() {
    check(
        "trailing_after_value",
        "a: 1 # after value\nb: x # after x\n",
    );
}

#[test]
fn trailing_comment_after_a_tab() {
    check("trailing_after_tab", "a: 1\t# after a tab\n");
}

#[test]
fn trailing_comments_after_an_anchor_and_a_tag() {
    check(
        "trailing_after_properties",
        "a: &x # after anchor\n  b: 1\nc: !!map # after tag\n  d: *x # after alias\n",
    );
}

#[test]
fn trailing_comments_after_explicit_key_and_value() {
    check("trailing_after_explicit_key", "? a # key\n: b # value\n");
}

#[test]
fn trailing_comment_after_a_literal_block_scalar_header() {
    check("after_literal_header", "run: | # note\n  echo hi\n");
}

#[test]
fn trailing_comment_after_a_folded_block_scalar_header_with_indicators() {
    check(
        "after_folded_header",
        "run: >-2 # note\n   echo hi\nnext: 1\n",
    );
}

#[test]
fn comment_after_a_block_scalar_ends() {
    check(
        "after_block_scalar",
        "a: |\n  x\n # dedented\nb: |\n  y\n\n# after an empty line\nc: 1\n",
    );
}

#[test]
fn trailing_comments_after_multi_line_scalars() {
    check(
        "after_multi_line_scalars",
        "a: \"x\n  y\" # after quoted\nb: x\n  y # after plain\nc: 1\n",
    );
}

#[test]
fn own_line_comment_indented_after_a_plain_scalar() {
    check("indented_after_plain_scalar", "a: b\n   # indented\nc: d\n");
}

#[test]
fn comments_inside_flow_sequence() {
    check(
        "flow_sequence",
        "[a, # after a\n  # own line\n  b # after b\n  , c] # after the sequence\n",
    );
}

#[test]
fn comments_inside_flow_mapping() {
    check(
        "flow_mapping",
        "{a: 1, # after a\n  # own line\n  b: # after the key\n    2}\n",
    );
}

#[test]
fn comment_only_lines_in_block_mapping_at_any_indentation() {
    check(
        "block_mapping_indentation",
        "jobs:\n  # indent 2\n  build:\n        # deeper than the next key\n    runs-on: x\n# column 0 inside the nested mapping\n    steps: []\n    # after the last key\n  test: {}\n",
    );
}

#[test]
fn comment_only_lines_in_block_sequence_at_any_indentation() {
    check(
        "block_sequence_indentation",
        "steps:\n  # before the first item\n  - run: a\n    # inside the item\n    name: b\n# column 0 between items\n  - uses: c\n      # deeper than the item\n  -\n    # in an item that starts empty\n    d\n  - # after the dash\n    e\n",
    );
}

#[test]
fn comment_after_a_dash_before_a_nested_sequence() {
    check("after_dash_before_sequence", "- # after the dash\n  - a\n");
}

#[test]
fn comments_with_crlf_line_breaks() {
    check("crlf", "a: 1 # after\r\n# own line\r\nb: 2\r\n");
}

#[test]
fn comment_text_with_hashes_and_non_ascii_characters() {
    check("non_ascii_text", "a: 1 # ø # 日本\n# ü\n");
}

#[test]
fn hash_inside_literal_block_scalar_is_not_a_comment() {
    check_without_comments(
        "hash_in_literal_block_scalar",
        "run: |\n  # shell comment\n  echo hi # shell trailing\n",
    );
}

#[test]
fn hash_inside_folded_block_scalar_is_not_a_comment() {
    check_without_comments("hash_in_folded_block_scalar", "run: >\n  # not a comment\n");
}

#[test]
fn hash_inside_quoted_scalars_is_not_a_comment() {
    check_without_comments(
        "hash_in_quoted_scalars",
        "a: 'x # y'\nb: \"x # y\"\nc: \"x\n  # y\"\n",
    );
}

#[test]
fn hash_in_plain_scalar_without_preceding_whitespace_is_not_a_comment() {
    check_without_comments(
        "hash_in_plain_scalar",
        "a: a#b\nc#d: e\nurl: https://example.com/#frag\nf: [g#h]\n",
    );
}

#[test]
fn comment_events_come_before_the_first_event_that_starts_after_them() {
    let kinds: Vec<String> = workflint_yaml::event::EventParser::new("a: # c\n  b: 1 # d\n# e\n")
        .map(|event| {
            let event = event.map_err(|err| err.to_string());
            match event.map(|event| event.data) {
                Ok(EventData::Scalar { value, .. }) => format!("Scalar {value}"),
                Ok(EventData::Comment { kind }) => format!("Comment {kind:?}"),
                Ok(data) => format!("{data:?}")
                    .split([' ', '{'])
                    .next()
                    .unwrap_or_default()
                    .to_owned(),
                Err(err) => err,
            }
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "StreamStart",
            "DocumentStart",
            "MappingStart",
            "Scalar a",
            "Comment Trailing",
            "MappingStart",
            "Scalar b",
            "Scalar 1",
            "Comment Trailing",
            "Comment OwnLine",
            "MappingEnd",
            "MappingEnd",
            "DocumentEnd",
            "StreamEnd",
        ]
    );
}
