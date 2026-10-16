//! Unit tests of the scalar source maps.
//!
//! The first group covers the wrong positions that section 1.3 of
//! `docs/research/01-parsing-ast.md` records for the reference. The others
//! cover each scalar style.

#![allow(clippy::unwrap_used, clippy::panic)]

use workflint_diagnostics::{FileId, LineCol, SourceFile};
use workflint_yaml::{Document, Node, NodeKind, SourceMap, parse};

fn source(text: &str) -> SourceFile {
    SourceFile::new(FileId::new(0), "test.yaml", text).unwrap()
}

fn document(file: &SourceFile) -> Document {
    let parsed = parse(file);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    parsed.document.unwrap()
}

/// Returns the first scalar node, in source order, whose value contains
/// `needle`.
fn scalar_with<'d>(document: &'d Document, needle: &str) -> &'d Node {
    document
        .nodes()
        .map(|(_, node)| node)
        .find(|node| matches!(&node.kind, NodeKind::Scalar(s) if s.value.contains(needle)))
        .unwrap_or_else(|| panic!("no scalar contains {needle:?}"))
}

fn value(node: &Node) -> &str {
    let NodeKind::Scalar(scalar) = &node.kind else {
        panic!("not a scalar");
    };
    &scalar.value
}

/// Builds the source map of the scalar that contains `needle` and returns it
/// with the decoded offset of `needle`.
fn map_of<'f>(file: &'f SourceFile, document: &Document, needle: &str) -> (SourceMap<'f>, usize) {
    let node = scalar_with(document, needle);
    let offset = value(node).find(needle).unwrap();
    let map = node.source_map(file).unwrap();
    assert!(map.is_exact(), "the map of {:?} is not exact", value(node));
    (map, offset)
}

/// Returns the line and column where `needle` maps to.
fn position(text: &str, needle: &str) -> LineCol {
    let file = source(text);
    let document = document(&file);
    let (map, offset) = map_of(&file, &document, needle);
    map.line_col(offset)
}

fn at(line: usize, column: usize) -> LineCol {
    LineCol { line, column }
}

#[test]
fn expression_on_line_8_of_a_literal_block_maps_to_line_8() {
    let text = "\
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: |
          echo hello
          echo ${{ github.foo }}
";
    assert_eq!(position(text, "${{"), at(8, 16));
}

#[test]
fn expression_after_a_tab_escape_maps_to_its_real_column() {
    let text = "run: \"a\\tb ${{ github.bar }}\"\n";
    assert_eq!(position(text, "${{"), at(1, 12));
}

#[test]
fn expression_after_multibyte_characters_maps_to_its_code_point_column() {
    let text = "run: \"日本 ${{ github.baz }}\"\n";
    assert_eq!(position(text, "${{"), at(1, 10));
}

#[test]
fn expression_inside_a_folded_block_maps_to_its_own_line() {
    let text = "\
run: >
  echo a
  echo ${{ github.qux }}
";
    assert_eq!(position(text, "${{"), at(3, 8));
}

#[test]
fn doubled_quote_in_a_single_quoted_scalar_maps_to_the_first_quote() {
    let text = "x: 'it''s ${{ a }}'\n";
    let file = source(text);
    let document = document(&file);
    let (map, offset) = map_of(&file, &document, "${{");
    assert_eq!(map.line_col(offset), at(1, 11));
    // The `'` of `it's` comes from the first quote of `''`.
    assert_eq!(map.source_offset(2), 6);
    assert_eq!(map.source_offset(3), 8);
}

#[test]
fn smiley_written_as_a_character_maps_to_itself() {
    let text = "x: \"☺ ${{ a }}\"\n";
    let file = source(text);
    let document = document(&file);
    let (map, offset) = map_of(&file, &document, "☺");
    assert_eq!(map.source_offset(offset), 4);
    assert_eq!(map.line_col(offset), at(1, 5));
    assert_eq!(position(text, "${{"), at(1, 7));
}

#[test]
fn unicode_escapes_map_to_their_backslash() {
    let text = "x: \"\\u263A \\U0001F600 ${{ a }}\"\n";
    let file = source(text);
    let document = document(&file);
    let (map, smiley) = map_of(&file, &document, "☺");
    assert_eq!(smiley, 0);
    assert_eq!(map.source_offset(smiley), 4);
    let grin = value(scalar_with(&document, "😀")).find('😀').unwrap();
    assert_eq!(map.source_offset(grin), 11);
    // `${{` follows the ten bytes of `\U0001F600` and a space.
    assert_eq!(position(text, "${{"), at(1, 23));
}

#[test]
fn escape_covers_its_whole_sequence_in_a_source_span() {
    let text = "x: \"a\\U0001F600b\"\n";
    let file = source(text);
    let document = document(&file);
    let (map, grin) = map_of(&file, &document, "😀");
    let span = map.source_span(grin..grin + '😀'.len_utf8());
    assert_eq!(&text[span.start as usize..span.end as usize], "\\U0001F600");
}

#[test]
fn escaped_line_break_is_dropped_with_the_next_line_indentation() {
    let text = "x: \"ab\\\n    cd ${{ e }}\"\n";
    let file = source(text);
    let document = document(&file);
    let node = scalar_with(&document, "abcd");
    assert_eq!(value(node), "abcd ${{ e }}");
    let (map, c) = map_of(&file, &document, "cd");
    assert_eq!(map.line_col(c), at(2, 5));
    assert_eq!(map.line_col(c - 1), at(1, 6));
}

