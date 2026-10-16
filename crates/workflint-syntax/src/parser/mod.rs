//! The workflow parser: a recursive-descent walk over the span tree with one
//! function per production, as in the reference's `parse.go`.
//!
//! No error stops the walk. Each production reports its error, returns an
//! absent or invalid value, and its parent goes on (research 3.7).
//! Diagnostics come out in walk order, and the caller sorts and
//! deduplicates them.
//!
//! The walk follows resolved aliases, so a subtree used through several
//! aliases is parsed once per use, as in the reference, and the same error
//! is reported once per use at the anchored node. Deduplication then keeps
//! one. Before the walk, [`parse`] counts the YAML nodes of the document
//! with its aliases expanded, and walks nothing if there are more than
//! GitHub accepts. So a "billion laughs" input cannot make the walk
//! exponential.

mod container;
mod event;
mod job;
mod leaf;
mod mapping;
mod shared;
mod workflow;

use workflint_diagnostics::{Diagnostic, RuleMeta, SourceFile, Span};
use workflint_yaml::{Document, Node, NodeId, NodeKind, Tag};

use crate::Workflow;

pub(crate) use mapping::{Key, KeySpec, Label, MapEntry, Section};

/// The result of [`parse`].
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed<'a> {
    /// The workflow. It is never missing: a file with no document or with
    /// a root that is not a mapping gives a workflow with every section
    /// absent, as in the reference.
    pub workflow: Workflow<'a>,
    /// The diagnostics of the parser, in no particular order. Sort them with
    /// [`sort_and_dedup`](workflint_diagnostics::sort_and_dedup).
    pub diagnostics: Vec<Diagnostic>,
}

/// Parses the span tree of `file` into a workflow.
///
/// The diagnostics are the reference's generic `syntax-check` errors, with
/// byte-identical messages. They do not include the YAML layer's
/// diagnostics, which come with `document`.
///
/// ```
/// use workflint_diagnostics::{FileId, SourceFile};
///
/// let text = "name: CI\non: push\njobs: {}\nrun_name: x\n";
/// let file = SourceFile::new(FileId::new(0), "ci.yml", text).unwrap();
/// let document = workflint_yaml::parse(&file).document.unwrap();
///
/// let parsed = workflint_syntax::parse(&file, &document);
/// assert_eq!(parsed.workflow.name.value().map(|name| name.value()), Some("CI"));
/// assert!(parsed.diagnostics.iter().any(|diagnostic| diagnostic.message
///     == r#"unexpected key "run_name" for "workflow" section. expected one of "concurrency", "defaults", "env", "jobs", "name", "on", "permissions", "run-name""#));
/// ```
#[must_use]
pub fn parse<'a>(file: &SourceFile, document: &'a Document) -> Parsed<'a> {
    let mut parser = Parser::new(file, document);
    let workflow = parser.workflow();
    Parsed {
        workflow,
        diagnostics: parser.into_diagnostics(),
    }
}

/// The state of one parse: the file, its span tree, and the diagnostics so
/// far.
pub(crate) struct Parser<'a, 'f> {
    file: &'f SourceFile,
    document: &'a Document,
    diagnostics: Vec<Diagnostic>,
}

impl<'a, 'f> Parser<'a, 'f> {
    pub(crate) fn new(file: &'f SourceFile, document: &'a Document) -> Self {
        Self {
            file,
            document,
            diagnostics: Vec::new(),
        }
    }

    /// Returns the node `id` of the document. Every production gets its
    /// child nodes through this function.
    pub(crate) fn node(&self, id: NodeId) -> Option<&'a Node> {
        self.document.node(id)
    }

    /// Returns the diagnostics reported so far.
    pub(crate) fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    /// Reports `message` at `span`.
    pub(crate) fn error(&mut self, rule: &'static RuleMeta, span: Span, message: String) {
        self.diagnostics.push(Diagnostic::new(rule, span, message));
    }

    /// Formats the start of `span` as the reference's `Pos.String()`, such
    /// as `line:3,col:5`.
    pub(crate) fn position(&self, span: Span) -> String {
        let pos = self.file.line_col(span.start);
        format!("line:{},col:{}", pos.line, pos.column)
    }
}

/// The most YAML nodes that GitHub reads in a workflow, `c_maxYamlNodes` of
/// `YamlObjectReader` in `actions/runner`. Past it GitHub rejects the file
/// with `Maximum YAML nodes exceeded`.
pub(crate) const MAX_YAML_NODES: u64 = 50_000;

