// workflint parses a `&str`, so this reader replaces upstream's byte reader,
// which decoded UTF-8 and UTF-16 from a `BufRead` and needed one `unsafe`
// block. The character checks in `push_char` are upstream's.

use std::collections::VecDeque;

use super::scanner::Scanner;
use super::{Encoding, Error, Result};

const BOM: char = '\u{feff}';

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

/// Moves characters from the input into the scanner's buffer until the buffer
/// holds `length` characters or the input ends.
///
/// The first call skips a leading byte order mark. The scanner's mark still
/// counts its bytes, so `Mark::index` stays a byte offset into the source,
/// while the column of the first character stays 0, as in libyaml.
pub(crate) fn yaml_parser_update_buffer(parser: &mut Scanner<'_>, length: usize) -> Result<()> {
    if parser.buffer.len() >= length {
        return Ok(());
    }
    if parser.encoding == Encoding::Any {
        parser.encoding = Encoding::Utf8;
        if parser.input.as_str().starts_with(BOM) {
            parser.input.next();
            parser.offset += BOM.len_utf8();
            parser.mark.index += BOM.len_utf8() as u64;
        }
    }
    while parser.buffer.len() < length {
        let Some(ch) = parser.input.next() else {
            return Ok(());
        };
        push_char(&mut parser.buffer, ch, parser.offset)?;
        parser.offset += ch.len_utf8();
    }
    Ok(())
}
