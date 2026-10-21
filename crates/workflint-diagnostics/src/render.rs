//! The default and one-line output formats.
//!
//! Both write actionlint's layout. The default format prints a header line per diagnostic and
//! then a snippet of the source line with an indicator under the span:
//!
//! ```text
//! test.yaml:4:14: label "ubuntu-lates" is unknown [runner-label]
//!   |
//! 4 |     runs-on: ubuntu-lates
//!   |              ^~~~~~~~~~~~
//! ```
//!
//! The one-line format prints only the header line. With [`Color::On`], both write the same
//! escape sequences as actionlint. The caller decides whether to use color, and the renderers
//! read no environment variables, so the output depends only on the arguments.

use std::fmt::Display;
use std::io::{self, Write};

use unicode_width::UnicodeWidthStr;

use crate::{Diagnostic, SourceFile};

/// Whether a renderer writes ANSI color escape sequences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Color {
    /// Write the escape sequences actionlint writes with `-color`.
    On,
    /// Write plain text.
    Off,
}

const YELLOW: &str = "\x1b[33m";
const GRAY: &str = "\x1b[90m";
const BOLD: &str = "\x1b[1m";
const GREEN: &str = "\x1b[32m";
const RESET: &str = "\x1b[0m";

/// Writes `diagnostics` in the default format: a header line and, when the source line exists, a
/// snippet with an indicator.
///
/// Every diagnostic's span must be in `file`. The diagnostics are written in the given order.
///
/// # Errors
///
/// Returns the first error that `out` returns.
pub fn render_default<W: Write + ?Sized>(
    out: &mut W,
    file: &SourceFile,
    diagnostics: &[Diagnostic],
    color: Color,
) -> io::Result<()> {
    for diagnostic in diagnostics {
        write_header(out, file, diagnostic, color)?;
        write_snippet(out, file, diagnostic, color)?;
    }
    Ok(())
}

/// Writes `diagnostics` in the one-line format: `path:line:col: message [rule-id]` and a newline
/// each.
///
/// Every diagnostic's span must be in `file`. The diagnostics are written in the given order.
///
/// # Errors
///
/// Returns the first error that `out` returns.
pub fn render_oneline<W: Write + ?Sized>(
    out: &mut W,
    file: &SourceFile,
    diagnostics: &[Diagnostic],
    color: Color,
) -> io::Result<()> {
    for diagnostic in diagnostics {
        write_header(out, file, diagnostic, color)?;
    }
    Ok(())
}

/// Writes `text`, wrapped in `style` and a reset when color is on.
fn paint<W: Write + ?Sized>(
    out: &mut W,
    color: Color,
    style: &str,
    text: impl Display,
) -> io::Result<()> {
    match color {
        Color::On => write!(out, "{style}{text}{RESET}"),
        Color::Off => write!(out, "{text}"),
    }
}

/// Writes `path:line:col: message [rule-id]` and a newline.
fn write_header<W: Write + ?Sized>(
    out: &mut W,
    file: &SourceFile,
    diagnostic: &Diagnostic,
    color: Color,
) -> io::Result<()> {
    let pos = file.line_col(diagnostic.span.start);
    paint(out, color, YELLOW, file.path())?;
    paint(out, color, GRAY, ":")?;
    write!(out, "{}", pos.line)?;
    paint(out, color, GRAY, ":")?;
    write!(out, "{}", pos.column)?;
    paint(out, color, GRAY, ": ")?;
    paint(out, color, BOLD, &diagnostic.message)?;
    // actionlint writes the newline inside the gray segment, so the reset follows it.
    paint(
        out,
        color,
        GRAY,
        format_args!(" [{}]\n", diagnostic.rule.id),
    )
}

