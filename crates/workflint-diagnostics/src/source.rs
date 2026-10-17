use std::fmt;
use std::ops::Range;

/// Identifies one [`SourceFile`] within a lint run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(u32);

impl FileId {
    /// Creates a file ID from its index. The caller that owns the files assigns the indexes.
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// Returns the index this ID was created from.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// A 1-based line and a 1-based column counted in Unicode code points.
///
/// The derived order compares the line first, then the column. `Display` prints the
/// `line:N,col:M` form that actionlint's messages use when they mention another position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LineCol {
    /// The 1-based line number.
    pub line: usize,
    /// The 1-based column, counted in Unicode code points.
    pub column: usize,
}

impl fmt::Display for LineCol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line:{},col:{}", self.line, self.column)
    }
}

/// The text of one linted file, with its display path and a line index.
#[derive(Debug, Clone)]
pub struct SourceFile {
    id: FileId,
    path: String,
    text: String,
    /// The byte offset where each line starts. The first entry is always 0.
    line_starts: Vec<usize>,
    /// Character counts at fixed points in the text, so that a column takes
    /// time independent of the line length. Empty if the text is ASCII.
    char_counts: Vec<CharCount>,
}

/// The number of characters before `offset` in the text.
#[derive(Debug, Clone, Copy)]
struct CharCount {
    offset: usize,
    chars: usize,
}

/// The distance in bytes between two [`CharCount`]s. Entry `k` holds the
/// character boundary at or before byte `k * CHAR_COUNT_STEP`.
const CHAR_COUNT_STEP: usize = 256;

const BOM: char = '\u{feff}';

impl SourceFile {
    /// Creates a source file and builds its line index.
    ///
    /// # Errors
    ///
    /// Returns [`SourceTooLarge`] if `text` is longer than `u32::MAX` bytes, because a
    /// [`Span`](crate::Span) stores byte offsets as `u32`.
    pub fn new(
        id: FileId,
        path: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<Self, SourceTooLarge> {
        let text = text.into();
        if u32::try_from(text.len()).is_err() {
            return Err(SourceTooLarge { len: text.len() });
        }
        let line_starts = line_starts(&text);
        let char_counts = char_counts(&text);
        Ok(Self {
            id,
            path: path.into(),
            text,
            line_starts,
            char_counts,
        })
    }

    /// Returns the ID of this file.
    #[must_use]
    pub fn id(&self) -> FileId {
        self.id
    }

    /// Returns the path to show in output.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the full source text, including a leading byte order mark if the file has one.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Converts a byte offset into a line and column.
    ///
    /// The line breaks are the ones the YAML parser counts: `\n`, `\r\n`, a lone `\r`, U+0085,
    /// U+2028, and U+2029. A byte order mark at the start of the file takes up no column. An
    /// offset past the end of the text is treated as the end of the text. An offset inside a
    /// multi-byte character is treated as the start of that character, and an offset on the `\n`
    /// of a `\r\n` as the `\r`, because both bytes are one line break.
    #[must_use]
    pub fn line_col(&self, offset: u32) -> LineCol {
        let offset = usize::try_from(offset).unwrap_or(usize::MAX);
        let mut offset = self.text.floor_char_boundary(offset.min(self.text.len()));
        if offset > 0 && self.text.get(offset - 1..=offset) == Some("\r\n") {
            offset -= 1;
        }
        // `line_starts[0]` is 0, so at least one start is at or before `offset`.
        let line = self.line_starts.partition_point(|&start| start <= offset);
        let mut start = self
            .line_starts
            .get(line.saturating_sub(1))
            .copied()
            .unwrap_or(0);
        if start == 0 && self.text.starts_with(BOM) && offset >= BOM.len_utf8() {
            start = BOM.len_utf8();
        }
        let column = self.chars_before(offset) - self.chars_before(start) + 1;
        LineCol { line, column }
    }

    /// The number of characters before `offset`, a character boundary.
    fn chars_before(&self, offset: usize) -> usize {
        if self.char_counts.is_empty() {
            return offset;
        }
        let count = self
            .char_counts
            .get(offset / CHAR_COUNT_STEP)
            .copied()
            .unwrap_or(CharCount {
                offset: 0,
                chars: 0,
            });
        count.chars
            + self
                .text
                .get(count.offset..offset)
                .map_or(0, |s| s.chars().count())
    }

    /// Returns the byte range of a 1-based line without its line break, or `None` if the file
    /// has no such line.
    pub(crate) fn line_range(&self, line: usize) -> Option<Range<usize>> {
        let start = *self.line_starts.get(line.checked_sub(1)?)?;
        let Some(&next) = self.line_starts.get(line) else {
            return Some(start..self.text.len());
        };
        let with_break = self.text.get(start..next)?;
        let break_len = LINE_BREAKS
            .iter()
            .find(|brk| with_break.ends_with(**brk))
            .map_or(0, |brk| brk.len());
        Some(start..next - break_len)
    }
}

/// The line breaks [`line_starts`] recognises. `\r\n` comes before `\n` and `\r` so that it is
/// removed as one break.
const LINE_BREAKS: [&str; 6] = ["\r\n", "\n", "\r", "\u{85}", "\u{2028}", "\u{2029}"];

/// Returns the byte offset where each line of `text` starts.
fn line_starts(text: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    let mut starts = vec![0];
    let mut i = 0;
    while let Some(&byte) = bytes.get(i) {
        let break_len = match byte {
            b'\r' if bytes.get(i + 1) == Some(&b'\n') => 2,
            b'\n' | b'\r' => 1,
            // U+0085 is C2 85 in UTF-8.
            0xc2 if bytes.get(i + 1) == Some(&0x85) => 2,
            // U+2028 is E2 80 A8 and U+2029 is E2 80 A9.
            0xe2 if bytes.get(i + 1) == Some(&0x80)
                && matches!(bytes.get(i + 2), Some(0xa8 | 0xa9)) =>
            {
                3
            }
            _ => {
                i += 1;
                continue;
            }
        };
        i += break_len;
        starts.push(i);
    }
    starts
}

/// Returns the [`CharCount`] at every [`CHAR_COUNT_STEP`] bytes of `text`, or
/// nothing if `text` is ASCII, where offsets count characters.
fn char_counts(text: &str) -> Vec<CharCount> {
    if text.is_ascii() {
        return Vec::new();
    }
    let mut counts = Vec::with_capacity(text.len() / CHAR_COUNT_STEP + 1);
    let mut previous = CharCount {
        offset: 0,
        chars: 0,
    };
    for step in (0..=text.len()).step_by(CHAR_COUNT_STEP) {
        let offset = text.floor_char_boundary(step);
        let chars = previous.chars
            + text
                .get(previous.offset..offset)
                .map_or(0, |s| s.chars().count());
        previous = CharCount { offset, chars };
        counts.push(previous);
    }
    counts
}

/// The error [`SourceFile::new`] returns for text longer than `u32::MAX` bytes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "source text is {len} bytes, more than the limit of {} bytes",
    u32::MAX
)]
pub struct SourceTooLarge {
    /// The length of the rejected text in bytes.
    pub len: usize,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn file(text: &str) -> SourceFile {
        SourceFile::new(FileId::new(0), "test.yaml", text).unwrap()
    }