/// Returns how many parsing events GitHub's `YamlObjectReader` reads for
/// `document`, or `cap` if that is more.
///
/// The reader counts every event it moves to: the stream start, the
/// document start, each scalar, the start and end of each collection, the
/// document end, and the stream end. It reads an alias by replaying the
/// events of its anchored node, so each use of an alias counts that node's
/// events again. The count takes time in proportion to the number of nodes,
/// however many times aliases expand them.
///
/// GitHub replays a recursive alias until the limit stops it. The YAML
/// layer reports a recursive alias already, so it counts as one event here.
pub(crate) fn yaml_node_count(document: &Document, cap: u64) -> u64 {
    let Some(root) = document.root() else {
        return 0;
    };
    // The events of each node's subtree with aliases expanded, once known.
    let mut events: Vec<Option<u64>> = vec![None; document.nodes().count()];
    let index = |id: NodeId| usize::try_from(id.index()).ok();
    let children = |id: NodeId| -> Vec<NodeId> {
        match document.node(id).map(|node| &node.kind) {
            Some(NodeKind::Sequence(sequence)) => document.items(sequence).to_vec(),
            Some(NodeKind::Mapping(mapping)) => document
                .entries(mapping)
                .iter()
                .flat_map(|entry| [entry.key, entry.value])
                .collect(),
            _ => Vec::new(),
        }
    };
    // A depth-first walk that visits a node's children before the node. The
    // tree has no cycles, because a recursive alias stays an alias node.
    let mut stack = vec![(root, false)];
    while let Some((id, children_done)) = stack.pop() {
        let Some(slot) = index(id).filter(|&i| i < events.len()) else {
            continue;
        };
        if events[slot].is_some() {
            continue;
        }
        let collection = matches!(
            document.node(id).map(|node| &node.kind),
            Some(NodeKind::Sequence(_) | NodeKind::Mapping(_))
        );
        if !collection {
            events[slot] = Some(1);
        } else if children_done {
            let total = children(id)
                .into_iter()
                .filter_map(|child| events.get(index(child)?).copied().flatten())
                .fold(2u64, u64::saturating_add);
            events[slot] = Some(total.min(cap));
        } else {
            stack.push((id, true));
            stack.extend(children(id).into_iter().map(|child| (child, false)));
        }
    }
    let root_events = index(root)
        .and_then(|i| events.get(i).copied().flatten())
        .unwrap_or(0);
    // The stream and document start and end events.
    root_events.saturating_add(4).min(cap)
}

/// The reference's `nodeKindName`.
pub(crate) fn kind_name(node: &Node) -> &'static str {
    match node.kind {
        NodeKind::Scalar(_) => "scalar",
        NodeKind::Sequence(_) => "sequence",
        NodeKind::Mapping(_) => "mapping",
        NodeKind::Alias(_) => "alias",
    }
}

/// The tag of `node` as the reference prints it, which is empty for an
/// alias.
pub(crate) fn tag_name(node: &Node) -> &str {
    node.tag().map_or("", Tag::as_str)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use workflint_diagnostics::FileId;

    use super::*;
    use crate::testing::laughs;

    fn count(text: &str) -> u64 {
        let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();
        let document = workflint_yaml::parse(&file).document.unwrap();
        yaml_node_count(&document, u64::MAX)
    }

    #[test]
    fn the_count_has_an_event_per_scalar_and_two_per_collection() {
        // The stream and document events, the mapping's start and end, and
        // two scalars.
        assert_eq!(count("a: b\n"), 8);
        assert_eq!(count("[a, [b, c], {d: e}]\n"), 4 + 2 + 1 + 4 + 4);
        assert_eq!(count("---\n"), 5);
    }

    #[test]
    fn each_use_of_an_alias_counts_its_node_again() {
        // `x` and `y` are one event each, and `[a, b]` is four events at
        // both of its uses.
        assert_eq!(count("x: &x [a, b]\ny: *x\n"), 4 + 2 + 1 + 4 + 1 + 4);
    }

    #[test]
    fn a_recursive_alias_counts_once() {
        assert_eq!(count("a: &a [*a]\n"), 4 + 2 + 1 + 3);
    }

    #[test]
    fn the_count_stops_at_the_cap() {
        let text = laughs(20, "x");
        let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();
        let document = workflint_yaml::parse(&file).document.unwrap();
        assert_eq!(
            yaml_node_count(&document, MAX_YAML_NODES + 1),
            MAX_YAML_NODES + 1
        );
        assert_eq!(yaml_node_count(&document, u64::MAX), u64::MAX);
    }

    #[test]
    fn a_file_with_no_document_has_no_events() {
        assert_eq!(
            yaml_node_count(
                &workflint_yaml::parse(&SourceFile::new(FileId::new(0), "t.yml", "").unwrap())
                    .document
                    .unwrap(),
                u64::MAX
            ),
            0
        );
    }
}
