//! Helpers that parse YAML for the unit tests, which need real nodes for
//! their string leaves.

#![allow(clippy::unwrap_used, clippy::panic)]

use workflint_diagnostics::{FileId, SourceFile};
use workflint_yaml::{Document, Node, NodeKind, parse};

use crate::Str;

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
