//! Snapshot tests of the span tree.
//!
//! Each snapshot holds the tree dump from `Document::dump`, followed by the
//! diagnostics of the parse, one per line as `line:column: message [rule-id]`.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fmt::Write as _;

use insta::assert_snapshot;
use workflint_diagnostics::{FileId, SourceFile, sort_and_dedup};
use workflint_yaml::{CommentKind, Document, KeyComparison, NodeKind, Parsed, parse};

fn source(text: &str) -> SourceFile {
    SourceFile::new(FileId::new(0), "test.yaml", text).unwrap()
}

fn render(file: &SourceFile, parsed: &mut Parsed) -> String {
    let mut out = match &parsed.document {
        Some(document) => document.dump(file),
        None => "no document\n".to_owned(),
    };
    sort_and_dedup(file, &mut parsed.diagnostics);
    for diagnostic in &parsed.diagnostics {
        let pos = file.line_col(diagnostic.span.start);
        let _ = writeln!(
            out,
            "{}:{}: {} [{}]",
            pos.line, pos.column, diagnostic.message, diagnostic.rule.id
        );
    }
    out
}

/// Checks the snapshot `name` of the parse of `text`, and returns the
/// document for further checks.
fn check(name: &str, text: &str) -> (SourceFile, Option<Document>) {
    let file = source(text);
    let mut parsed = parse(&file);
    let mut settings = insta::Settings::clone_current();
    settings.set_description(text);
    settings.set_omit_expression(true);
    settings.bind(|| assert_snapshot!(name, render(&file, &mut parsed)));
    (file, parsed.document)
}

fn mapping_entries(document: &Document) -> Vec<(String, String)> {
    let root = document.root().and_then(|id| document.node(id)).unwrap();
    let NodeKind::Mapping(mapping) = &root.kind else {
        panic!("root is not a mapping");
    };
    document
        .entries(mapping)
        .iter()
        .map(|entry| {
            let text = |id| match &document.node(id).unwrap().kind {
                NodeKind::Scalar(scalar) => scalar.value.clone(),
                other => format!("{other:?}"),
            };
            (text(entry.key), text(entry.value))
        })
        .collect()
}

#[test]
fn anchors_and_aliases() {
    check(
        "anchors_and_aliases",
        "scalar: &s value\nuse_scalar: *s\nmap: &m {k: v}\nuse_map: *m\nseq: &q\n  - a\nuse_seq: *q\n*s : alias as key\n",
    );
}

#[test]
fn an_anchor_redefined_later_applies_to_later_aliases() {
    check(
        "anchor_redefined",
        "a: &x first\nb: *x\nc: &x second\nd: *x\n",
    );
}

#[test]
fn explicit_tags() {
    check(
        "explicit_tags",
        "str: !!str 1\nint: !!int text\ncustom: !custom v\nverbatim: !<tag:yaml.org,2002:float> 1\nnon_specific: ! 12\nmap: !!map {}\nseq: !!seq []\nanchor_then_tag: &a !!str x\ntag_then_anchor: !!str &b y\nempty: !!null\nuse: [*a, *b]\n",
    );
}

#[test]
fn tag_directive_expands_the_handle() {
    check(
        "tag_directive",
        "%TAG !e! tag:example.com,2000:app/\n---\na: !e!foo x\nb: !!binary aGk=\n",
    );
}

#[test]
fn every_scalar_style_and_block_header() {
    check(
        "scalar_styles",
        concat!(
            "plain: a b\n",
            "multi_line_plain: a\n  b\n",
            "single: 'it''s'\n",
            "double: \"a\\tb\\u00e9\"\n",
            "literal_clip: |\n  x\n  y\n\n",
            "literal_strip: |-\n  x\n\n",
            "literal_keep: |+\n  x\n\n",
            "literal_indent: |2\n   x\n",
            "literal_indent_strip: |2-\n   x\n",
            "literal_strip_indent: |-2\n   x\n",
            "folded_clip: >\n  x\n  y\n",
            "folded_strip: >-\n  x\n",
            "folded_keep: >+\n  x\n",
            "folded_with_comment: > # header comment\n  x\n",
            "anchored_literal: &lit |\n  x\n",
            "last: |\n  end",
        ),
    );
}

