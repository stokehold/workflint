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
//! one. To keep a "billion laughs" input from making the walk exponential,
//! [`Parser::node`] counts the nodes the walk enters and stops it once
//! aliases account for too many of them, by go-yaml's rule for decoding
//! aliases.

mod leaf;
mod mapping;
mod workflow;

use workflint_diagnostics::{Diagnostic, RuleMeta, SourceFile, Span};
use workflint_yaml::{Document, Node, NodeId, NodeKind, Tag};

use crate::Workflow;
use crate::rules::EXCESSIVE_ALIASING;

pub(crate) use mapping::{KeySpec, Label, MapEntry, Section};

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

/// The state of one parse: the diagnostics so far and the count of nodes
/// the walk has entered.
pub(crate) struct Parser<'a, 'f> {
    file: &'f SourceFile,
    document: &'a Document,
    diagnostics: Vec<Diagnostic>,
    /// Whether the walk has entered each node.
    entered: Vec<bool>,
    /// Every node the walk has entered, counting each use of a node.
    visits: u64,
    /// The visits to a node that the walk had entered before, which happen
    /// only through aliases.
    alias_visits: u64,
    /// Whether the walk has stopped because of excessive aliasing.
    stopped: bool,
}

impl<'a, 'f> Parser<'a, 'f> {
    pub(crate) fn new(file: &'f SourceFile, document: &'a Document) -> Self {
        Self {
            file,
            document,
            diagnostics: Vec::new(),
            entered: vec![false; document.nodes().count()],
            visits: 0,
            alias_visits: 0,
            stopped: false,
        }
    }

    /// Enters the node `id` and returns it, or returns `None` if the walk
    /// has stopped.
    ///
    /// Every production gets its child nodes through this function. It
    /// stops the walk with an `excessive-aliasing` diagnostic as go-yaml's
    /// decoder stops with `document contains excessive aliasing`: when more
    /// than 100 visits came through aliases, more than 1000 visits happened,
    /// and the share of alias visits is above go-yaml's limit (see
    /// [`exceeds_alias_share`]). From
    /// then on it returns `None` and the parser reports nothing else, so
    /// the work is linear in the size of the document.
    pub(crate) fn node(&mut self, id: NodeId) -> Option<&'a Node> {
        if self.stopped {
            return None;
        }
        let node = self.document.node(id)?;
        let entered = usize::try_from(id.index())
            .ok()
            .and_then(|index| self.entered.get_mut(index))
            .is_some_and(|entered| std::mem::replace(entered, true));
        self.visits += 1;
        if entered {
            self.alias_visits += 1;
        }
        if self.alias_visits > 100
            && self.visits > 1000
            && exceeds_alias_share(self.alias_visits, self.visits)
        {
            self.diagnostics.push(Diagnostic::new(
                &EXCESSIVE_ALIASING,
                node.span,
                "document contains excessive aliasing. the parts of the workflow that aliases \
                 lead to from here on are not checked",
            ));
            self.stopped = true;
            return None;
        }
        Some(node)
    }

    /// Returns the diagnostics reported so far.
    pub(crate) fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    /// Reports `message` at `span`, unless the walk has stopped.
    pub(crate) fn error(&mut self, rule: &'static RuleMeta, span: Span, message: String) {
        if !self.stopped {
            self.diagnostics.push(Diagnostic::new(rule, span, message));
        }
    }

    /// Formats the start of `span` as the reference's `Pos.String()`, such
    /// as `line:3,col:5`.
    pub(crate) fn position(&self, span: Span) -> String {
        let pos = self.file.line_col(span.start);
        format!("line:{},col:{}", pos.line, pos.column)
    }
}

