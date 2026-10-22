// workflint parses a `&str`, so this reader replaces upstream's byte reader,
// which decoded UTF-8 and UTF-16 from a `BufRead` and needed one `unsafe`
// block. It moves characters into the scanner's buffer in the same chunks as
// go-yaml's `updateBuffer` and `updateRawBuffer` in `reader.go`, so that a
// control character is reported at the same point of the scan as in go-yaml.
// The character checks in `is_allowed` are upstream's.

use super::macros::{is_blank, is_blankz, is_break};
use super::scanner::Scanner;
use super::{Error, Result};

const BOM: char = '\u{feff}';

/// The size of go-yaml's raw input buffer, `input_raw_buffer_size`. go-yaml
/// reads the input in chunks of this many bytes and decodes a whole chunk at
/// once.
const RAW_CHUNK: usize = 512;

/// Where the reader is in the input. The byte offsets count from the start of
/// the input, including a leading byte order mark.
#[derive(Default)]
pub(crate) struct ReaderState {
    /// Whether the leading byte order mark was looked for.
    started: bool,
    /// The bytes go-yaml has read into its raw buffer.
    read: usize,
    /// The bytes decoded into the scanner's buffer.
    decoded: usize,
    /// Whether a read found the end of the input.
    eof: bool,
    /// The NUL characters go-yaml appends to its buffer at the end of the
    /// input. They count as unread characters.
    nuls: usize,
}

/// Whether YAML allows `ch` in a stream, as upstream's reader checks.
fn is_allowed(ch: char) -> bool {
    ch == '\x09'
        || ch == '\x0A'
        || ch == '\x0D'
        || ch >= '\x20' && ch <= '\x7E'
        || ch == '\u{0085}'
        || ch >= '\u{00A0}' && ch <= '\u{D7FF}'
        || ch >= '\u{E000}' && ch <= '\u{FFFD}'
        || ch >= '\u{10000}' && ch <= '\u{10FFFF}'
}

/// Whether `byte` is an ASCII character that [`is_allowed`] allows.
fn is_allowed_ascii(byte: u8) -> bool {
    matches!(byte, b'\t' | b'\n' | b'\r' | b' '..=b'~')
}

/// The scanner's buffer of decoded characters, which upstream kept in a
/// `VecDeque<char>`.
///
/// The characters live in a `Vec` with a cursor at the first unread one, so
/// that reading a character by index is one bounds check. The reader drops
/// the read characters before it decodes the next chunk, as go-yaml's
/// `updateBuffer` moves the unread characters to the start of its buffer.
pub(crate) struct CharBuffer {
    chars: Vec<char>,
    /// The index in `chars` of the first unread character.
    start: usize,
}

impl CharBuffer {
    /// An empty buffer with room for what it usually holds: the characters
    /// of one raw chunk and the few that the scanner has not read yet.
    pub(crate) fn new() -> Self {
        Self {
            chars: Vec::with_capacity(2 * RAW_CHUNK),
            start: 0,
        }
    }

    /// The number of unread characters.
    pub(crate) fn len(&self) -> usize {
        self.chars.len() - self.start
    }

    /// The unread character at `index`, counting from the first unread one.
    #[inline]
    pub(crate) fn get(&self, index: usize) -> Option<&char> {
        self.chars.get(self.start.checked_add(index)?)
    }

    #[inline]
    pub(crate) fn front(&self) -> Option<&char> {
        self.chars.get(self.start)
    }

    /// Reads the first unread character.
    #[inline]
    pub(crate) fn pop_front(&mut self) -> Option<char> {
        let ch = self.front().copied()?;
        self.start += 1;
        Some(ch)
    }

    /// Counts the unread characters, from the first, that a plain scalar
    /// continues over in one step each, and returns their number and their
    /// length in bytes.
    ///
    /// Such a character is no blank, no line break, and no `:` that a blank
    /// or a line break follows, and in a flow collection (`flow`) also no
    /// `,`, `[`, `]`, `{`, `}`, or `?` that a blank or a line break follows.
    /// The count stops two characters before the end of the buffer, so that
    /// each character after it is known and the scanner's `cache(2)` after
    /// each step would not have read on.
    pub(crate) fn plain_run(&self, flow: bool) -> (usize, usize) {
        let unread = self.chars.get(self.start..).unwrap_or_default();
        let mut count = 0;
        let mut bytes = 0;
        for pair in unread.windows(3) {
            let [ch, next, _] = *pair else {
                break;
            };
            let ends = is_blank(ch)
                || is_break(ch)
                || ch == ':' && is_blankz(next)
                || flow
                    && (matches!(ch, ',' | '[' | ']' | '{' | '}') || ch == '?' && is_blankz(next));
            if ends {
                break;
            }
            count += 1;
            bytes += ch.len_utf8();
        }
        (count, bytes)
    }