#[test]
fn plain_scalars_resolve_under_the_core_schema() {
    check(
        "core_schema",
        concat!(
            "- [true, True, TRUE, false, False, FALSE]\n",
            "- [yes, no, on, off, y, n, Yes, ON]\n",
            "- [~, null, Null, NULL, '']\n",
            "- [0, 1, -1, +1, 017, 0o17, 0x1F, 0b10, 1_000, -0]\n",
            "- [1.5, .5, 1., -1e3, 1E+3, .inf, -.Inf, +.INF, .nan, .NaN, .NAN]\n",
            "- [2026-10-15, 2001-12-14T21:59:43Z, '1:20', 1:20, <<, 0x, 1e, .]\n",
            "- 'true'\n",
            "- \"1\"\n",
            "- |\n  1\n",
            "- \n",
        ),
    );
}

#[test]
fn flow_and_block_collections() {
    check(
        "collections",
        concat!(
            "block_map:\n  a: 1\n  b: 2\n",
            "block_seq:\n  - 1\n  - 2\n",
            "indentless_seq:\n- 1\n- 2\n",
            "flow_map: {a: 1, b: [x, y]}\n",
            "flow_seq: [1, {c: d}, [e]]\n",
            "nested:\n  - - a\n    - b\n  - k: v\n    l: w\n",
            "empty_flow: [[], {}]\n",
        ),
    );
}

#[test]
fn implicit_null_is_right_after_the_colon() {
    check(
        "implicit_null",
        "on:\njobs:\n  test:\n    permissions:\n    steps:\n      -\n      - run:\n",
    );
}

#[test]
fn duplicate_keys_stay_in_source_order() {
    let (_, document) = check("duplicate_keys", "a: 1\nA: 2\na: 3\nb: 4\nB: 5\nb: 6\n");
    let document = document.unwrap();
    assert_eq!(
        mapping_entries(&document),
        [
            ("a", "1"),
            ("A", "2"),
            ("a", "3"),
            ("b", "4"),
            ("B", "5"),
            ("b", "6")
        ]
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
    );
}

#[test]
fn duplicate_keys_are_listed_under_the_chosen_comparison() {
    let file = source("a: 1\nA: 2\na: 3\nb: 4\nB: 5\nc: 6\n'a': 7\n? [x]\n: 8\n? [x]\n: 9\n");
    let parsed = parse(&file);
    let document = parsed.document.unwrap();
    let root = document.node(document.root().unwrap()).unwrap();
    let NodeKind::Mapping(mapping) = &root.kind else {
        panic!("root is not a mapping");
    };
    let key_lines = |comparison| {
        document
            .duplicate_keys(mapping, comparison)
            .iter()
            .map(|duplicate| {
                let line = |id| file.line_col(document.node(id).unwrap().span.start).line;
                (line(duplicate.first.key), line(duplicate.duplicate.key))
            })
            .collect::<Vec<_>>()
    };
    // The first entry with a key wins, and keys that are not scalars are
    // never duplicates.
    assert_eq!(key_lines(KeyComparison::Exact), [(1, 3), (1, 7)]);
    assert_eq!(
        key_lines(KeyComparison::AsciiCaseInsensitive),
        [(1, 2), (1, 3), (4, 5), (1, 7)]
    );
}

#[test]
fn merge_key_is_a_plain_entry() {
    check(
        "merge_key",
        "base: &b {x: 1}\nderived:\n  <<: *b\n  y: 2\nquoted:\n  \"<<\": *b\n",
    );
}

#[test]
fn bom_and_crlf_line_breaks() {
    check(
        "bom_crlf",
        "\u{feff}on: push # c\r\njobs:\r\n  a: |\r\n    x\r\n  b: [1,\r\n    2]\r\n",
    );
}

#[test]
fn only_the_first_document_is_in_the_tree() {
    check(
        "multi_document",
        "# before\na: 1\n# end of first\n...\n# after end\n---\nb: *x\n---\n[\n",
    );
}

#[test]
fn an_empty_first_document_is_a_null_scalar() {
    check("empty_first_document", "---\n...\n---\non: push\n");
}

#[test]
fn an_empty_file_has_no_root() {
    check("empty_file", "");
}

#[test]
fn a_comment_only_file_has_no_root() {
    check("comment_only_file", "# one\n  # two\n");
}

