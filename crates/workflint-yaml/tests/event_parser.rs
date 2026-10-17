//! Behaviour of the event parser that workflint relies on or fixed in the
//! vendored copy. Expected values come from go-yaml v4.0.0-rc.3, the YAML
//! library of the reference, except in the tests that name an entry of
//! `docs/differences.md`: `tag-before-flow-comma`,
//! `yaml-solidus-escape`, and `yaml-1-2-directive`.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::time::{Duration, Instant};

use workflint_diagnostics::{FileId, SourceFile};
use workflint_yaml::event::{Error, ErrorKind, Event, EventData, EventParser, Mark, ScalarStyle};
use workflint_yaml::{NodeKind, parse};

/// Parses `input` up to the end of the stream or the first error, and
/// returns the events other than comments and the error.
fn run(input: &str) -> (Vec<Event>, Option<Error>) {
    let mut events = Vec::new();
    for event in EventParser::new(input) {
        match event {
            Ok(Event {
                data: EventData::Comment { .. },
                ..
            }) => {}
            Ok(event) => events.push(event),
            Err(err) => return (events, Some(err)),
        }
    }
    (events, None)
}

/// The 0-based line and column of a mark.
fn at(mark: Mark) -> (u64, u64) {
    (mark.line, mark.column)
}

/// Checks the problem, the problem position, and the context of an error.
#[track_caller]
fn assert_error(
    err: &Error,
    problem: &str,
    problem_at: (u64, u64),
    context: Option<(&str, (u64, u64))>,
) {
    assert_eq!(err.problem(), problem);
    assert_eq!(err.problem_mark().map(at), Some(problem_at), "problem mark");
    assert_eq!(
        err.context().zip(err.context_mark().map(at)),
        context,
        "context"
    );
}

struct Scalar {
    value: String,
    style: ScalarStyle,
    start: Mark,
}

