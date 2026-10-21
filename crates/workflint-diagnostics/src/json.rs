//! The `json` and `jsonl` output formats.
//!
//! Both print [`JsonRecord`]s, which [`json_records`] builds from the diagnostics of a file.
//! `json` prints one document, `{"version":1,"diagnostics":[...]}`, and `jsonl` prints one
//! record per line, each with the `version` key first. `docs/json-output.schema.json` is the
//! schema, and `docs/json-output.md` describes it. The output is compact, and every line ends
//! with a line break.

use std::io::{self, Write};

use serde::Serialize;

use crate::{Diagnostic, SourceFile};

/// The version of the schema. It changes only when a key is removed, renamed, or changes its
/// meaning. Adding a key keeps the version.
pub const JSON_VERSION: u32 = 1;

/// One diagnostic as the JSON formats print it, with its positions worked out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JsonRecord {
    /// The rule ID.
    pub rule: &'static str,
    /// The actionlint kind of the rule.
    pub kind: &'static str,
    /// `error`, `warning`, or `info`.
    pub severity: &'static str,
    /// The message.
    pub message: String,
    /// The help text, which prints as `null` when there is none.
    pub help: Option<Box<str>>,
    /// The path of the file, as the default format prints it.
    pub path: String,
    /// The position of the first byte of the span.
    pub start: JsonPosition,
    /// The position just after the last byte of the span.
    pub end: JsonPosition,
}

/// A position in a file: a 1-based line, a 1-based column in code points, and a 0-based byte
/// offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct JsonPosition {
    /// The 1-based line.
    pub line: usize,
    /// The 1-based column, counted in Unicode code points.
    pub column: usize,
    /// The offset in bytes from the start of the file.
    pub offset: u32,
}

/// The records of the diagnostics of `file`, in the given order.
///
/// Every diagnostic's span must be in `file`.
#[must_use]
pub fn json_records(file: &SourceFile, diagnostics: &[Diagnostic]) -> Vec<JsonRecord> {
    let position = |offset: u32| {
        let pos = file.line_col(offset);
        JsonPosition {
            line: pos.line,
            column: pos.column,
            offset,
        }
    };
    diagnostics
        .iter()
        .map(|diagnostic| JsonRecord {
            rule: diagnostic.rule.id,
            kind: diagnostic.rule.kind,
            severity: diagnostic.severity.as_str(),
            message: diagnostic.message.clone(),
            help: diagnostic.help.clone(),
            path: file.path().to_owned(),
            start: position(diagnostic.span.start),
            end: position(diagnostic.span.end),
        })
        .collect()
}

/// Writes `records` as one JSON document and a line break.
///
/// # Errors
///
/// Returns the first error that `out` returns.
pub fn render_json<W: Write + ?Sized>(out: &mut W, records: &[JsonRecord]) -> io::Result<()> {
    #[derive(Serialize)]
    struct Document<'a> {
        version: u32,
        diagnostics: &'a [JsonRecord],
    }
    serde_json::to_writer(
        &mut *out,
        &Document {
            version: JSON_VERSION,
            diagnostics: records,
        },
    )?;
    out.write_all(b"\n")
}

/// Writes each of `records` as a JSON object on its own line, with the `version` key first.
///
/// # Errors
///
/// Returns the first error that `out` returns.
pub fn render_jsonl<W: Write + ?Sized>(out: &mut W, records: &[JsonRecord]) -> io::Result<()> {
    #[derive(Serialize)]
    struct Line<'a> {
        version: u32,
        #[serde(flatten)]
        record: &'a JsonRecord,
    }
    for record in records {
        serde_json::to_writer(
            &mut *out,
            &Line {
                version: JSON_VERSION,
                record,
            },
        )?;
        out.write_all(b"\n")?;
    }
    Ok(())
}