#[test]
fn plain_scalar_folds_a_line_break_into_a_space_at_the_break() {
    let text = "x: echo\n   ${{ a }}  and\n\n  more\n";
    let file = source(text);
    let document = document(&file);
    let node = scalar_with(&document, "echo");
    assert_eq!(value(node), "echo ${{ a }}  and\nmore");
    let map = node.source_map(&file).unwrap();
    assert!(map.is_exact());
    // The space comes from the line break after `echo`.
    assert_eq!(map.source_offset(4), 7);
    assert_eq!(map.line_col(5), at(2, 4));
    // The newline comes from the second of the two line breaks.
    let newline = value(node).find('\n').unwrap();
    assert_eq!(map.source_offset(newline), 25);
    assert_eq!(map.line_col(newline + 1), at(4, 3));
}

#[test]
fn literal_block_with_strip_chomping_ends_after_the_last_character() {
    let text = "x: |-\n  one\n  two\n\ny: 1\n";
    let file = source(text);
    let document = document(&file);
    let (map, two) = map_of(&file, &document, "two");
    assert_eq!(map.line_col(two), at(3, 3));
    let end = map.source_offset("one\ntwo".len());
    assert_eq!(&text[..end as usize], "x: |-\n  one\n  two");
}

#[test]
fn literal_block_with_keep_chomping_maps_kept_breaks_to_empty_lines() {
    let text = "x: |+\n  one\n\n\ny: 1\n";
    let file = source(text);
    let document = document(&file);
    let node = scalar_with(&document, "one");
    assert_eq!(value(node), "one\n\n\n");
    let map = node.source_map(&file).unwrap();
    assert!(map.is_exact());
    assert_eq!(map.line_col(3), at(2, 6));
    assert_eq!(map.line_col(4), at(3, 1));
    assert_eq!(map.line_col(5), at(4, 1));
    assert_eq!(map.source_offset(6), 14);
}

#[test]
fn literal_block_with_an_indentation_indicator_keeps_extra_spaces() {
    let text = "steps:\n  - run: |2\n        indented\n      flush\n";
    let file = source(text);
    let document = document(&file);
    let node = scalar_with(&document, "indented");
    assert_eq!(value(node), "  indented\nflush\n");
    let map = node.source_map(&file).unwrap();
    assert!(map.is_exact());
    assert_eq!(map.line_col(0), at(3, 7));
    assert_eq!(map.line_col(2), at(3, 9));
    assert_eq!(map.line_col(11), at(4, 7));
}

#[test]
fn literal_block_maps_crlf_line_breaks_to_the_carriage_return() {
    let text = "x: |\r\n  a\r\n  b\r\n";
    let file = source(text);
    let document = document(&file);
    let node = scalar_with(&document, "a");
    assert_eq!(value(node), "a\nb\n");
    let map = node.source_map(&file).unwrap();
    assert!(map.is_exact());
    assert_eq!(map.source_offset(1), 9);
    assert_eq!(map.line_col(2), at(3, 3));
    assert_eq!(map.source_offset(4), 16);
}

#[test]
fn folded_block_keeps_more_indented_lines() {
    let text = "x: >\n  a\n  b\n    c\n  d\n";
    let file = source(text);
    let document = document(&file);
    let node = scalar_with(&document, "a b");
    assert_eq!(value(node), "a b\n  c\nd\n");
    let map = node.source_map(&file).unwrap();
    assert!(map.is_exact());
    // The space between `a` and `b` is the line break after `a`.
    assert_eq!(map.source_offset(1), 8);
    assert_eq!(map.line_col(4), at(4, 3));
    assert_eq!(map.line_col(8), at(5, 3));
}

#[test]
fn source_span_of_an_expression_in_a_literal_block_covers_its_source() {
    let text = "x: |\n  echo ${{ a.b }}\n";
    let file = source(text);
    let document = document(&file);
    let (map, start) = map_of(&file, &document, "${{");
    let span = map.source_span(start..start + "${{ a.b }}".len());
    assert_eq!(&text[span.start as usize..span.end as usize], "${{ a.b }}");
    assert_eq!(span.file, file.id());
}

#[test]
fn end_of_an_empty_quoted_scalar_maps_to_the_closing_quote() {
    let text = "x: ''\ny: \"\"\n";
    let file = source(text);
    let document = document(&file);
    let scalars: Vec<_> = document
        .nodes()
        .filter(|(_, node)| matches!(&node.kind, NodeKind::Scalar(s) if s.value.is_empty()))
        .map(|(_, node)| node.source_map(&file).unwrap().source_offset(0))
        .collect();
    assert_eq!(scalars, [4, 10]);
}

#[test]
fn offset_inside_a_multibyte_character_maps_to_its_start() {
    let text = "x: |\n  日本\n";
    let file = source(text);
    let document = document(&file);
    let (map, _) = map_of(&file, &document, "日本");
    assert_eq!(map.source_offset(1), 7);
    assert_eq!(map.source_offset(3), 10);
}

#[test]
fn collections_have_no_source_map() {
    let file = source("x: [a]\n");
    let document = document(&file);
    let root = document.node(document.root().unwrap()).unwrap();
    assert!(root.source_map(&file).is_none());
}

#[test]
fn source_map_needs_the_file_the_node_comes_from() {
    let file = source("x: a\n");
    let other = SourceFile::new(FileId::new(1), "other.yaml", "x: a\n").unwrap();
    let document = document(&file);
    assert!(scalar_with(&document, "a").source_map(&other).is_none());
}
