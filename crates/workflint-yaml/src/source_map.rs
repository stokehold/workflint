//! Source maps from decoded scalar values back to the source (ADR 0005).
//!
//! A map is built on demand from the scalar's raw source and style. The
//! decoder here repeats what the vendored scanner does to a scalar's
//! characters, `scan_plain_scalar`, `scan_flow_scalar`, and
//! `scan_block_scalar` in `libyaml/scanner.rs`, and records where each
//! decoded character comes from. If its result differs from the scalar's
//! value, the map falls back to the start of the content.

use std::ops::Range;

use workflint_diagnostics::{LineCol, SourceFile, Span};

use crate::document::{Chomping, Node, NodeKind, Scalar, ScalarStyle};

/// Maps byte offsets in a scalar's decoded value to byte offsets in the
/// source file.
///
/// [`Node::source_map`] builds it. A character copied unchanged from the
/// source maps to that character. A character that an escape produces maps
/// to the escape's backslash, and the `'` of a `''` maps to the first quote.
/// A space or line feed that line folding produces maps to the source line
/// break it comes from, and a line feed read from a `\r\n` break maps to the
/// `\r`. Offsets never decrease as the decoded offset grows.
///
/// ```
/// use workflint_diagnostics::{FileId, LineCol, SourceFile};
/// use workflint_yaml::{NodeKind, parse};
///
/// let file = SourceFile::new(FileId::new(0), "ci.yml", "run: |\n  make\n  echo ${{ x }}\n").unwrap();
/// let document = parse(&file).document.unwrap();
/// let (_, node) = document
///     .nodes()
///     .find(|(_, node)| matches!(&node.kind, NodeKind::Scalar(s) if s.value.contains("${{")))
///     .unwrap();
/// let NodeKind::Scalar(scalar) = &node.kind else { unreachable!() };
/// let offset = scalar.value.find("${{").unwrap();
/// let map = node.source_map(&file).unwrap();
/// assert_eq!(map.line_col(offset), LineCol { line: 3, column: 8 });
/// ```
#[derive(Debug, Clone)]
pub struct SourceMap<'a> {
    file: &'a SourceFile,
    /// Sorted by decoded offset. The first run starts at decoded offset 0
    /// unless the value is empty or the map fell back.
    runs: Vec<Run>,
    /// The length of the decoded value.
    len: usize,
    /// Where the content starts, for the fallback.
    start: u32,
    /// The source offset that the end of the value maps to.
    end: u32,
    exact: bool,
}

/// Decoded bytes from `decoded` up to the next run, which come from the
/// source from `source` to `source_end`.
#[derive(Debug, Clone, Copy)]
struct Run {
    decoded: usize,
    source: u32,
    source_end: u32,
    /// Whether the bytes are a copy of the source bytes. Otherwise the run is
    /// one character, and each of its bytes maps to `source`.
    copied: bool,
}

impl SourceMap<'_> {
    /// Returns the source byte offset of the decoded byte offset `decoded`.
    ///
    /// An offset inside a decoded character maps like the start of that
    /// character. The end of the value, and any offset past it, maps to the
    /// end of the source of the last decoded character, or to the start of
    /// the content if the value is empty.
    #[must_use]
    pub fn source_offset(&self, decoded: usize) -> u32 {
        if decoded >= self.len {
            return self.end;
        }
        let index = self.runs.partition_point(|run| run.decoded <= decoded);
        let Some(run) = index.checked_sub(1).and_then(|i| self.runs.get(i)) else {
            return self.start;
        };
        if !run.copied {
            return run.source;
        }
        let delta = decoded - run.decoded;
        let offset = usize::try_from(run.source).map_or(usize::MAX, |s| s.saturating_add(delta));
        let offset = self.file.text().floor_char_boundary(offset);
        u32::try_from(offset).unwrap_or(run.source)
    }

    /// Returns the source span of the decoded byte range `decoded`.
    ///
    /// The span starts where the start of the range maps to. A non-empty
    /// range ends after the source of its last character, so a range that
    /// covers an escape covers the whole escape sequence.
    #[must_use]
    pub fn source_span(&self, decoded: Range<usize>) -> Span {
        let start = self.source_offset(decoded.start);
        let end = if decoded.end > decoded.start {
            self.source_end(decoded.end)
        } else {
            start
        };
        Span::new(self.file.id(), start, end.max(start))
    }

    /// Returns the line and column of the decoded byte offset `decoded`,
    /// with the column counted in code points.
    #[must_use]
    pub fn line_col(&self, decoded: usize) -> LineCol {
        self.file.line_col(self.source_offset(decoded))
    }

    /// Whether the map is exact. A map that is not exact maps every offset to
    /// the start of the scalar's content.
    ///
    /// The map falls back when its decoder does not reproduce the scalar's
    /// value. No input is known to cause that.
    #[must_use]
    pub fn is_exact(&self) -> bool {
        self.exact
    }

    /// Returns the end of the source of the decoded character that holds
    /// the byte before `decoded`.
    fn source_end(&self, decoded: usize) -> u32 {
        if decoded >= self.len {
            return self.end;
        }
        let index = self.runs.partition_point(|run| run.decoded < decoded);
        let Some(run) = index.checked_sub(1).and_then(|i| self.runs.get(i)) else {
            return self.start;
        };
        if !run.copied {
            return run.source_end;
        }
        let delta = decoded - run.decoded;
        let offset = usize::try_from(run.source).map_or(usize::MAX, |s| s.saturating_add(delta));
        let offset = self.file.text().ceil_char_boundary(offset);
        u32::try_from(offset).map_or(run.source_end, |offset| offset.min(run.source_end))
    }
}

