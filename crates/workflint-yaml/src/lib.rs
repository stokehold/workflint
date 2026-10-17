//! The YAML layer of workflint (ADR 0005).
//!
//! This is an internal crate with no stability promise. [`parse`] turns the
//! first YAML document of a [`SourceFile`](workflint_diagnostics::SourceFile)
//! into a [`Document`]: an arena of nodes with byte spans, tags, styles,
//! anchors, decoded scalar values, resolved aliases, and attached comments.
//! YAML syntax errors, unknown anchors, recursive aliases, and unused anchors
//! become diagnostics of the rules in [`rules`]. [`Node::source_map`] maps
//! offsets in a decoded scalar value back to the source.
//!
//! ```
//! use workflint_diagnostics::{FileId, SourceFile};
//! use workflint_yaml::{NodeKind, parse};
//!
//! let file = SourceFile::new(FileId::new(0), "ci.yml", "on: &a push\nx: *a\n").unwrap();
//! let parsed = parse(&file);
//! assert!(parsed.diagnostics.is_empty());
//!
//! let document = parsed.document.unwrap();
//! let root = document.node(document.root().unwrap()).unwrap();
//! let NodeKind::Mapping(mapping) = &root.kind else { panic!() };
//! let entries = document.entries(mapping);
//! // The alias `*a` is replaced by the anchored node.
//! assert_eq!(entries[0].value, entries[1].value);
//! ```
//!
//! The [`event`] module exposes the event parser of the vendored
//! libyaml-safer fork that the tree is built from. [`decode()`] turns the bytes
//! of a file into the text that both parse, as go-yaml decodes UTF-8 and
//! UTF-16.

mod attach;
mod build;
mod decode;
mod document;
mod dump;
mod libyaml;
mod resolve;
pub mod rules;
mod source_map;

pub use build::{Parsed, parse};
pub use decode::{DecodeError, decode};
pub use document::{
    Alias, Anchor, BlockHeader, Chomping, CollectionStyle, Comment, Document, DuplicateKey, Entry,
    KeyComparison, Mapping, Node, NodeId, NodeKind, Scalar, ScalarStyle, Sequence, Tag,
    simple_lowercase,
};
pub use libyaml::CommentKind;
pub use source_map::SourceMap;

/// The event parser of the vendored libyaml-safer fork, which also reports
/// comments as [`EventData::Comment`](event::EventData::Comment) events.
///
/// ```
/// use workflint_yaml::event::{EventData, EventParser, ScalarStyle};
///
/// let scalars: Vec<_> = EventParser::new("a: |\n  x")
///     .filter_map(|event| match event {
///         Ok(event) => match event.data {
///             EventData::Scalar { value, style, .. } => Some((value, style)),
///             _ => None,
///         },
///         Err(_) => None,
///     })
///     .collect();
/// assert_eq!(
///     scalars,
///     [
///         ("a".to_owned(), ScalarStyle::Plain),
///         ("x".to_owned(), ScalarStyle::Literal),
///     ],
/// );
/// ```
pub mod event {
    pub use crate::libyaml::{
        CommentKind, Encoding, Error, ErrorKind, Event, EventData, MappingStyle, Mark, NodeMarks,
        Parser as EventParser, ScalarStyle, SequenceStyle, TagDirective, VersionDirective,
    };
}