    fn line_col(text: &str, offset: usize) -> (usize, usize) {
        let pos = file(text).line_col(u32::try_from(offset).unwrap());
        (pos.line, pos.column)
    }

    #[test]
    fn ascii_offsets_map_to_lines_and_columns() {
        let text = "on: push\njobs:\n  test:\n";
        assert_eq!(line_col(text, 0), (1, 1));
        assert_eq!(line_col(text, 4), (1, 5));
        assert_eq!(line_col(text, 9), (2, 1));
        assert_eq!(line_col(text, 17), (3, 3));
    }

    #[test]
    fn columns_count_code_points_not_bytes() {
        let text = "\"日本\": v";
        assert_eq!(line_col(text, text.find('v').unwrap()), (1, 7));
    }

    #[test]
    fn a_tab_is_one_column() {
        let text = "\t\tv";
        assert_eq!(line_col(text, 2), (1, 3));
    }

    #[test]
    fn crlf_is_one_line_break() {
        let text = "a: 1\r\nb: 2\r\n";
        assert_eq!(line_col(text, 6), (2, 1));
        assert_eq!(line_col(text, 9), (2, 4));
        assert_eq!(line_col(text, 12), (3, 1));
    }

    #[test]
    fn an_offset_on_the_line_feed_of_a_crlf_points_at_the_carriage_return() {
        let text = "ab\r\ncd";
        assert_eq!(line_col(text, 2), (1, 3));
        assert_eq!(line_col(text, 3), (1, 3));
        assert_eq!(line_col(text, 4), (2, 1));
    }