impl Node {
    /// Builds the source map of a scalar node.
    ///
    /// Returns `None` if the node is not a scalar or if `file` is not the
    /// file the node was parsed from.
    #[must_use]
    pub fn source_map<'a>(&self, file: &'a SourceFile) -> Option<SourceMap<'a>> {
        let NodeKind::Scalar(scalar) = &self.kind else {
            return None;
        };
        if file.id() != self.span.file {
            return None;
        }
        let text = file.text();
        let (start, end) = content(self, scalar);
        let decoded = (start <= end)
            .then(|| text.get(start..end))
            .flatten()
            .and_then(|_| decode(text, start, end, scalar))
            .filter(|out| out.value == scalar.value);
        let to_u32 = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        Some(match decoded {
            Some(out) => SourceMap {
                file,
                runs: out.runs,
                len: scalar.value.len(),
                start: to_u32(start),
                end: to_u32(out.end.unwrap_or(start)),
                exact: true,
            },
            None => SourceMap {
                file,
                runs: Vec::new(),
                len: scalar.value.len(),
                start: self.content_start,
                end: self.content_start,
                exact: false,
            },
        })
    }
}

/// Returns the byte range of the scalar's raw content: inside the quotes of
/// a quoted scalar, and from the line after the header of a block scalar.
fn content(node: &Node, scalar: &Scalar) -> (usize, usize) {
    let start = usize::try_from(node.content_start).unwrap_or(usize::MAX);
    let end = usize::try_from(node.span.end).unwrap_or(usize::MAX);
    match scalar.style {
        ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted => {
            (start.saturating_add(1), end.saturating_sub(1))
        }
        ScalarStyle::Plain | ScalarStyle::Literal(_) | ScalarStyle::Folded(_) => (start, end),
    }
}

fn decode(text: &str, start: usize, end: usize, scalar: &Scalar) -> Option<Out> {
    let mut decoder = Decoder {
        text,
        pos: start,
        end,
        column: 0,
        out: Out::default(),
        leading_break: None,
        trailing_breaks: Vec::new(),
        whitespaces: Vec::new(),
    };
    match scalar.style {
        ScalarStyle::Plain => decoder.plain(),
        ScalarStyle::SingleQuoted => decoder.flow(true)?,
        ScalarStyle::DoubleQuoted => decoder.flow(false)?,
        ScalarStyle::Literal(header) => {
            decoder.block(true, header.chomping, header.indent, scalar.parent_indent)?;
        }
        ScalarStyle::Folded(header) => {
            decoder.block(false, header.chomping, header.indent, scalar.parent_indent)?;
        }
    }
    Some(decoder.out)
}

/// One decoded character and the source it comes from.
#[derive(Debug, Clone, Copy)]
struct Piece {
    ch: char,
    source: Range32,
    copied: bool,
}

/// A source byte range. `Range` is not `Copy`.
#[derive(Debug, Clone, Copy)]
struct Range32 {
    start: usize,
    end: usize,
}

/// The decoded value and its runs.
#[derive(Debug, Default)]
struct Out {
    value: String,
    runs: Vec<Run>,
    /// The end of the source of the last decoded character.
    end: Option<usize>,
}

impl Out {
    fn push(&mut self, piece: Piece) {
        let to_u32 = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        let (source, source_end) = (to_u32(piece.source.start), to_u32(piece.source.end));
        match self.runs.last_mut() {
            Some(run) if piece.copied && run.copied && run.source_end == source => {
                run.source_end = source_end;
            }
            _ => self.runs.push(Run {
                decoded: self.value.len(),
                source,
                source_end,
                copied: piece.copied,
            }),
        }
        self.value.push(piece.ch);
        self.end = Some(piece.source.end);
    }
}

