//! The YAML layer of workflint (ADR 0005).
//!
//! This is an internal crate with no stability promise. For now it exposes
//! the event parser of a vendored libyaml-safer fork, which also reports
//! comments as [`EventData::Comment`] events. Later tasks build a span tree
//! with scalar source maps on top of it.
//!
//! ```
//! use workflint_yaml::{EventData, EventParser, ScalarStyle};
//!
//! let scalars: Vec<_> = EventParser::new("a: |\n  x")
//!     .filter_map(|event| match event {
//!         Ok(event) => match event.data {
//!             EventData::Scalar { value, style, .. } => Some((value, style)),
//!             _ => None,
//!         },
//!         Err(_) => None,
//!     })
//!     .collect();
//! assert_eq!(
//!     scalars,
//!     [
//!         ("a".to_owned(), ScalarStyle::Plain),
//!         ("x".to_owned(), ScalarStyle::Literal),
//!     ],
//! );
//! ```

mod libyaml;

pub use libyaml::{
    CommentKind, Encoding, Error, ErrorKind, Event, EventData, MappingStyle, Mark,
    Parser as EventParser, ScalarStyle, SequenceStyle, TagDirective, VersionDirective,
};
