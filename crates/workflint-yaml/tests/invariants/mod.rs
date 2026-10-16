//! The checks that the fuzz targets in `fuzz/` run on each input.
//!
//! The fuzz targets include this file with `#[path]`, and
//! `fuzz_regressions.rs` and `reference_files.rs` run it on files, so a crash
//! input found by the fuzzer fails the same check as a regression test. Each
//! check panics when an invariant does not hold.

#![allow(clippy::unwrap_used)]

use std::borrow::Cow;

use workflint_diagnostics::{FileId, SourceFile, Span, sort_and_dedup};
use workflint_yaml::event::{EventParser, Mark};
use workflint_yaml::{NodeId, NodeKind, parse};

/// Turns fuzzer bytes into the text the checks parse. Invalid UTF-8 becomes
/// U+FFFD, so every input reaches the parser.
pub(crate) fn text(data: &[u8]) -> Cow<'_, str> {
    String::from_utf8_lossy(data)
}

/// Drains the event parser on `text` up to the end of the stream or the
/// first error.
///
/// Every event's marks lie inside `text` on character boundaries, and no
/// event ends before it starts.
pub(crate) fn check_events(text: &str) {
    for event in EventParser::new(text) {
        let Ok(event) = event else { break };
        let what = format!("{:?}", event.data);
        check_marks(text, event.start_mark, event.end_mark, &what);
        if let Some((start, end)) = event.node_marks.anchor {
            check_marks(text, start, end, &format!("anchor of {what}"));
        }
        if let Some(content) = event.node_marks.content {
            check_marks(text, content, content, &format!("content of {what}"));
        }
    }
}

fn check_marks(text: &str, start: Mark, end: Mark, what: &str) {
    let range = usize::try_from(start.index).unwrap()..usize::try_from(end.index).unwrap();
    assert!(
        range.start <= range.end,
        "{what}: ends at {} before it starts at {}",
        range.end,
        range.start
    );
    assert!(
        text.get(range.clone()).is_some(),
        "{what}: {range:?} is outside the input or not on character boundaries"
    );
}

/// Builds the span tree of `text` and checks it.
///
/// A parse without a document has exactly one diagnostic. Every span of a
/// node, anchor, comment, and diagnostic lies inside `text` on character
/// boundaries and converts to a line and column, and its end does not come
/// before its start. Each node's content starts inside its span. Every node
/// ID in the tree refers to a node.
pub(crate) fn check_tree(text: &str) {
    let file = SourceFile::new(FileId::new(0), "fuzz.yaml", text).unwrap();
    let mut parsed = parse(&file);
    for diagnostic in &parsed.diagnostics {
        check_span(&file, diagnostic.span, &diagnostic.message);
    }
    sort_and_dedup(&file, &mut parsed.diagnostics);
    let Some(document) = &parsed.document else {
        assert_eq!(
            parsed.diagnostics.len(),
            1,
            "a parse without a document has one diagnostic"
        );
        return;
    };
    assert_eq!(document.file(), file.id());
    let check_id = |id: NodeId, what: &str| {
        assert!(
            document.node(id).is_some(),
            "{what} refers to missing node {}",
            id.index()
        );
    };
    if let Some(root) = document.root() {
        check_id(root, "the root");
    }
    for (id, node) in document.nodes() {
        let what = format!("node {}", id.index());
        check_span(&file, node.span, &what);
        check_span(
            &file,
            Span::new(file.id(), node.content_start, node.span.end),
            &format!("content of {what}"),
        );
        assert!(
            node.span.start <= node.content_start,
            "{what}: content starts at {} before the node at {}",
            node.content_start,
            node.span.start
        );
        if let Some(anchor) = &node.anchor {
            check_span(&file, anchor.span, &format!("anchor of {what}"));
        }
        match &node.kind {
            NodeKind::Scalar(_) => {}
            NodeKind::Sequence(sequence) => {
                for &item in document.items(sequence) {
                    check_id(item, &format!("an item of {what}"));
                }
            }
            NodeKind::Mapping(mapping) => {
                for entry in document.entries(mapping) {
                    check_id(entry.key, &format!("a key of {what}"));
                    check_id(entry.value, &format!("a value of {what}"));
                }
            }
            NodeKind::Alias(alias) => {
                if let Some(target) = alias.target {
                    check_id(target, &format!("alias {what}"));
                }
            }
        }
    }
    for comment in document.comments() {
        check_span(&file, comment.span, "a comment");
        if let Some(node) = comment.node {
            check_id(node, "a comment");
        }
    }
    let _ = document.dump(&file);
}

/// Checks that `span` is in `file`, does not end before it starts, lies
/// inside the text on character boundaries, and has a start and end that
/// convert to lines and columns in order.
fn check_span(file: &SourceFile, span: Span, what: &str) {
    assert_eq!(span.file, file.id(), "{what}: span is in another file");
    let range = usize::try_from(span.start).unwrap()..usize::try_from(span.end).unwrap();
    assert!(
        range.start <= range.end,
        "{what}: ends at {} before it starts at {}",
        range.end,
        range.start
    );
    assert!(
        file.text().get(range.clone()).is_some(),
        "{what}: {range:?} is outside the text or not on character boundaries"
    );
    let start = file.line_col(span.start);
    let end = file.line_col(span.end);
    let last = file.line_col(u32::try_from(file.text().len()).unwrap());
    assert!(
        start.line >= 1 && start.column >= 1,
        "{what}: starts at {start}"
    );
    assert!(
        start <= end && end <= last,
        "{what}: {start} to {end} is out of order or past the end at {last}"
    );
}