    /// Reads the first `count` unread characters, or every one if there are
    /// fewer.
    pub(crate) fn skip(&mut self, count: usize) {
        self.start = self.chars.len().min(self.start + count);
    }

    /// Drops the characters that were read.
    fn compact(&mut self) {
        self.chars.drain(..self.start);
        self.start = 0;
    }
}

impl Scanner<'_> {
    /// The characters in the buffer that the scanner has not consumed,
    /// counted as go-yaml's `unread`, which includes the NULs at the end.
    #[inline]
    pub(crate) fn unread(&self) -> usize {
        self.buffer.len() + self.reader.nuls
    }

    /// Reads the next chunk into go-yaml's raw buffer, as `updateRawBuffer`
    /// does with a string input.
    fn read_raw_chunk(&mut self) {
        let reader = &mut self.reader;
        if reader.eof {
            return;
        }
        let pending = reader.read - reader.decoded;
        if pending >= RAW_CHUNK {
            return;
        }
        let len = self.source.len();
        if reader.read == len {
            reader.eof = true;
            return;
        }
        reader.read += (RAW_CHUNK - pending).min(len - reader.read);
    }

    /// Decodes every complete character of the raw buffer into the scanner's
    /// buffer.
    fn decode_raw_chunk(&mut self) -> Result<()> {
        let chunk = self
            .source
            .get(self.reader.decoded..)
            .ok_or_else(|| Error::internal(super::ErrorKind::Reader, self.mark))?;
        // The raw buffer ends at `read`, which can split a character. That
        // character waits for the next chunk.
        let mut end = self.reader.read - self.reader.decoded;
        while !chunk.is_char_boundary(end) {
            end -= 1;
        }
        let chunk = chunk.get(..end).unwrap_or_default();
        // Most chunks are ASCII without control characters. Checking the
        // bytes first and widening them in one pass is several times faster
        // than the loop below.
        if chunk.bytes().all(is_allowed_ascii) {
            self.buffer.chars.extend(chunk.bytes().map(char::from));
            self.reader.decoded += end;
            return Ok(());
        }
        self.buffer.chars.reserve(chunk.len());
        for (offset, ch) in chunk.char_indices() {
            if !is_allowed(ch) {
                let offset = self.reader.decoded + offset;
                self.reader.decoded = offset;
                return Err(Error::reader(
                    "control characters are not allowed",
                    offset,
                    ch as _,
                ));
            }
            self.buffer.chars.push(ch);
        }
        self.reader.decoded += end;
        Ok(())
    }
}

/// Moves characters from the input into the scanner's buffer until the
/// buffer holds `length` characters or the input ends, as go-yaml's
/// `updateBuffer` does.
///
/// The first call skips a leading byte order mark. The scanner's mark still
/// counts its bytes, so `Mark::index` stays a byte offset into the source,
/// while the column of the first character stays 0, as in libyaml.
pub(crate) fn yaml_parser_update_buffer(parser: &mut Scanner<'_>, length: usize) -> Result<()> {
    if parser.unread() >= length {
        return Ok(());
    }
    if !parser.reader.started {
        parser.reader.started = true;
        while !parser.reader.eof && parser.reader.read - parser.reader.decoded < 3 {
            parser.read_raw_chunk();
        }
        if parser.source.starts_with(BOM) {
            let width = BOM.len_utf8();
            parser.reader.decoded += width;
            parser.mark.index += width as u32;
        }
    }
    parser.buffer.compact();
    let mut first = true;
    while parser.unread() < length {
        if !first || parser.reader.decoded == parser.reader.read {
            parser.read_raw_chunk();
        }
        first = false;
        parser.decode_raw_chunk()?;
        if parser.reader.eof {
            parser.reader.nuls += 1;
            break;
        }
    }
    // go-yaml moves the unread characters to the start of its buffer here, and
    // its `isBOM` looks only at the start of the buffer.
    parser.buffer_starts_with_bom = parser.buffer.front() == Some(&BOM);
    Ok(())
}
