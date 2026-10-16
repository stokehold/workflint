//! The checks that the `syntax` fuzz target in `fuzz/` runs on each input.
//!
//! The fuzz target includes this file with `#[path]`, and
//! `reference_files.rs` runs it on files, so a crash input found by the
//! fuzzer fails the same check as a test. Each check panics when an
//! invariant does not hold.

#![allow(clippy::unwrap_used)]

use workflint_diagnostics::{FileId, SourceFile, sort_and_dedup};

/// Parses `text` with the YAML layer and, if it gives a document, with the
/// workflow parser.
///
/// The parser does not panic, and every diagnostic belongs to the file, has
/// a span inside `text` on character boundaries, and converts to a line and
/// column. Sorting and deduplicating the diagnostics does not panic.
pub(crate) fn check_parse(text: &str) {
    let Ok(file) = SourceFile::new(FileId::new(0), "fuzz.yml", text) else {
        return;
    };
    let Some(document) = workflint_yaml::parse(&file).document else {
        return;
    };
    let mut diagnostics = workflint_syntax::parse(&file, &document).diagnostics;
    for diagnostic in &diagnostics {
        let span = diagnostic.span;
        let what = format!("{span:?} of {:?}", diagnostic.message);
        assert_eq!(span.file, file.id(), "{what}");
        assert!(span.start <= span.end, "{what}");
        let (start, end) = (
            usize::try_from(span.start).unwrap(),
            usize::try_from(span.end).unwrap(),
        );
        assert!(end <= text.len(), "{what}");
        assert!(text.is_char_boundary(start), "{what}");
        assert!(text.is_char_boundary(end), "{what}");
        let pos = file.line_col(span.start);
        assert!(pos.line >= 1 && pos.column >= 1, "{what}");
    }
    sort_and_dedup(&file, &mut diagnostics);
}
