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
        key_lines(KeyComparison::CaseInsensitive),
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
fn the_document_starts_at_its_first_token() {
    // go-yaml reports a document at the start mark of its document start
    // event: the first directive, the `---`, or else the first token of the
    // root node, properties included.
    let cases = [
        ("on: push\n", (1, 1)),
        ("# c\n\n  on: push\n", (3, 3)),
        ("# c\n\n---\n\non: push\n", (3, 1)),
        ("%YAML 1.1\n---\non: push\n", (1, 1)),
        ("&a\non: *a\n", (1, 1)),
        ("---\n", (1, 1)),
        ("", (1, 1)),
        ("# only\n", (1, 1)),
    ];
    for (text, want) in cases {
        let file = source(text);
        let document = parse(&file).document.unwrap();
        let pos = file.line_col(document.start());
        assert_eq!((pos.line, pos.column), want, "{text:?}");
    }
}

#[test]
fn an_empty_file_has_no_root() {
    check("empty_file", "");
}

#[test]
fn a_comment_only_file_has_no_root() {
    check("comment_only_file", "# one\n  # two\n");
}

/// Lists each comment's text with the node it is attached to, as the node's
/// kind and its 1-based start position.
fn attachments(file: &SourceFile, document: &Document) -> Vec<(String, Option<String>)> {
    document
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
        .collect()
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
    let attached = attachments(&file, &document);
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

fn owned(pairs: &[(&str, Option<&str>)]) -> Vec<(String, Option<String>)> {
    pairs
        .iter()
        .map(|&(text, node)| (text.to_owned(), node.map(str::to_owned)))
        .collect()
}

#[test]
fn an_own_line_comment_before_a_line_without_a_node_attaches_to_no_node() {
    let file = source("a: [\n  x,\n  # c\n]\nb: 1\n");
    let document = parse(&file).document.unwrap();
    assert_eq!(attachments(&file, &document), owned(&[("# c", None)]));
}

#[test]
fn an_own_line_comment_before_an_empty_line_attaches_to_no_node() {
    let file = source("a: 1\n# c\n\nb: 2\n# d\n  \nc: 3\n");
    let document = parse(&file).document.unwrap();
    assert_eq!(
        attachments(&file, &document),
        owned(&[("# c", None), ("# d", None)])
    );
}

#[test]
fn own_line_comments_in_a_row_attach_to_the_line_after_the_last() {
    let file = source("a: [\n  # c\n  # d\n  x]\n");
    let document = parse(&file).document.unwrap();
    assert_eq!(
        attachments(&file, &document),
        owned(&[
            ("# c", Some("scalar \"x\" at 4:3")),
            ("# d", Some("scalar \"x\" at 4:3")),
        ])
    );
}

#[test]
fn comments_on_an_alias_item_are_reachable_through_the_written_items() {
    let file = source("- &x 1\n# c\n- *x # t\n");
    let document = parse(&file).document.unwrap();
    assert_eq!(
        attachments(&file, &document),
        owned(&[
            ("# c", Some("alias x at 3:3")),
            ("# t", Some("alias x at 3:3"))
        ])
    );

    let root = document.root().unwrap();
    let NodeKind::Sequence(sequence) = &document.node(root).unwrap().kind else {
        panic!("root is not a sequence");
    };
    let [anchored, alias] = document.written_items(sequence)[..] else {
        panic!("the sequence does not have two items");
    };
    // `items` replaces the alias by the anchored node, which has no comment.
    assert_eq!(document.items(sequence), [anchored, anchored]);
    assert_eq!(document.node_comments(anchored).count(), 0);
    let texts: Vec<_> = document
        .node_comments(alias)
        .map(|c| &file.text()[c.span.start as usize..c.span.end as usize])
        .collect();
    assert_eq!(texts, ["# c", "# t"]);
}

#[test]
fn written_entries_keep_aliases_where_they_are_written() {
    let file = source("a: &x [1, &k k]\nb: *x # t\n*k : [*x]\n");
    let document = parse(&file).document.unwrap();
    let root = document.root().unwrap();
    let describe = |id| {
        let node = document.node(id).unwrap();
        let pos = file.line_col(node.span.start);
        let what = match &node.kind {
            NodeKind::Scalar(scalar) => format!("scalar {:?}", scalar.value),
            NodeKind::Mapping(_) => "mapping".to_owned(),
            NodeKind::Sequence(_) => "sequence".to_owned(),
            NodeKind::Alias(alias) => format!("alias {}", alias.name),
        };
        format!("{what} at {}:{}", pos.line, pos.column)
    };
    let NodeKind::Mapping(mapping) = &document.node(root).unwrap().kind else {
        panic!("root is not a mapping");
    };
    let entries: Vec<_> = document
        .written_entries(mapping)
        .iter()
        .map(|entry| (describe(entry.key), describe(entry.value)))
        .collect();
    assert_eq!(
        entries,
        [
            ("scalar \"a\" at 1:1", "sequence at 1:4"),
            ("scalar \"b\" at 2:1", "alias x at 2:4"),
            ("alias k at 3:1", "sequence at 3:6"),
        ]
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
    );
    let last = document.written_entries(mapping)[2].value;
    let NodeKind::Sequence(sequence) = &document.node(last).unwrap().kind else {
        panic!("the last value is not a sequence");
    };
    let items: Vec<_> = document
        .written_items(sequence)
        .iter()
        .map(|&id| describe(id))
        .collect();
    assert_eq!(items, ["alias x at 3:7"]);
    // The lists are parallel to the resolved ones.
    assert_eq!(
        document.written_entries(mapping).len(),
        document.entries(mapping).len()
    );
    assert_eq!(document.items(sequence).len(), 1);
}

#[test]
fn duplicate_keys_compare_lowercase_characters_like_go() {
    let file = source(concat!(
        "Ä: 1\nä: 2\n",
        // Go maps U+0130 to `i` alone, without the combining dot.
        "İ: 3\ni: 4\n",
        // The Kelvin sign lowercases to `k`.
        "\u{212a}: 5\nk: 6\n",
        // Final sigma has no case mapping of its own, so it stays apart.
        "Σ: 7\nσ: 8\nς: 9\n",
        "ǅ: 10\nǆ: 11\n",
    ));
    let document = parse(&file).document.unwrap();
    let root = document.node(document.root().unwrap()).unwrap();
    let NodeKind::Mapping(mapping) = &root.kind else {
        panic!("root is not a mapping");
    };
    let lines: Vec<_> = document
        .duplicate_keys(mapping, KeyComparison::CaseInsensitive)
        .iter()
        .map(|duplicate| {
            let line = |id| file.line_col(document.node(id).unwrap().span.start).line;
            (line(duplicate.first.key), line(duplicate.duplicate.key))
        })
        .collect();
    assert_eq!(lines, [(1, 2), (3, 4), (5, 6), (7, 8), (10, 11)]);
}

#[test]
fn merge_keys_are_never_duplicates() {
    let file = source("<<: {a: 1}\n<<: {b: 2}\n'<<': {c: 3}\nx: 1\nX: 2\n");
    let document = parse(&file).document.unwrap();
    let root = document.node(document.root().unwrap()).unwrap();
    let NodeKind::Mapping(mapping) = &root.kind else {
        panic!("root is not a mapping");
    };
    assert_eq!(document.duplicate_keys(mapping, KeyComparison::Exact), []);
    let duplicates = document.duplicate_keys(mapping, KeyComparison::CaseInsensitive);
    let lines: Vec<_> = duplicates
        .iter()
        .map(|duplicate| {
            let line = |id| file.line_col(document.node(id).unwrap().span.start).line;
            (line(duplicate.first.key), line(duplicate.duplicate.key))
        })
        .collect();
    assert_eq!(lines, [(4, 5)]);
}

/// go-yaml accepts this depth and rejects one level more.
const MAX_DEPTH: usize = 10_000;

#[test]
fn a_document_at_the_depth_limit_dumps_on_a_two_mebibyte_stack() {
    let dump = std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let file = source(&format!(
                "{}{}\n",
                "[".repeat(MAX_DEPTH),
                "]".repeat(MAX_DEPTH)
            ));
            let document = parse(&file).document.unwrap();
            document.dump(&file)
        })
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(dump.lines().count(), MAX_DEPTH);
    let innermost = dump.lines().last().unwrap();
    assert!(
        innermost.starts_with(&format!(
            "{}item #{} sequence flow",
            "  ".repeat(MAX_DEPTH - 1),
            MAX_DEPTH - 1
        )),
        "{innermost}"
    );
}