/// Walks a scalar's raw content the way the scanner does.
struct Decoder<'t> {
    text: &'t str,
    pos: usize,
    end: usize,
    /// The 0-based column in code points, for block scalars, whose content
    /// starts at the start of a line.
    column: i64,
    out: Out,
    leading_break: Option<Piece>,
    trailing_breaks: Vec<Piece>,
    whitespaces: Vec<Piece>,
}

fn is_blank(ch: Option<char>) -> bool {
    matches!(ch, Some(' ' | '\t'))
}

fn is_break(ch: Option<char>) -> bool {
    matches!(ch, Some('\r' | '\n' | '\u{85}' | '\u{2028}' | '\u{2029}'))
}

impl Decoder<'_> {
    fn rest(&self) -> &str {
        self.text.get(self.pos..self.end).unwrap_or_default()
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn peek_second(&self) -> Option<char> {
        self.rest().chars().nth(1)
    }

    fn at_end(&self) -> bool {
        self.pos >= self.end
    }

    /// Moves past the next character and returns it with its source.
    fn take(&mut self) -> Option<Piece> {
        let ch = self.peek()?;
        let start = self.pos;
        self.pos += ch.len_utf8();
        self.column += 1;
        Some(Piece {
            ch,
            source: Range32 {
                start,
                end: self.pos,
            },
            copied: true,
        })
    }

    /// Moves past `len` bytes of an escape or a `''` and returns `ch` with
    /// their source.
    fn take_as(&mut self, ch: char, len: usize) -> Piece {
        let start = self.pos;
        self.pos = (self.pos + len).min(self.end);
        self.column += 1;
        Piece {
            ch,
            source: Range32 {
                start,
                end: self.pos,
            },
            copied: false,
        }
    }

    /// Reads a line break the way the scanner's `read_line_break` does: a
    /// `\r\n`, `\r`, `\n`, or U+0085 becomes a line feed, and U+2028 and
    /// U+2029 stay as they are. Returns `None` if no line break follows.
    fn take_break(&mut self) -> Option<Piece> {
        let ch = self.peek().filter(|&ch| is_break(Some(ch)))?;
        let start = self.pos;
        let len = if ch == '\r' && self.peek_second() == Some('\n') {
            2
        } else {
            ch.len_utf8()
        };
        self.pos += len;
        self.column = 0;
        let decoded = if ch.len_utf8() == 3 { ch } else { '\n' };
        Some(Piece {
            ch: decoded,
            source: Range32 {
                start,
                end: self.pos,
            },
            copied: decoded == ch,
        })
    }

    /// Joins the line breaks or blanks before the next characters, as the
    /// plain and flow scanners do.
    fn join(&mut self, leading_blanks: bool) {
        if leading_blanks {
            match self.leading_break.take() {
                Some(piece) if piece.ch == '\n' => {
                    if self.trailing_breaks.is_empty() {
                        self.out.push(Piece {
                            ch: ' ',
                            copied: false,
                            ..piece
                        });
                    }
                }
                other => {
                    if let Some(piece) = other {
                        self.out.push(piece);
                    }
                }
            }
            for piece in self.trailing_breaks.drain(..) {
                self.out.push(piece);
            }
        } else {
            for piece in self.whitespaces.drain(..) {
                self.out.push(piece);
            }
        }
    }

    /// Reads the blanks and line breaks after a run of other characters.
    /// Returns whether a line break was read.
    fn blanks(&mut self, mut leading_blanks: bool) -> bool {
        loop {
            let ch = self.peek();
            if is_blank(ch) {
                if let Some(piece) = self.take()
                    && !leading_blanks
                {
                    self.whitespaces.push(piece);
                }
            } else if is_break(ch) {
                let piece = self.take_break();
                if leading_blanks {
                    self.trailing_breaks.extend(piece);
                } else {
                    self.whitespaces.clear();
                    self.leading_break = piece;
                    leading_blanks = true;
                }
            } else {
                return leading_blanks;
            }
        }
    }

    /// Decodes a plain scalar, as `scan_plain_scalar` does. The content ends
    /// at its last non-blank character, so every word in it belongs to the
    /// scalar.
    fn plain(&mut self) {
        let mut leading_blanks = false;
        loop {
            while !self.at_end() && !is_blank(self.peek()) && !is_break(self.peek()) {
                if leading_blanks || !self.whitespaces.is_empty() {
                    self.join(leading_blanks);
                    leading_blanks = false;
                }
                if let Some(piece) = self.take() {
                    self.out.push(piece);
                }
            }
            if self.at_end() {
                return;
            }
            leading_blanks = self.blanks(leading_blanks);
        }
    }

    /// Decodes a quoted scalar, as `scan_flow_scalar` does, from after the
    /// opening quote to the closing quote.
    fn flow(&mut self, single: bool) -> Option<()> {
        loop {
            let mut leading_blanks = false;
            while !self.at_end() && !is_blank(self.peek()) && !is_break(self.peek()) {
                let ch = self.peek();
                if single && ch == Some('\'') && self.peek_second() == Some('\'') {
                    let piece = self.take_as('\'', 2);
                    self.out.push(piece);
                } else if !single && ch == Some('\\') && is_break(self.peek_second()) {
                    self.take();
                    self.take_break();
                    leading_blanks = true;
                    break;
                } else if !single && ch == Some('\\') {
                    let piece = self.escape()?;
                    self.out.push(piece);
                } else if let Some(piece) = self.take() {
                    self.out.push(piece);
                }
            }
            if self.at_end() {
                return Some(());
            }
            leading_blanks = self.blanks(leading_blanks);
            self.join(leading_blanks);
        }
    }

    /// Decodes the escape sequence at the current `\`.
    fn escape(&mut self) -> Option<Piece> {
        let (ch, digits) = match self.peek_second()? {
            '0' => ('\0', 0),
            'a' => ('\x07', 0),
            'b' => ('\x08', 0),
            't' | '\t' => ('\t', 0),
            'n' => ('\n', 0),
            'v' => ('\x0b', 0),
            'f' => ('\x0c', 0),
            'r' => ('\r', 0),
            'e' => ('\x1b', 0),
            ' ' => (' ', 0),
            '"' => ('"', 0),
            '\'' => ('\'', 0),
            '/' => ('/', 0),
            '\\' => ('\\', 0),
            'N' => ('\u{85}', 0),
            '_' => ('\u{a0}', 0),
            'L' => ('\u{2028}', 0),
            'P' => ('\u{2029}', 0),
            'x' => ('\0', 2),
            'u' => ('\0', 4),
            'U' => ('\0', 8),
            _ => return None,
        };
        let ch = if digits == 0 {
            ch
        } else {
            let hex = self.rest().get(2..2 + digits)?;
            char::from_u32(u32::from_str_radix(hex, 16).ok()?)?
        };
        Some(self.take_as(ch, 2 + digits))
    }

    /// Decodes a block scalar, as `scan_block_scalar` does, from the start of
    /// the line after its header. `parent_indent` is the scanner's
    /// indentation when it read the header.
    fn block(
        &mut self,
        literal: bool,
        chomping: Chomping,
        increment: Option<u8>,
        parent_indent: i64,
    ) -> Option<()> {
        let mut indent = match increment {
            Some(increment) if parent_indent >= 0 => parent_indent + i64::from(increment),
            Some(increment) => i64::from(increment),
            None => 0,
        };
        self.block_breaks(&mut indent, parent_indent)?;
        let mut leading_blank = false;
        while self.column == indent && !self.at_end() {
            let trailing_blank = is_blank(self.peek());
            let folds = self.leading_break.is_some_and(|piece| piece.ch == '\n');
            if !literal && folds && !leading_blank && !trailing_blank {
                if let Some(piece) = self.leading_break.take()
                    && self.trailing_breaks.is_empty()
                {
                    self.out.push(Piece {
                        ch: ' ',
                        copied: false,
                        ..piece
                    });
                }
            } else if let Some(piece) = self.leading_break.take() {
                self.out.push(piece);
            }
            for piece in self.trailing_breaks.drain(..) {
                self.out.push(piece);
            }
            leading_blank = is_blank(self.peek());
            while !self.at_end() && !is_break(self.peek()) {
                if let Some(piece) = self.take() {
                    self.out.push(piece);
                }
            }
            self.leading_break = self.take_break();
            self.block_breaks(&mut indent, parent_indent)?;
        }
        if chomping != Chomping::Strip
            && let Some(piece) = self.leading_break.take()
        {
            self.out.push(piece);
        }
        if chomping == Chomping::Keep {
            for piece in self.trailing_breaks.drain(..) {
                self.out.push(piece);
            }
        }
        Some(())
    }

    /// Reads the indentation and empty lines before a block scalar line, as
    /// `scan_block_scalar_breaks` does, and sets the indentation if it is
    /// not known yet.
    fn block_breaks(&mut self, indent: &mut i64, parent_indent: i64) -> Option<()> {
        let mut max_indent = 0;
        loop {
            while (*indent == 0 || self.column < *indent) && self.peek() == Some(' ') {
                self.take();
            }
            max_indent = max_indent.max(self.column);
            if (*indent == 0 || self.column < *indent) && self.peek() == Some('\t') {
                return None;
            }
            let Some(piece) = self.take_break() else {
                break;
            };
            self.trailing_breaks.push(piece);
        }
        if *indent == 0 {
            *indent = max_indent.max(parent_indent + 1).max(1);
        }
        Some(())
    }
}