    #[test]
    fn a_lone_cr_is_a_line_break() {
        let text = "a: 1\rb: 2";
        assert_eq!(line_col(text, 5), (2, 1));
        assert_eq!(line_col(text, 8), (2, 4));
    }

    #[test]
    fn unicode_line_breaks_match_the_yaml_parser() {
        for brk in ['\u{85}', '\u{2028}', '\u{2029}'] {
            let text = format!("a{brk}b");
            assert_eq!(line_col(&text, text.find('b').unwrap()), (2, 1), "{brk:?}");
        }
    }

    #[test]
    fn a_leading_bom_takes_no_column() {
        let text = "\u{feff}on: push\r\njobs:";
        assert_eq!(line_col(text, 0), (1, 1));
        assert_eq!(line_col(text, 3), (1, 1));
        assert_eq!(line_col(text, 7), (1, 5));
        assert_eq!(line_col(text, text.find('j').unwrap()), (2, 1));
    }

    #[test]
    fn an_offset_at_end_of_file_is_after_the_last_character() {
        assert_eq!(line_col("ab", 2), (1, 3));
        assert_eq!(line_col("ab\n", 3), (2, 1));
    }

    #[test]
    fn an_offset_past_end_of_file_is_clamped_to_the_end() {
        assert_eq!(file("ab").line_col(100), LineCol { line: 1, column: 3 });
    }

    #[test]
    fn an_offset_inside_a_character_points_at_that_character() {
        // `日` occupies bytes 1 to 3.
        assert_eq!(line_col("a日b", 2), (1, 2));
    }

    // Counting the characters from the start of the line for each offset
    // took about 9 seconds here in a debug build, and now takes under one.
    #[test]
    fn offsets_on_a_long_line_convert_in_time_independent_of_the_column() {
        let line = "key: 'é', 日本: [a, b], ".repeat(80_000);
        let text = format!("a: 1\n{line}\nb: 2\n");
        let f = file(&text);
        let started = std::time::Instant::now();
        for (column, (offset, _)) in (1..).zip(line.char_indices()).step_by(10) {
            let pos = f.line_col(u32::try_from(5 + offset).unwrap());
            assert_eq!((pos.line, pos.column), (2, column), "offset {offset}");
        }
        let elapsed = started.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "converting the offsets took {elapsed:?}"
        );
    }

    #[test]
    fn columns_after_non_ascii_text_count_code_points_on_every_line() {
        let text = "日本語\n\u{feff}x\n".repeat(200);
        let f = file(&text);
        let mut line = 1;
        let mut column = 1;
        for (offset, ch) in text.char_indices() {
            let pos = f.line_col(u32::try_from(offset).unwrap());
            assert_eq!((pos.line, pos.column), (line, column), "offset {offset}");
            if ch == '\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
        }
    }

    #[test]
    fn an_empty_file_has_one_line() {
        assert_eq!(line_col("", 0), (1, 1));
    }

    #[test]
    fn line_range_excludes_the_line_break() {
        let f = file("a: 1\r\nb: 2\rc\u{2028}d\n");
        let lines: Vec<_> = (1..=5).map(|n| f.line_range(n)).collect();
        assert_eq!(
            lines,
            [
                Some(0..4),
                Some(6..10),
                Some(11..12),
                Some(15..16),
                Some(17..17)
            ]
        );
    }

    #[test]
    fn line_range_of_a_last_line_without_a_break_ends_at_end_of_file() {
        assert_eq!(file("a\nbc").line_range(2), Some(2..4));
    }

    #[test]
    fn line_range_of_a_missing_line_is_none() {
        let f = file("a\n");
        assert_eq!(f.line_range(0), None);
        assert_eq!(f.line_range(3), None);
    }

    #[test]
    fn line_col_displays_in_the_reference_message_format() {
        let pos = LineCol {
            line: 3,
            column: 14,
        };
        assert_eq!(pos.to_string(), "line:3,col:14");
    }

    #[test]
    fn source_too_large_names_the_length_and_the_limit() {
        let err = SourceTooLarge { len: 5_000_000_000 };
        assert_eq!(
            err.to_string(),
            "source text is 5000000000 bytes, more than the limit of 4294967295 bytes"
        );
    }

    #[test]
    fn source_file_keeps_id_path_and_text() {
        let f = SourceFile::new(FileId::new(7), ".github/workflows/ci.yml", "on: push").unwrap();
        assert_eq!(f.id().index(), 7);
        assert_eq!(f.path(), ".github/workflows/ci.yml");
        assert_eq!(f.text(), "on: push");
    }
}