// Building the tree takes time linear in the size of the input. Walking the
// open collections for each scalar took over 20 seconds here in a debug
// build, and the parse alone takes about a second.
#[test]
fn many_scalars_at_the_depth_limit_build_in_linear_time() {
    const SCALARS: usize = 200_000;
    let text = format!(
        "{}{}{}\n",
        "[".repeat(MAX_DEPTH),
        "1,".repeat(SCALARS),
        "]".repeat(MAX_DEPTH)
    );
    let file = source(&text);
    let started = std::time::Instant::now();
    let parsed = parse(&file);
    let elapsed = started.elapsed();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(
        parsed.document.unwrap().nodes().count(),
        MAX_DEPTH + SCALARS
    );
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "building the tree took {elapsed:?}"
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

// The reference reports `4:13` with a snippet of line 4: go-yaml's position
// of an error with a context is the context mark, here the opening quote,
// with a 0-based column (docs/differences.md#yaml-error-positions).
#[test]
fn an_unclosed_quoted_scalar_is_reported_at_its_opening_quote() {
    assert_eq!(
        diagnostics(
            "on: push\njobs:\n  test:\n    runs-on: \"ubuntu\n    steps:\n      - run: echo\n# end\n"
        ),
        "4:14-4:15: could not parse as YAML: found unexpected end of stream [yaml-syntax-error]\n"
    );
    assert_eq!(
        diagnostics("on: push\njobs:\n  test:\n    runs-on: 'ubuntu\n    steps:\n---\n"),
        "4:14-4:15: could not parse as YAML: found unexpected document indicator [yaml-syntax-error]\n"
    );
}

// An escape error in a quoted scalar has a context too, but it is reported
// at the escape's backslash, which says more than the opening quote.
#[test]
fn an_unknown_escape_is_reported_at_the_escape() {
    assert_eq!(
        diagnostics("a: \"x\\qy\"\n"),
        "1:6-1:7: could not parse as YAML: found unknown escape character [yaml-syntax-error]\n"
    );
}

#[test]
fn a_comment_after_a_dash_attaches_to_the_item_that_follows() {
    let file = source("steps:\n  - # c\n    run: a\n  - # d\n    uses: b\n");
    let document = parse(&file).document.unwrap();
    assert_eq!(
        attachments(&file, &document),
        owned(&[
            ("# c", Some("mapping at 3:5")),
            ("# d", Some("mapping at 5:5")),
        ])
    );
}

#[test]
fn a_comment_after_indicators_attaches_to_the_node_that_follows() {
    for (text, want) in [
        ("? # c\n  a\n: b\n", "scalar \"a\" at 2:3"),
        ("- - # c\n    x\n", "scalar \"x\" at 2:5"),
        ("\u{feff}- # c\n  x\n", "scalar \"x\" at 2:3"),
        ("- # c\n  - x\n", "sequence at 2:3"),
        ("- # c\n- b\n", "scalar \"\" at 1:2"),
    ] {
        let file = source(text);
        let document = parse(&file).document.unwrap();
        assert_eq!(
            attachments(&file, &document),
            owned(&[("# c", Some(want))]),
            "{text:?}"
        );
    }
}

#[test]
fn a_comment_after_a_node_or_a_document_marker_keeps_the_trailing_rule() {
    for (text, want) in [
        ("- a # c\n", Some("scalar \"a\" at 1:3")),
        ("- -a # c\n", Some("scalar \"-a\" at 1:3")),
        // `---` is not three `-` indicators, and no node starts before the
        // comment on its line.
        ("--- # c\na\n", None),
    ] {
        let file = source(text);
        let document = parse(&file).document.unwrap();
        assert_eq!(
            attachments(&file, &document),
            owned(&[("# c", want)]),
            "{text:?}"
        );
    }
}
