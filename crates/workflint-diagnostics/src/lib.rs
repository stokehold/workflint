//! The types every workflint crate uses to report a problem.
//!
//! A [`Diagnostic`] names the [`RuleMeta`] that produced it and points at a [`Span`], a byte
//! range in a [`SourceFile`]. The source file's line index turns byte offsets into the 1-based
//! lines and code-point columns that output shows. [`sort_and_dedup`] puts the diagnostics of
//! one file in output order, and the [`quote`](mod@quote) module formats values the way
//! actionlint's messages do.
//!
//! This is an internal crate with no stability promise.

mod diagnostic;
pub mod quote;
mod rule;
mod source;
mod span;

pub use diagnostic::{Applicability, Diagnostic, Fix, Label, TextEdit, sort_and_dedup};
pub use rule::{Category, RuleMeta, Severity};
pub use source::{FileId, LineCol, SourceFile, SourceTooLarge};
pub use span::Span;