/// Writes the gray `|` line, the numbered source line, and the indicator line, when the
/// diagnostic has a [`snippet`].
fn write_snippet<W: Write + ?Sized>(
    out: &mut W,
    file: &SourceFile,
    diagnostic: &Diagnostic,
    color: Color,
) -> io::Result<()> {
    let Some(snippet) = snippet(file, diagnostic) else {
        return Ok(());
    };
    let gutter = format!("{} | ", file.line_col(diagnostic.span.start).line);
    let indent = " ".repeat(gutter.len().saturating_sub(2));
    paint(out, color, GRAY, format_args!("{indent}|\n"))?;
    paint(out, color, GRAY, &gutter)?;
    writeln!(out, "{}", snippet.line)?;
    paint(out, color, GRAY, format_args!("{indent}| "))?;
    paint(out, color, GREEN, &snippet.indicator)?;
    writeln!(out)
}

/// The source line of a diagnostic and the indicator under its span, as the default format
/// prints them. The `Snippet` and `EndColumn` fields of `-format` templates come from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snippet<'a> {
    /// The line that the span starts on, without its line break.
    pub line: &'a str,
    /// `^` under the first character of the span and `~` under the rest of it, up to the end
    /// of the line.
    pub indicator: String,
    /// The 1-based column, in Unicode code points, of the last character that the indicator
    /// covers. Tabs at the end of the span do not count. It is the diagnostic's column when the
    /// indicator is a lone `^` over nothing visible.
    pub end_column: usize,
}

/// Returns the [`Snippet`] of `diagnostic`, or `None` when its line does not exist. As with
/// actionlint's line reader, a file has no lines when it is empty, and the empty line after a
/// final line break does not exist.
///
/// The span must be in `file`.
#[must_use]
pub fn snippet<'a>(file: &'a SourceFile, diagnostic: &Diagnostic) -> Option<Snippet<'a>> {
    let text = file.text();
    let pos = file.line_col(diagnostic.span.start);
    let range = file.line_range(pos.line)?;
    if range.start >= text.len() {
        return None;
    }
    let source = text.get(range.clone())?;
    // Offsets into `source`, clamped to the line and moved to character boundaries.
    let to_line = |offset: u32| {
        let offset = usize::try_from(offset).unwrap_or(usize::MAX);
        offset.clamp(range.start, range.end) - range.start
    };
    let start = source.floor_char_boundary(to_line(diagnostic.span.start));
    let end = source.ceil_char_boundary(to_line(diagnostic.span.end));
    let (indicator, covered) = indicator(source, start, end);
    Some(Snippet {
        line: source,
        indicator,
        end_column: pos.column + covered.chars().count().saturating_sub(1),
    })
}

/// Builds the indicator for the byte range `start..end` of `line`: `^` under the first
/// character, then `~` under the rest of the range. Also returns the text that the indicator
/// covers, which is empty when it is a lone `^` over nothing visible.
///
/// The text before `start` becomes spaces, as many as its display width, and every tab in it
/// stays a tab, so the `^` lines up with the source line in any terminal. An empty range, or one
/// whose `end` is before its `start`, is widened to the run of characters up to the next space
/// or tab, as actionlint does for every error.
///
/// A tab at the start of the range is copied before the `^`, so the `^` sits under the first
/// character after it. Tabs at the end of the range are dropped. `docs/differences.md#column-units`
/// describes both.
fn indicator(line: &str, start: usize, end: usize) -> (String, &str) {
    let (before, rest) = line.split_at_checked(start).unwrap_or((line, ""));
    let underlined = if end > start {
        rest.get(..end - start).unwrap_or(rest)
    } else {
        rest.split([' ', '\t']).next().unwrap_or("")
    };

    let mut indicator = String::with_capacity(before.len() + underlined.len() + 1);
    for (run, tab) in runs(before) {
        indicator.extend(std::iter::repeat_n(' ', run.width()));
        if tab {
            indicator.push('\t');
        }
    }
    let prefix_len = indicator.len();
    let mut caret_written = false;
    for (run, tab) in runs(underlined) {
        for _ in 0..run.width() {
            indicator.push(if caret_written { '~' } else { '^' });
            caret_written = true;
        }
        if tab {
            indicator.push('\t');
        }
    }
    if caret_written {
        indicator.truncate(indicator.trim_end_matches('\t').len());
        (indicator, underlined.trim_end_matches('\t'))
    } else {
        // The range is empty or has no visible character. Point at its start.
        indicator.truncate(prefix_len);
        indicator.push('^');
        (indicator, "")
    }
}

