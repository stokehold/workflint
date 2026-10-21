//! The types every workflint crate uses to report a problem.
//!
//! A [`Diagnostic`] names the [`RuleMeta`] that produced it and points at a [`Span`], a byte
//! range in a [`SourceFile`]. The source file's line index turns byte offsets into the 1-based
//! lines and code-point columns that output shows. [`sort_and_dedup`] puts the diagnostics of
//! one file in output order, and the [`quote`](mod@quote) module formats values the way
//! actionlint's messages do. [`render_default`] and [`render_oneline`] write diagnostics in
//! actionlint's default and `-oneline` output formats, and [`render_json`] and [`render_jsonl`]
//! write workflint's JSON formats.
//!
//! This is an internal crate with no stability promise.

mod diagnostic;
mod json;
pub mod quote;
mod render;
mod rule;
mod source;
mod span;

pub use diagnostic::{Applicability, Diagnostic, Fix, Label, TextEdit, sort_and_dedup};
pub use json::{JSON_VERSION, JsonPosition, JsonRecord, json_records, render_json, render_jsonl};
pub use render::{Color, Snippet, render_default, render_oneline, snippet};
pub use rule::{Category, RuleMeta, Severity};
pub use source::{FileId, LineCol, SourceFile, SourceTooLarge};
pub use span::Span;