#[test]
fn comments_attach_to_nodes() {
    let (file, document) = check(
        "comment_attachment",
        concat!(
            "# before the first entry\n",
            "on: push # trailing on a scalar value\n",
            "\n",
            "# before an entry\n",
            "#   in two lines\n",
            "jobs:\n",
            "  test: # trailing after a key\n",
            "    steps:\n",
            "      # before the first item\n",
            "      - run: a\n",
            "      # before the second item\n",
            "      - uses: b\n",
            "        with: [x, # in a flow sequence\n",
            "          # before a flow item\n",
            "          y]\n",
            "# at the end\n",
        ),
    );
    let document = document.unwrap();
    let attached: Vec<(String, Option<String>)> = document
        .comments()
        .iter()
        .map(|comment| {
            let text = &file.text()[comment.span.start as usize..comment.span.end as usize];
            let node = comment.node.map(|id| {
                let node = document.node(id).unwrap();
                let pos = file.line_col(node.span.start);
                let what = match &node.kind {
                    NodeKind::Scalar(scalar) => format!("scalar {:?}", scalar.value),
                    NodeKind::Mapping(_) => "mapping".to_owned(),
                    NodeKind::Sequence(_) => "sequence".to_owned(),
                    NodeKind::Alias(alias) => format!("alias {}", alias.name),
                };
                format!("{what} at {}:{}", pos.line, pos.column)
            });
            (text.to_owned(), node)
        })
        .collect();
    let want: [(&str, Option<&str>); 9] = [
        ("# before the first entry", Some("scalar \"on\" at 2:1")),
        (
            "# trailing on a scalar value",
            Some("scalar \"push\" at 2:5"),
        ),
        ("# before an entry", Some("scalar \"jobs\" at 6:1")),
        ("#   in two lines", Some("scalar \"jobs\" at 6:1")),
        ("# trailing after a key", Some("scalar \"test\" at 7:3")),
        ("# before the first item", Some("mapping at 10:9")),
        ("# before the second item", Some("mapping at 12:9")),
        ("# in a flow sequence", Some("scalar \"x\" at 13:16")),
        ("# before a flow item", Some("scalar \"y\" at 15:11")),
    ];
    let want: Vec<(String, Option<String>)> = want
        .into_iter()
        .map(|(text, node)| (text.to_owned(), node.map(str::to_owned)))
        .chain([("# at the end".to_owned(), None)])
        .collect();
    assert_eq!(attached, want);

    // Each node lists its own comments in source order.
    let jobs_key = document.comments()[2].node.unwrap();
    let texts: Vec<_> = document
        .node_comments(jobs_key)
        .map(|c| {
            (
                c.kind,
                &file.text()[c.span.start as usize..c.span.end as usize],
            )
        })
        .collect();
    assert_eq!(
        texts,
        [
            (CommentKind::OwnLine, "# before an entry"),
            (CommentKind::OwnLine, "#   in two lines"),
        ]
    );
}

fn diagnostics(text: &str) -> String {
    let file = source(text);
    let mut parsed = parse(&file);
    let mut out = String::new();
    sort_and_dedup(&file, &mut parsed.diagnostics);
    for diagnostic in &parsed.diagnostics {
        let start = file.line_col(diagnostic.span.start);
        let end = file.line_col(diagnostic.span.end);
        let _ = writeln!(
            out,
            "{}:{}-{}:{}: {} [{}]",
            start.line, start.column, end.line, end.column, diagnostic.message, diagnostic.rule.id
        );
    }
    out
}

// The reference reports `tagfirst.yaml:3:6: anchor "x" is defined but not
// used` for this input, at the tag.
#[test]
fn an_unused_anchor_after_a_tag_is_reported_from_the_tag() {
    assert_eq!(
        diagnostics("on: push\njobs:\n  a: !!str &x\n    runs-on: x\n"),
        "3:6-3:14: anchor \"x\" is defined but not used [unused-anchor]\n"
    );
}

// The reference reports this error at 0:0 (docs/differences.md#yaml-error-positions).
#[test]
fn a_control_character_is_reported_at_its_position() {
    assert_eq!(
        diagnostics("on: push\njobs:\n  a:\n    runs-on: x\u{1}\n"),
        "4:15-4:16: could not parse as YAML: control characters are not allowed [yaml-syntax-error]\n"
    );
}