/// Splits `text` at control characters. Each item is the run of text before a control character
/// and whether that character is a tab. The last run ends at the end of `text` and reports no
/// tab.
///
/// Measuring a whole run with [`UnicodeWidthStr::width`] gives an emoji sequence the two cells a
/// terminal shows. The sum of its characters' widths is 1 for `❤` followed by U+FE0F, and 6 for
/// three emoji joined with U+200D. East Asian ambiguous characters take one cell whatever the
/// locale. Control characters take no cells, because they end a run and are not measured.
fn runs(text: &str) -> impl Iterator<Item = (&str, bool)> {
    text.split_inclusive(char::is_control)
        .map(|piece| match piece.strip_suffix(char::is_control) {
            Some(run) => (run, piece.ends_with('\t')),
            None => (piece, false),
        })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::{Category, FileId, RuleMeta, Severity, Span};

    static RULE: RuleMeta = RuleMeta {
        id: "a-rule",
        description: "",
        kind: "syntax-check",
        category: Category::Correctness,
        default_severity: Severity::Error,
        docs_url: "https://example.com",
    };

    /// The snippet of a diagnostic on the first `token` in `text`.
    fn snippet_of(text: &str, token: &str) -> Option<(String, String, usize)> {
        let file = SourceFile::new(FileId::new(0), "test.yaml", text).unwrap();
        let start = u32::try_from(text.find(token).unwrap()).unwrap();
        let end = start + u32::try_from(token.len()).unwrap();
        let diagnostic = Diagnostic::new(&RULE, Span::new(FileId::new(0), start, end), "m");
        snippet(&file, &diagnostic).map(|snippet| {
            (
                snippet.line.to_owned(),
                snippet.indicator,
                snippet.end_column,
            )
        })
    }

    #[test]
    fn snippet_end_column_is_the_column_of_the_last_underlined_character() {
        assert_eq!(
            snippet_of("on: push\n    branch: main\n", "branch"),
            Some(("    branch: main".to_owned(), "    ^~~~~~".to_owned(), 10))
        );
    }

    #[test]
    fn snippet_end_column_counts_code_points_not_cells() {
        // Each CJK character takes two cells and is one code point.
        let (_, indicator, end_column) = snippet_of("k: 日本語 x\n", "日本語").unwrap();
        assert_eq!(indicator, "   ^~~~~~");
        assert_eq!(end_column, 6);
    }

    #[test]
    fn snippet_end_column_leaves_out_tabs_at_the_end_of_the_span() {
        assert_eq!(snippet_of("k: ab\t\n", "ab\t").map(|s| s.2), Some(5));
    }

    #[test]
    fn snippet_end_column_of_an_empty_span_ends_the_non_blank_run() {
        let text = "k: foo bar\n";
        let file = SourceFile::new(FileId::new(0), "test.yaml", text).unwrap();
        let diagnostic = Diagnostic::new(&RULE, Span::new(FileId::new(0), 3, 3), "m");
        let snippet = snippet(&file, &diagnostic).unwrap();
        assert_eq!(snippet.indicator, "   ^~~");
        assert_eq!(snippet.end_column, 6);
    }

    #[test]
    fn snippet_end_column_is_the_column_for_a_caret_over_nothing_visible() {
        assert_eq!(snippet_of("k:\t\t\n", "\t\t").map(|s| s.2), Some(3));
    }

    #[test]
    fn a_span_past_the_last_line_has_no_snippet() {
        let text = "on: push\n";
        let file = SourceFile::new(FileId::new(0), "test.yaml", text).unwrap();
        let diagnostic = Diagnostic::new(&RULE, Span::new(FileId::new(0), 9, 9), "m");
        assert_eq!(snippet(&file, &diagnostic), None);
    }

    fn at(line: &str, token: &str) -> String {
        let start = line.find(token).unwrap_or(0);
        indicator(line, start, start + token.len()).0
    }

    #[test]
    fn indicator_underlines_the_whole_range() {
        assert_eq!(
            at("runs-on: ubuntu-latest", "ubuntu-latest"),
            "         ^~~~~~~~~~~~~"
        );
    }

    #[test]
    fn indicator_underlines_a_range_with_spaces() {
        assert_eq!(at("run: echo one two", "one two"), "          ^~~~~~~");
    }

    #[test]
    fn indicator_counts_two_cells_for_a_wide_character() {
        assert_eq!(at("日本 x", "x"), "     ^");
        assert_eq!(at("x 日本", "日本"), "  ^~~~");
    }

    #[test]
    fn indicator_counts_one_cell_for_an_ambiguous_character() {
        // U+00B1 PLUS-MINUS SIGN is East Asian ambiguous.
        assert_eq!(at("± x", "x"), "  ^");
    }

    #[test]
    fn indicator_counts_two_cells_for_an_emoji_presentation_sequence() {
        // U+2764 HEAVY BLACK HEART takes one cell alone and two with U+FE0F.
        assert_eq!(at("❤\u{fe0f} x", "x"), "   ^");
        assert_eq!(at("k: ❤\u{fe0f}", "❤\u{fe0f}"), "   ^~");
    }

    #[test]
    fn indicator_counts_two_cells_for_a_zwj_sequence() {
        let family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
        assert_eq!(at(&format!("{family} x"), "x"), "   ^");
        assert_eq!(at(&format!("k: {family}"), family), "   ^~");
    }

    #[test]
    fn indicator_measures_each_run_between_tabs() {
        assert_eq!(at("❤\u{fe0f}\t❤\u{fe0f}\tx", "x"), "  \t  \t^");
        assert_eq!(
            at("k: ❤\u{fe0f}\t❤\u{fe0f}", "❤\u{fe0f}\t❤\u{fe0f}"),
            "   ^~\t~~"
        );
    }

    #[test]
    fn indicator_counts_no_cells_for_control_characters() {
        assert_eq!(at("\u{1}\u{7f}x", "x"), "^");
        assert_eq!(at("k: a\u{1}b", "a\u{1}b"), "   ^~");
    }

    #[test]
    fn indicator_copies_tabs_inside_the_range() {
        assert_eq!(at("k: a\tb", "a\tb"), "   ^\t~");
    }

    #[test]
    fn indicator_drops_tabs_at_the_end_of_the_range() {
        assert_eq!(at("k: ab\t", "ab\t"), "   ^~");
    }

    #[test]
    fn indicator_places_the_caret_after_a_leading_tab_of_the_range() {
        assert_eq!(at("k:\tab", "\tab"), "  \t^~");
    }

    #[test]
    fn indicator_for_a_range_of_only_tabs_is_a_caret_at_its_start() {
        assert_eq!(at("k:\t\t", "\t\t"), "  ^");
    }

    #[test]
    fn indicator_for_zero_width_characters_is_a_caret() {
        // U+0301 COMBINING ACUTE ACCENT has no width.
        assert_eq!(at("a \u{301}", "\u{301}"), "  ^");
    }

    #[test]
    fn indicator_for_an_empty_range_follows_the_non_blank_run() {
        assert_eq!(indicator("foo bar", 0, 0).0, "^~~");
        assert_eq!(indicator("foo\tbar", 4, 4).0, "   \t^~~");
        assert_eq!(indicator("foo bar", 3, 3).0, "   ^");
        assert_eq!(indicator("foo", 3, 3).0, "   ^");
    }
}