/// Parses `input` and returns its scalars in order, or the first error.
fn scalars(input: &str) -> Result<Vec<Scalar>, workflint_yaml::event::Error> {
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
fn parse_error(input: &str) -> workflint_yaml::event::Error {
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

// docs/differences.md#tag-before-flow-comma
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

// Nesting depth. go-yaml stops at 10,000 flow levels or block indents.

#[test]
fn ten_thousand_flow_levels_parse() {
    let input = format!("{}{}", "[".repeat(10_000), "]".repeat(10_000));
    let (events, err) = run(&input);
    assert!(err.is_none(), "{err:?}");
    let starts = events
        .iter()
        .filter(|event| matches!(event.data, EventData::SequenceStart { .. }))
        .count();
    assert_eq!(starts, 10_000);
}

#[test]
fn flow_level_beyond_ten_thousand_exceeds_the_max_depth() {
    let input = format!("{}{}", "[".repeat(10_001), "]".repeat(10_001));
    let (_, err) = run(&input);
    let err = err.unwrap();
    assert_eq!(err.kind(), ErrorKind::Scanner);
    assert_error(
        &err,
        "exceeded max depth of 10000",
        (0, 10_000),
        Some(("while increasing flow level", (0, 10_000))),
    );
}

#[test]
fn ten_thousand_block_indents_parse() {
    let input = format!("{}x\n", "- ".repeat(10_000));
    let (_, err) = run(&input);
    assert!(err.is_none(), "{err:?}");
}

#[test]
fn block_indent_beyond_ten_thousand_exceeds_the_max_depth() {
    let input = format!("{}x\n", "- ".repeat(10_001));
    let (_, err) = run(&input);
    // go-yaml takes the context mark from the simple key of the current
    // flow level, which no token has set here.
    assert_error(
        &err.unwrap(),
        "exceeded max depth of 10000",
        (0, 20_000),
        Some(("while increasing indent level", (0, 0))),
    );
}

#[test]
fn deeply_nested_flow_sequences_parse_in_well_under_a_second() {
    let input = format!("{}{}", "[".repeat(10_000), "]".repeat(10_000));
    let file = SourceFile::new(FileId::new(0), "deep.yaml", &input).unwrap();
    let start = Instant::now();
    let parsed = parse(&file);
    let elapsed = start.elapsed();
    assert!(parsed.document.is_some());
    // One second is the bound in a release build, which CI checks. The debug
    // bound is loose so that a slow machine passes. A quadratic scanner takes
    // minutes here.
    let limit = if cfg!(debug_assertions) {
        Duration::from_secs(5)
    } else {
        Duration::from_secs(1)
    };
    assert!(elapsed < limit, "took {elapsed:?}");
}

// Anchor and alias names.

#[test]
fn anchor_names_take_any_printable_ascii_except_flow_indicators_and_colon() {
    for name in ["a.b", "job/defaults", "$v", "x+y", "a!", "@"] {
        let input = format!("x: &{name} 1\ny: *{name}\n");
        let (events, err) = run(&input);
        assert!(err.is_none(), "&{name}: {err:?}");
        let anchors: Vec<_> = events
            .iter()
            .filter_map(|event| match &event.data {
                EventData::Scalar { anchor, .. } => anchor.clone(),
                EventData::Alias { anchor } => Some(anchor.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(anchors, [name, name]);
    }
}

#[test]
fn anchor_name_with_a_non_ascii_character_is_an_error() {
    let (_, err) = run("a: &\u{e9} 1\n");
    assert_error(
        &err.unwrap(),
        "did not find expected alphabetic or numeric character",
        (0, 4),
        Some(("while scanning an anchor", (0, 3))),
    );
}

// The simple key limit counts characters.

#[test]
fn simple_key_of_600_two_byte_characters_is_a_key() {
    let key = "\u{e9}".repeat(600);
    let scalars = scalars(&format!("{key}: 1\n")).unwrap();
    assert_eq!(scalars[0].value, key);
    assert_eq!(scalars[1].value, "1");
}

#[test]
fn simple_key_of_1024_characters_is_a_key() {
    let key = "a".repeat(1024);
    let scalars = scalars(&format!("{key}: 1\n")).unwrap();
    assert_eq!(scalars[0].value, key);
}

#[test]
fn simple_key_of_1025_characters_is_not_a_key() {
    let (_, err) = run(&format!("{}: 1\n", "\u{e9}".repeat(1025)));
    assert_error(
        &err.unwrap(),
        "mapping values are not allowed in this context",
        (0, 1025),
        None,
    );
}

// Comments and tabs.

#[test]
fn tab_before_an_own_line_comment_after_a_comment_line_is_allowed() {
    let scalars = scalars("# c\n\t# d\na: 1\n").unwrap();
    assert_eq!(at(scalars[0].start), (2, 0));
}

#[test]
fn tab_before_an_own_line_comment_after_an_empty_line_and_a_comment_is_allowed() {
    assert!(scalars("# c\n\n\t# d\na: 1\n").is_ok());
}

#[test]
fn tab_before_the_first_own_line_comment_is_an_error() {
    let (_, err) = run("\t# d\na: 1\n");
    assert_error(
        &err.unwrap(),
        "found character that cannot start any token",
        (0, 0),
        Some(("while scanning for the next token", (0, 0))),
    );
}

#[test]
fn tab_before_a_comment_after_a_colon_is_allowed() {
    let scalars = scalars("a:\t# c\n").unwrap();
    assert_eq!(scalars[1].value, "");
    assert_eq!(at(scalars[1].start), (0, 2));
}

#[test]
fn comments_after_a_tab_are_comment_events() {
    let comments: Vec<_> = EventParser::new("# c\n\t# d\na:\t# e\n")
        .filter_map(|event| match event.unwrap() {
            Event {
                data: EventData::Comment { .. },
                start_mark,
                end_mark,
                ..
            } => Some((start_mark.index, end_mark.index)),
            _ => None,
        })
        .collect();
    assert_eq!(comments, [(0, 3), (5, 8), (12, 15)]);
}

// Quoted scalars.

#[test]
fn escaped_single_quote_in_a_double_quoted_scalar_is_a_single_quote() {
    assert_eq!(
        value_of_a("a: \"\\'\"\n"),
        ("'".to_owned(), ScalarStyle::DoubleQuoted)
    );
}

// docs/differences.md#yaml-solidus-escape
#[test]
fn escaped_solidus_in_a_double_quoted_scalar_is_a_solidus() {
    assert_eq!(
        value_of_a("a: \"\\/\"\n"),
        ("/".to_owned(), ScalarStyle::DoubleQuoted)
    );
}

#[test]
fn bad_hex_escape_reports_a_hexadecimal_number() {
    let (_, err) = run("a: \"\\xZZ\"\n");
    assert_error(
        &err.unwrap(),
        "did not find expected hexadecimal number",
        (0, 6),
        Some(("while parsing a quoted scalar", (0, 3))),
    );
}

// Plain scalars in flow collections.

#[test]
fn colon_before_a_comma_in_a_flow_sequence_is_part_of_the_scalar() {
    let values: Vec<_> = scalars("a: [b:,c]\n")
        .unwrap()
        .into_iter()
        .map(|scalar| scalar.value)
        .collect();
    assert_eq!(values, ["a", "b:", "c"]);
}

#[test]
fn colon_before_a_brace_in_a_flow_mapping_is_part_of_the_key() {
    let scalars = scalars("a: {b:}\n").unwrap();
    let values: Vec<_> = scalars.iter().map(|scalar| scalar.value.as_str()).collect();
    assert_eq!(values, ["a", "b:", ""]);
    assert_eq!(at(scalars[2].start), (0, 6));
}

#[test]
fn question_mark_before_a_character_in_a_flow_sequence_starts_a_scalar() {
    let (events, err) = run("a: [?x]\n");
    assert!(err.is_none());
    let flow_mappings = events
        .iter()
        .filter(|event| {
            matches!(
                event.data,
                EventData::MappingStart {
                    style: workflint_yaml::event::MappingStyle::Flow,
                    ..
                }
            )
        })
        .count();
    assert_eq!(flow_mappings, 0);
    let values: Vec<_> = scalars("a: [?x]\n")
        .unwrap()
        .into_iter()
        .map(|scalar| scalar.value)
        .collect();
    assert_eq!(values, ["a", "?x"]);
}

// Byte order marks and control characters.

#[test]
fn byte_order_mark_after_the_start_of_the_stream_is_content() {
    let scalars = scalars("---\n\u{feff}a: 1\n").unwrap();
    assert_eq!(scalars[0].value, "\u{feff}a");
    assert_eq!(at(scalars[0].start), (1, 0));
}

#[test]
fn control_character_in_a_later_document_is_a_reader_error() {
    let (events, err) = run("a: 1\n---\nb: \u{1}\n");
    assert_eq!(events, []);
    assert_eq!(err.unwrap().kind(), ErrorKind::Reader);
}

#[test]
fn control_character_wins_over_an_earlier_syntax_error_in_the_same_chunk() {
    let (events, err) = run("a: b: c\nd: \u{1}");
    assert_eq!(events, []);
    let err = err.unwrap();
    assert_eq!(err.kind(), ErrorKind::Reader);
    assert_eq!(err.problem(), "control characters are not allowed");
    assert_eq!(err.problem_offset(), 11);
}

#[test]
fn control_character_in_a_later_chunk_is_found_when_the_scanner_reaches_it() {
    let input = format!("a: b: c\n{}\u{1}\n", "#".repeat(600));
    let (_, err) = run(&input);
    let err = err.unwrap();
    assert_eq!(err.kind(), ErrorKind::Scanner);
    assert_eq!(
        err.problem(),
        "mapping values are not allowed in this context"
    );

    let input = format!("a: {}\u{1}\n", "x".repeat(600));
    let (events, err) = run(&input);
    assert_eq!(err.unwrap().kind(), ErrorKind::Reader);
    assert!(
        events
            .iter()
            .any(|event| matches!(event.data, EventData::MappingStart { .. }))
    );
}

// Tags.

#[test]
fn uri_escapes_in_a_tag_decode_as_utf8() {
    let (events, err) = run("a: !<tag:%C3%A9> x\n");
    assert!(err.is_none());
    let tags: Vec<_> = events
        .iter()
        .filter_map(|event| match &event.data {
            EventData::Scalar { tag, .. } => tag.clone(),
            _ => None,
        })
        .collect();
    assert_eq!(tags, ["tag:\u{e9}"]);
}

#[test]
fn tag_outside_a_flow_collection_takes_commas_and_brackets() {
    let (events, err) = run("a: !foo,[x] y\n");
    assert!(err.is_none());
    let tags: Vec<_> = events
        .iter()
        .filter_map(|event| match &event.data {
            EventData::Scalar { tag, .. } => tag.clone(),
            _ => None,
        })
        .collect();
    assert_eq!(tags, ["!foo,[x]"]);
}

// Flow pairs.

#[test]
fn empty_value_of_a_pair_in_a_flow_sequence_starts_at_the_colon() {
    let scalars = scalars("x: [a:\n]\n").unwrap();
    assert_eq!(scalars[2].value, "");
    assert_eq!(at(scalars[2].start), (0, 5));
}

// An empty flow collection that starts a simple key is taken by the parser
// before the scanner finds the `:`. The KEY and BLOCK-MAPPING-START tokens
// then go to the end of the token queue, as in go-yaml.
#[test]
fn empty_flow_collection_with_tokens_before_its_colon_fails_in_the_parser() {
    for input in ["- [] b: 1\n", "- {} b: 1\n- c\n"] {
        let (_, err) = run(input);
        let err = err.unwrap_or_else(|| panic!("{input:?} parsed"));
        assert_eq!(err.kind(), ErrorKind::Parser, "{input:?}");
        assert_error(
            &err,
            "did not find expected '-' indicator",
            (0, 5),
            Some(("while parsing a block collection", (0, 0))),
        );
    }
}

#[test]
fn empty_flow_collection_with_tokens_before_its_colon_is_the_root() {
    for (input, sequence) in [("[] x: y\n", true), ("{} a: x\n", false)] {
        let file = SourceFile::new(FileId::new(0), "a.yaml", input).unwrap();
        let parsed = parse(&file);
        assert!(parsed.diagnostics.is_empty(), "{input:?}");
        let document = parsed.document.unwrap();
        let root = document.node(document.root().unwrap()).unwrap();
        let empty = match &root.kind {
            NodeKind::Sequence(seq) if sequence => document.items(seq).is_empty(),
            NodeKind::Mapping(map) if !sequence => document.entries(map).is_empty(),
            _ => false,
        };
        assert!(empty, "{input:?}: {:?}", root.kind);
        assert_eq!((root.span.start, root.span.end), (0, 2), "{input:?}");
    }
}

// Directives.

// docs/differences.md#yaml-1-2-directive
#[test]
fn yaml_1_2_directive_is_accepted() {
    assert_eq!(scalars("%YAML 1.2\n---\na: 1\n").unwrap().len(), 2);
}

#[test]
fn yaml_1_3_directive_is_an_incompatible_document() {
    let (_, err) = run("%YAML 1.3\n---\na\n");
    assert_error(
        &err.unwrap(),
        "found incompatible YAML document",
        (0, 0),
        None,
    );
}

#[test]
fn version_number_of_three_digits_is_extremely_long() {
    let (_, err) = run("%YAML 1.123\n---\na\n");
    assert_error(
        &err.unwrap(),
        "found extremely long version number",
        (0, 10),
        Some(("while scanning a %YAML directive", (0, 0))),
    );
}

// Lookahead. go-yaml's scanner reads at least three tokens ahead of the
// parser, so an error in those tokens comes before the event.

#[test]
fn error_in_a_lookahead_token_comes_before_the_event() {
    let (events, err) = run("a: 1\n}d:\n");
    assert_eq!(events.len(), 5, "{events:?}");
    assert_error(
        &err.unwrap(),
        "mapping values are not allowed in this context",
        (1, 2),
        None,
    );
}

#[test]
fn simple_key_without_a_colon_reports_the_next_line() {
    let (events, err) = run("x: 1\na\nb: 1\n");
    assert_eq!(events.len(), 4, "{events:?}");
    assert_error(
        &err.unwrap(),
        "could not find expected ':'",
        (2, 0),
        Some(("while scanning a simple key", (1, 0))),
    );
}

#[test]
fn errors_in_the_tokens_after_the_first_document_are_reported() {
    for (input, problem, problem_at) in [
        (
            "a: 1\n--- @\n",
            "found character that cannot start any token",
            (1, 4),
        ),
        (
            "a: 1\n...\n@\n",
            "found character that cannot start any token",
            (2, 0),
        ),
        (
            "a: 1\n---\n\tb\n",
            "found character that cannot start any token",
            (2, 0),
        ),
        ("a: 1\n--- \"x\n", "found unexpected end of stream", (2, 0)),
        (
            "a: 1\n---\n---\n@\n",
            "found character that cannot start any token",
            (3, 0),
        ),
    ] {
        let (events, err) = run(input);
        let err = err.unwrap_or_else(|| panic!("{input:?} parsed"));
        assert_eq!(err.problem(), problem, "{input:?}");
        assert_eq!(err.problem_mark().map(at), Some(problem_at), "{input:?}");
        assert!(
            !events
                .iter()
                .any(|event| matches!(event.data, EventData::DocumentEnd { .. })),
            "{input:?}: the first document ended before the error"
        );

        let file = SourceFile::new(FileId::new(0), "a.yaml", input).unwrap();
        let parsed = parse(&file);
        assert!(parsed.document.is_none(), "{input:?}");
        assert_eq!(parsed.diagnostics.len(), 1, "{input:?}");
    }
}

#[test]
fn error_past_the_lookahead_of_the_first_document_is_not_reported() {
    let input = "a: 1\n---\nb: @\n";
    let (events, err) = run(input);
    assert!(err.is_some());
    assert!(
        events
            .iter()
            .any(|event| matches!(event.data, EventData::DocumentEnd { .. }))
    );
    let file = SourceFile::new(FileId::new(0), "a.yaml", input).unwrap();
    let parsed = parse(&file);
    assert!(parsed.document.is_some());
    assert_eq!(parsed.diagnostics, []);
}

// Positions of empty scalars and block ends.

#[test]
fn empty_document_without_a_newline_has_its_scalar_on_the_next_line() {
    let scalars = scalars("---").unwrap();
    assert_eq!(scalars[0].value, "");
    assert_eq!(at(scalars[0].start), (1, 0));
}

#[test]
fn explicit_key_without_a_value_at_the_end_of_the_stream_has_its_value_on_the_next_line() {
    let scalars = scalars("? ").unwrap();
    assert_eq!(at(scalars[0].start), (0, 1));
    assert_eq!(at(scalars[1].start), (1, 0));
}

/// The 1-based line and column of the last node in the tree of `input`.
fn last_node_position(input: &str) -> (usize, usize) {
    let file = SourceFile::new(FileId::new(0), "a.yaml", input).unwrap();
    let document = parse(&file).document.unwrap();
    let (_, node) = document.nodes().last().unwrap();
    let pos = file.line_col(node.span.start);
    (pos.line, pos.column)
}

// docs/differences.md#empty-scalar-past-last-line
#[test]
fn tree_puts_an_empty_scalar_past_the_last_line_at_the_end_of_the_file() {
    assert_eq!(last_node_position("---"), (1, 4));
    assert_eq!(last_node_position("? "), (1, 3));
    assert_eq!(last_node_position("---\n"), (2, 1));
    assert_eq!(last_node_position("? \n"), (2, 1));
}

#[test]
fn explicit_key_without_a_value_has_its_value_after_the_last_token() {
    let scalars = scalars("x:\n  ? !!str\n\ny: 1\n").unwrap();
    assert_eq!(at(scalars[2].start), (1, 9));
}

#[test]
fn block_end_moves_to_a_foot_comment_at_the_indentation_of_the_block() {
    let (events, err) = run("a:\n  b: 1\n  # foot\nc: 2\n");
    assert!(err.is_none());
    let inner_end = events
        .iter()
        .find(|event| matches!(event.data, EventData::MappingEnd))
        .unwrap();
    assert_eq!(at(inner_end.start_mark), (2, 3));

    let scalars = scalars("x:\n  ? a\n  # foot\ny: 1\n").unwrap();
    assert_eq!(at(scalars[2].start), (2, 3));
}

#[test]
fn block_end_after_a_quoted_scalar_is_right_after_the_quote() {
    let (events, _) = run("a:\n  b: \"x\"\nc: 2\n");
    let inner_end = events
        .iter()
        .find(|event| matches!(event.data, EventData::MappingEnd))
        .unwrap();
    assert_eq!(at(inner_end.start_mark), (1, 8));
}

#[test]
fn indentless_sequence_ends_where_the_next_token_starts() {
    let (events, _) = run("a:\n- b\nc: d\n");
    let end = events
        .iter()
        .find(|event| matches!(event.data, EventData::SequenceEnd))
        .unwrap();
    assert_eq!(end.start_mark, end.end_mark);
}

// The iterator ends after the first error.

#[test]
fn iterator_ends_after_the_first_error() {
    for input in ["a: b: c", "\t", "[a"] {
        let mut parser = EventParser::new(input);
        let mut errors = 0;
        let mut steps = 0;
        for event in parser.by_ref() {
            steps += 1;
            assert!(steps < 100, "{input:?}: the iterator does not end");
            if event.is_err() {
                errors += 1;
            }
        }
        assert_eq!(errors, 1, "{input:?}");
        assert!(parser.next().is_none(), "{input:?}");
    }
}
