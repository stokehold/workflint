//! Helpers that parse YAML for the unit tests, which need real nodes for
//! their string leaves.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fmt::Write as _;

use workflint_diagnostics::{Diagnostic, FileId, SourceFile, sort_and_dedup};
use workflint_yaml::{Document, Node, NodeKind, parse};

use crate::parser::Parser;
use crate::{EnvVar, Str, Template};

/// The text of a string leaf, as the tests print it.
pub(crate) trait Text {
    /// Returns the decoded value, or `-` for a value that is not a string.
    fn text(&self) -> &str;
}

impl Text for Str<'_> {
    fn text(&self) -> &str {
        self.value()
    }
}

impl Text for Template<'_> {
    fn text(&self) -> &str {
        self.value()
    }
}

impl Text for EnvVar<'_> {
    fn text(&self) -> &str {
        self.value.text()
    }
}

impl<T: Text> Text for Option<T> {
    fn text(&self) -> &str {
        self.as_ref().map_or("-", Text::text)
    }
}

/// Parses `text` as the YAML of a file with ID 0.
pub(crate) fn document(text: &str) -> Document {
    let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();
    parse(&file).document.unwrap()
}

/// Returns the keys and values of the root mapping of `document`, which must
/// all be scalars.
pub(crate) fn root_entries(document: &Document) -> Vec<(Str<'_>, Str<'_>)> {
    let root = document.node(document.root().unwrap()).unwrap();
    let NodeKind::Mapping(mapping) = &root.kind else {
        panic!("the root is not a mapping");
    };
    document
        .entries(mapping)
        .iter()
        .map(|entry| {
            let key = Str::new(document.node(entry.key).unwrap()).unwrap();
            let value = Str::new(document.node(entry.value).unwrap()).unwrap();
            (key, value)
        })
        .collect()
}

/// Returns the node at `path` from the root of `document`. A step of the
/// path is a mapping key or, for a sequence, an item index.
pub(crate) fn node_at<'a>(document: &'a Document, path: &[&str]) -> &'a Node {
    let mut node = document.node(document.root().unwrap()).unwrap();
    for step in path {
        let id = match &node.kind {
            NodeKind::Mapping(mapping) => {
                document
                    .entries(mapping)
                    .iter()
                    .find(|entry| {
                        Str::new(document.node(entry.key).unwrap()).unwrap().value() == *step
                    })
                    .unwrap()
                    .value
            }
            NodeKind::Sequence(sequence) => {
                document.items(sequence)[step.parse::<usize>().unwrap()]
            }
            _ => panic!("no child {step}"),
        };
        node = document.node(id).unwrap();
    }
    node
}

/// Returns the key node of the last step of `path`, which must end at a
/// mapping entry.
pub(crate) fn key_at<'a>(document: &'a Document, path: &[&str]) -> Str<'a> {
    let (last, parent) = path.split_last().unwrap();
    let NodeKind::Mapping(mapping) = &node_at(document, parent).kind else {
        panic!("not a mapping");
    };
    document
        .entries(mapping)
        .iter()
        .map(|entry| Str::new(document.node(entry.key).unwrap()).unwrap())
        .find(|key| key.value() == *last)
        .unwrap()
}

/// Returns the scalar at `path`.
pub(crate) fn str_at<'a>(document: &'a Document, path: &[&str]) -> Str<'a> {
    Str::new(node_at(document, path)).unwrap()
}

/// Renders `diagnostics`, sorted and deduplicated, one per line as
/// `line:column: message [rule-id]`.
pub(crate) fn render(file: &SourceFile, mut diagnostics: Vec<Diagnostic>) -> Vec<String> {
    sort_and_dedup(file, &mut diagnostics);
    diagnostics
        .iter()
        .map(|diagnostic| {
            let pos = file.line_col(diagnostic.span.start);
            format!(
                "{}:{}: {} [{}]",
                pos.line, pos.column, diagnostic.message, diagnostic.rule.id
            )
        })
        .collect()
}

/// Parses `text` as YAML, runs `f` with a parser and the root node, and
/// returns the parser's diagnostics as [`render`] prints them, followed by
/// the lines that `f` returns.
pub(crate) fn parse_with(
    text: &str,
    f: impl for<'a, 'f> FnOnce(&mut Parser<'a, 'f>, &'a Node) -> Vec<String>,
) -> Vec<String> {
    let file = SourceFile::new(FileId::new(0), "test.yml", text).unwrap();
    let document = parse(&file).document.unwrap();
    let mut parser = Parser::new(&file, &document);
    let root = parser.node(document.root().unwrap()).unwrap();
    let lines = f(&mut parser, root);
    let mut out = render(&file, parser.into_diagnostics());
    out.extend(lines);
    out
}

/// A "billion laughs": `levels` anchored sequences, each with ten aliases of
/// the one before, which expand to 10^`levels` copies of `item`.
pub(crate) fn laughs(levels: usize, item: &str) -> String {
    let mut text = format!("l0: &l0 [{}]\n", [item; 10].join(", "));
    for level in 1..levels {
        let items = vec![format!("*l{}", level - 1); 10].join(", ");
        let _ = writeln!(text, "l{level}: &l{level} [{items}]");
    }
    text
}
