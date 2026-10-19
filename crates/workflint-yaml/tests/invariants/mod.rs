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
use workflint_yaml::{
    Document, KeyComparison, Node, NodeId, NodeKind, SourceMap, decode, parse, simple_lowercase,
};

/// Turns fuzzer bytes into the text the checks parse, with [`decode`], as
/// the linter reads a file: UTF-8, or UTF-16 after its byte order mark.
///
/// Bytes that `decode` rejects give an error inside the input with a
/// message. The text before a UTF-8 error is valid UTF-8, and a UTF-16 error
/// is at a whole code unit. The checks then parse the input with invalid
/// UTF-8 replaced by U+FFFD, so every input reaches the parser.
pub(crate) fn text(data: &[u8]) -> Cow<'_, str> {
    let error = match decode(data) {
        Ok(text) => return text,
        Err(error) => error,
    };
    let offset = error.offset();
    assert!(!error.problem().is_empty(), "{error}: no message");
    assert!(offset <= data.len(), "{error}: past the end of the input");
    if data.starts_with(b"\xFF\xFE") || data.starts_with(b"\xFE\xFF") {
        assert!(
            offset >= 2 && offset % 2 == 0,
            "{error}: not at a UTF-16 code unit"
        );
    } else {
        let before = data.get(..offset).unwrap();
        assert!(
            std::str::from_utf8(before).is_ok(),
            "{error}: the text before it is not valid UTF-8"
        );
    }
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
/// ID in the tree refers to a node. Every scalar has an exact source map that
/// maps each decoded character, in order, to a character boundary inside the
/// scalar's span, and maps each range of decoded characters to a span
/// inside the scalar's span. The written items and entries of a collection
/// are its items and entries, with an alias in place of its target. The
/// duplicate keys of a mapping are pairs of its entries with equal keys.
/// Each node's comments are the comments attached to it.
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
            NodeKind::Scalar(scalar) => check_source_map(&file, node, &scalar.value, &what),
            NodeKind::Sequence(sequence) => {
                for &item in document.items(sequence) {
                    check_id(item, &format!("an item of {what}"));
                }
                let written = document.written_items(sequence);
                check_written(document, document.items(sequence), written, &what);
            }
            NodeKind::Mapping(mapping) => {
                for entry in document.entries(mapping) {
                    check_id(entry.key, &format!("a key of {what}"));
                    check_id(entry.value, &format!("a value of {what}"));
                }
                let flatten = |entries: &[workflint_yaml::Entry]| -> Vec<NodeId> {
                    entries.iter().flat_map(|e| [e.key, e.value]).collect()
                };
                let written = flatten(document.written_entries(mapping));
                check_written(
                    document,
                    &flatten(document.entries(mapping)),
                    &written,
                    &what,
                );
                check_duplicate_keys(document, mapping, &what);
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
    let mut attached = 0;
    for (id, _) in document.nodes() {
        for comment in document.node_comments(id) {
            assert_eq!(comment.node, Some(id), "a comment of node {}", id.index());
            attached += 1;
        }
    }
    let with_node = document.comments().iter().filter(|c| c.node.is_some());
    assert_eq!(
        attached,
        with_node.count(),
        "comments listed by their nodes"
    );
    let _ = document.dump(&file);
}

/// Checks that `written`, the children of the collection `what` as
/// written, are `children`, its children after alias resolution, with each
/// resolved alias in place of its target.
fn check_written(document: &Document, children: &[NodeId], written: &[NodeId], what: &str) {
    assert_eq!(
        written.len(),
        children.len(),
        "{what}: the written children are not the children"
    );
    for (&written, &child) in written.iter().zip(children) {
        if written == child {
            continue;
        }
        let target = match document.node(written).map(|node| &node.kind) {
            Some(NodeKind::Alias(alias)) => alias.target,
            _ => None,
        };
        assert_eq!(
            target,
            Some(child),
            "{what}: written child {} is not child {}",
            written.index(),
            child.index()
        );
    }
}

/// Checks that each duplicate key of the mapping `what` pairs two of its
/// entries, the first one earlier, with scalar keys that compare equal.
fn check_duplicate_keys(document: &Document, mapping: &workflint_yaml::Mapping, what: &str) {
    let entries = document.entries(mapping);
    let key = |id: NodeId| {
        let kind = document.node(id).map(|node| &node.kind);
        let value = match kind {
            Some(NodeKind::Scalar(scalar)) => Some(scalar.value.clone()),
            _ => None,
        };
        assert!(value.is_some(), "{what}: a duplicate key is {kind:?}");
        value.unwrap_or_default()
    };
    for comparison in [KeyComparison::Exact, KeyComparison::CaseInsensitive] {
        for duplicate in document.duplicate_keys(mapping, comparison) {
            let position = |entry| entries.iter().position(|&e| e == entry);
            let first = position(duplicate.first);
            let later = position(duplicate.duplicate);
            assert!(
                first.is_some() && later.is_some() && first < later,
                "{what}: {duplicate:?} is not two entries in order"
            );
            let (a, b) = (key(duplicate.first.key), key(duplicate.duplicate.key));
            let equal = match comparison {
                KeyComparison::Exact => a == b,
                KeyComparison::CaseInsensitive => simple_lowercase(&a) == simple_lowercase(&b),
            };
            assert!(equal, "{what}: {a:?} and {b:?} are not duplicates");
        }
    }
}

/// Checks the source map of the scalar `node` with the decoded `value`.
fn check_source_map(file: &SourceFile, node: &Node, value: &str, what: &str) {
    let map = node.source_map(file).unwrap();
    assert!(map.is_exact(), "{what}: the source map is not exact");
    let mut previous = node.span.start;
    for (offset, _) in value.char_indices() {
        let mapped = map.source_offset(offset);
        assert!(
            previous <= mapped
                && mapped < node.span.end
                && file
                    .text()
                    .is_char_boundary(usize::try_from(mapped).unwrap()),
            "{what}: decoded offset {offset} maps to {mapped}, outside {}..{} or before {previous}",
            node.span.start,
            node.span.end
        );
        previous = mapped;
    }
    let end = map.source_offset(value.len());
    assert!(
        previous <= end && end <= node.span.end,
        "{what}: the end of the value maps to {end}, outside {previous}..{}",
        node.span.end
    );
    check_source_spans(file, node, &map, value, what);
}

/// Checks that the source map of the scalar `node` maps each character of
/// `value`, the empty range at each character, and the whole value to spans
/// inside the node's span.
fn check_source_spans(
    file: &SourceFile,
    node: &Node,
    map: &SourceMap<'_>,
    value: &str,
    what: &str,
) {
    let ranges = value
        .char_indices()
        .flat_map(|(offset, ch)| [offset..offset + ch.len_utf8(), offset..offset])
        .chain([0..value.len(), value.len()..value.len()]);
    for range in ranges {
        let span = map.source_span(range.clone());
        let where_ = format!("{what}: decoded range {range:?}");
        check_span(file, span, &where_);
        assert!(
            node.span.start <= span.start && span.end <= node.span.end,
            "{where_} maps to {}..{}, outside {}..{}",
            span.start,
            span.end,
            node.span.start,
            node.span.end
        );
    }
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