/// Returns whether `alias_visits` out of `visits` is more than go-yaml's
/// `allowedAliasRatio(visits)`: 99% up to 400,000 visits, 10% from
/// 4,000,000 visits, and a linear scale in between.
fn exceeds_alias_share(alias_visits: u64, visits: u64) -> bool {
    const LOW: u64 = 400_000;
    const HIGH: u64 = 4_000_000;
    let (alias_visits, visits) = (u128::from(alias_visits), u128::from(visits));
    if visits <= u128::from(LOW) {
        // alias_visits / visits > 0.99
        alias_visits * 100 > visits * 99
    } else if visits >= u128::from(HIGH) {
        alias_visits * 10 > visits
    } else {
        // alias_visits / visits > 0.99 - 0.89 * (visits - LOW) / (HIGH - LOW)
        let range = u128::from(HIGH - LOW);
        let allowed = 99 * range - 89 * (visits - u128::from(LOW));
        alias_visits * 100 * range > allowed * visits
    }
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

    use std::fmt::Write as _;

    use workflint_diagnostics::FileId;

    use super::*;
    use crate::testing::render;

    /// Enters every node under `node`, reporting each empty string, as a
    /// production that reads arbitrary YAML would.
    fn walk_all<'a>(parser: &mut Parser<'a, '_>, node: &'a Node) {
        match &node.kind {
            NodeKind::Mapping(_) => {
                for entry in parser.parse_mapping(Label::Section("x"), node, true, false) {
                    walk_all(parser, entry.value);
                }
            }
            NodeKind::Sequence(_) => {
                for &id in parser.check_sequence("x", node, true).unwrap_or_default() {
                    if let Some(item) = parser.node(id) {
                        walk_all(parser, item);
                    }
                }
            }
            NodeKind::Scalar(_) | NodeKind::Alias(_) => {
                parser.check_string(node, false);
            }
        }
    }

    /// Nine levels of ten aliases each: 10^9 empty strings once expanded.
    fn laughs(levels: usize) -> String {
        let mut text = format!("l0: &l0 [{}]\n", ["''"; 10].join(", "));
        for level in 1..levels {
            let items = vec![format!("*l{}", level - 1); 10].join(", ");
            let _ = writeln!(text, "l{level}: &l{level} [{items}]");
        }
        text
    }

    /// Walks all of `text` and returns the parser's counts and diagnostics.
    fn walk(text: &str) -> (u64, usize, Vec<String>) {
        let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();
        let document = workflint_yaml::parse(&file).document.unwrap();
        let mut parser = Parser::new(&file, &document);
        let root = parser.node(document.root().unwrap()).unwrap();
        walk_all(&mut parser, root);
        let visits = parser.visits;
        let nodes = document.nodes().count();
        (visits, nodes, render(&file, parser.into_diagnostics()))
    }

    #[test]
    fn a_billion_laughs_stops_the_walk_after_linear_work() {
        let (visits, nodes, diagnostics) = walk(&laughs(9));

        let nodes = u64::try_from(nodes).unwrap();
        assert!(
            visits <= 100 * nodes + 1000,
            "{visits} visits for {nodes} nodes"
        );
        let stops: Vec<_> = diagnostics
            .iter()
            .filter(|line| line.ends_with("[excessive-aliasing]"))
            .collect();
        assert_eq!(stops.len(), 1, "{diagnostics:?}");
        assert!(
            stops[0].contains("document contains excessive aliasing"),
            "{stops:?}"
        );
        // Each empty string is reported once per use, at the same place, so
        // deduplication leaves the ten in `l0`.
        assert_eq!(diagnostics.len(), 11, "{diagnostics:?}");
    }

    #[test]
    fn an_anchor_used_a_few_times_is_walked_at_every_use() {
        // Twelve uses of a step list, as in the reference's
        // `err/dedup_errors` fixture.
        let mut text = "steps: &steps [{run: ''}, {run: x}]\n".to_owned();
        for job in 0..12 {
            let _ = writeln!(text, "job{job}: *steps");
        }
        let (visits, nodes, diagnostics) = walk(&text);

        assert_eq!(
            diagnostics,
            ["1:22: string should not be empty [empty-string]"]
        );
        assert!(visits > u64::try_from(nodes).unwrap());
    }

    #[test]
    fn the_alias_share_follows_go_yaml() {
        // Up to 400,000 visits, 99% may come through aliases.
        assert!(!exceeds_alias_share(990, 1000));
        assert!(exceeds_alias_share(991, 1000));
        // Halfway through the range, 54.5% may.
        assert!(!exceeds_alias_share(1_199_000, 2_200_000));
        assert!(exceeds_alias_share(1_199_001, 2_200_000));
        // From 4,000,000 visits, 10% may.
        assert!(!exceeds_alias_share(400_000, 4_000_000));
        assert!(exceeds_alias_share(400_001, 4_000_000));
    }
}
