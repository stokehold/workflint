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

use unicode_width::UnicodeWidthChar;

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

/// Writes the gray `|` line, the numbered source line, and the indicator line.
///
/// Nothing is written when the line does not exist. As with actionlint's line reader, a file
/// has no lines when it is empty, and the empty line after a final line break does not exist.
fn write_snippet<W: Write + ?Sized>(
    out: &mut W,
    file: &SourceFile,
    diagnostic: &Diagnostic,
    color: Color,
) -> io::Result<()> {
    let text = file.text();
    let line = file.line_col(diagnostic.span.start).line;
    let Some(range) = file.line_range(line) else {
        return Ok(());
    };
    if range.start >= text.len() {
        return Ok(());
    }
    let Some(source) = text.get(range.clone()) else {
        return Ok(());
    };
    // Offsets into `source`, clamped to the line and moved to character boundaries.
    let to_line = |offset: u32| {
        let offset = usize::try_from(offset).unwrap_or(usize::MAX);
        offset.clamp(range.start, range.end) - range.start
    };
    let start = source.floor_char_boundary(to_line(diagnostic.span.start));
    let end = source.ceil_char_boundary(to_line(diagnostic.span.end));

    let gutter = format!("{line} | ");
    let indent = " ".repeat(gutter.len().saturating_sub(2));
    paint(out, color, GRAY, format_args!("{indent}|\n"))?;
    paint(out, color, GRAY, &gutter)?;
    writeln!(out, "{source}")?;
    paint(out, color, GRAY, format_args!("{indent}| "))?;
    paint(out, color, GREEN, indicator(source, start, end))?;
    writeln!(out)
}

/// Builds the indicator for the byte range `start..end` of `line`: `^` under the first
/// character, then `~` under the rest of the range.
///
/// Every character before `start` becomes as many spaces as its display width, and every tab
/// stays a tab, so the `^` lines up with the source line in any terminal. An empty range is
/// widened to the run of characters up to the next space or tab, as actionlint does for every
/// error.
fn indicator(line: &str, start: usize, end: usize) -> String {
    let (before, rest) = line.split_at_checked(start).unwrap_or((line, ""));
    let underlined = if end > start {
        rest.get(..end - start).unwrap_or(rest)
    } else {
        rest.split([' ', '\t']).next().unwrap_or("")
    };

    let mut indicator = String::with_capacity(before.len() + underlined.len() + 1);
    for c in before.chars() {
        if c == '\t' {
            indicator.push('\t');
        } else {
            indicator.extend(std::iter::repeat_n(' ', width(c)));
        }
    }
    let prefix_len = indicator.len();
    let mut caret_written = false;
    for c in underlined.chars() {
        if c == '\t' {
            indicator.push('\t');
            continue;
        }
        for _ in 0..width(c) {
            indicator.push(if caret_written { '~' } else { '^' });
            caret_written = true;
        }
    }
    if caret_written {
        indicator.truncate(indicator.trim_end_matches('\t').len());
    } else {
        // The range is empty or has no visible character. Point at its start.
        indicator.truncate(prefix_len);
        indicator.push('^');
    }
    indicator
}

/// Returns the number of terminal cells `c` takes.
///
/// East Asian ambiguous characters count as one cell, whatever the locale. Control characters
/// count as none.
fn width(c: char) -> usize {
    c.width().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(line: &str, token: &str) -> String {
        let start = line.find(token).unwrap_or(0);
        indicator(line, start, start + token.len())
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
        assert_eq!(indicator("foo bar", 0, 0), "^~~");
        assert_eq!(indicator("foo\tbar", 4, 4), "   \t^~~");
        assert_eq!(indicator("foo bar", 3, 3), "   ^");
        assert_eq!(indicator("foo", 3, 3), "   ^");
    }
}
