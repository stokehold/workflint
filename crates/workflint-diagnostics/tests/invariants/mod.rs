//! The check that the `render` fuzz target in `fuzz/` runs on each input.
//!
//! The fuzz target and `fuzz_regressions.rs` include this file with
//! `#[path]`, so a crash input found by the fuzzer fails the same check as
//! a test. The check panics when an invariant does not hold.

#![allow(clippy::unwrap_used, clippy::panic)]

use workflint_diagnostics::{
    Category, Color, Diagnostic, FileId, RuleMeta, Severity, SourceFile, Span, render_default,
    render_oneline, sort_and_dedup,
};

static RULE: RuleMeta = RuleMeta {
    id: "fuzz",
    kind: "syntax-check",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://example.com",
};

/// Builds a `SourceFile` from the text in `data` and renders diagnostics
/// with the spans in `data` in the default and one-line formats.
///
/// The last byte of `data` is the number of diagnostics, modulo 9. Before
/// it come eight bytes per diagnostic: the start and the end offset as
/// little-endian `u32`, each modulo the text length plus 8, so that some
/// spans reach past the end of the text. The bytes before those are the
/// text, with invalid UTF-8 replaced by U+FFFD. A YAML file as input is
/// mostly text, so the seeds from `cargo xtask fuzz-seeds` work for this
/// check too.
///
/// Rendering does not panic and writes valid UTF-8. The one-line format
/// writes one header per diagnostic, and the default format writes the same
/// header and at most one three-line snippet per diagnostic. Every
/// indicator line has one `^`, only spaces and tabs before it, and only `~`
/// and tabs after it, with no tab at the end. With color on, the output is
/// the same apart from the escape sequences. Sorting and deduplicating the
/// diagnostics does not panic.
pub(crate) fn check_render(data: &[u8]) {
    let Some((&count, rest)) = data.split_last() else {
        return;
    };
    let count = usize::from(count % 9);
    let Some(text_len) = rest.len().checked_sub(count * 8) else {
        return;
    };
    let (text, spans) = rest.split_at(text_len);
    let text = String::from_utf8_lossy(text).into_owned();
    let Ok(file) = SourceFile::new(FileId::new(0), "fuzz.yaml", text) else {
        return;
    };
    let limit = u32::try_from(file.text().len())
        .unwrap_or(u32::MAX)
        .saturating_add(8);
    let offset = |bytes: &[u8]| {
        let bytes: [u8; 4] = bytes.try_into().unwrap_or_default();
        u32::from_le_bytes(bytes) % limit
    };
    let mut diagnostics: Vec<Diagnostic> = spans
        .as_chunks::<8>()
        .0
        .iter()
        .map(|span| {
            let (start, end) = span.split_at(4);
            let span = Span::new(file.id(), offset(start), offset(end));
            Diagnostic::new(&RULE, span, "msg")
        })
        .collect();

    let oneline = render(render_oneline, &file, &diagnostics, Color::Off);
    let headers: Vec<&str> = oneline.split_inclusive('\n').collect();
    assert_eq!(headers.len(), diagnostics.len());

    let default = render(render_default, &file, &diagnostics, Color::Off);
    let mut rest = default.as_str();
    for header in headers {
        rest = rest.strip_prefix(header).unwrap();
        // A snippet is three lines, the first of which starts with spaces. A
        // header starts with the path.
        if rest.starts_with(' ') {
            let mut lines = rest.splitn(4, '\n');
            let (Some(_), Some(_), Some(indicator)) = (lines.next(), lines.next(), lines.next())
            else {
                panic!("incomplete snippet: {rest:?}");
            };
            check_indicator(indicator);
            rest = lines.next().unwrap_or("");
        }
    }
    assert_eq!(rest, "");

    // Color adds escape sequences and changes nothing else.
    let colored = render(render_oneline, &file, &diagnostics, Color::On);
    assert_eq!(strip_colors(&colored), strip_colors(&oneline));
    let colored = render(render_default, &file, &diagnostics, Color::On);
    assert_eq!(strip_colors(&colored), strip_colors(&default));

    sort_and_dedup(&file, &mut diagnostics);
}

type Renderer = fn(&mut Vec<u8>, &SourceFile, &[Diagnostic], Color) -> std::io::Result<()>;

/// Renders `diagnostics` with `renderer` and checks that the output is UTF-8.
fn render(
    renderer: Renderer,
    file: &SourceFile,
    diagnostics: &[Diagnostic],
    color: Color,
) -> String {
    let mut out = Vec::new();
    renderer(&mut out, file, diagnostics, color).unwrap();
    String::from_utf8(out).unwrap()
}

/// Removes every `ESC [ <digits> m` sequence. The source text can hold such
/// sequences too, so both sides of a comparison go through this function.
fn strip_colors(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("\u{1b}[") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        let digits = after.len() - after.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        if after[digits..].starts_with('m') {
            rest = &after[digits + 1..];
        } else {
            out.push_str("\u{1b}[");
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// Checks the indicator line of a plain snippet, without its line break.
fn check_indicator(line: &str) {
    let (_, indicator) = line.split_once("| ").unwrap();
    assert_eq!(indicator.matches('^').count(), 1, "{indicator:?}");
    let (before, after) = indicator.split_once('^').unwrap();
    assert!(
        before.chars().all(|c| c == ' ' || c == '\t'),
        "{indicator:?}"
    );
    assert!(
        after.chars().all(|c| c == '~' || c == '\t'),
        "{indicator:?}"
    );
    assert!(!after.ends_with('\t'), "{indicator:?}");
}
