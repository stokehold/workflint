// workflint parses a `&str`, so this reader replaces upstream's byte reader,
// which decoded UTF-8 and UTF-16 from a `BufRead` and needed one `unsafe`
// block. It moves characters into the scanner's buffer in the same chunks as
// go-yaml's `updateBuffer` and `updateRawBuffer` in `reader.go`, so that a
// control character is reported at the same point of the scan as in go-yaml.
// The character checks in `push_char` are upstream's.

use std::collections::VecDeque;

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

fn push_char(out: &mut VecDeque<char>, ch: char, offset: usize) -> Result<()> {
    if !(ch == '\x09'
        || ch == '\x0A'
        || ch == '\x0D'
        || ch >= '\x20' && ch <= '\x7E'
        || ch == '\u{0085}'
        || ch >= '\u{00A0}' && ch <= '\u{D7FF}'
        || ch >= '\u{E000}' && ch <= '\u{FFFD}'
        || ch >= '\u{10000}' && ch <= '\u{10FFFF}')
    {
        return Err(Error::reader(
            "control characters are not allowed",
            offset,
            ch as _,
        ));
    }
    out.push_back(ch);
    Ok(())
}

impl Scanner<'_> {
    /// The characters in the buffer that the scanner has not consumed,
    /// counted as go-yaml's `unread`, which includes the NULs at the end.
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
        for ch in chunk.chars() {
            let offset = self.reader.decoded;
            if offset + ch.len_utf8() > self.reader.read {
                break;
            }
            push_char(&mut self.buffer, ch, offset)?;
            self.reader.decoded += ch.len_utf8();
        }
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
            parser.mark.index += width as u64;
        }
    }
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
