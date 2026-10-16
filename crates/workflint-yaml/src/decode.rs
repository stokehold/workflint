//! Turns the bytes of a file into text the way go-yaml's reader does
//! (`reader.go`).

use std::borrow::Cow;
use std::fmt;

const UTF16LE_BOM: &[u8] = b"\xFF\xFE";
const UTF16BE_BOM: &[u8] = b"\xFE\xFF";

/// Bytes that go-yaml cannot decode, with go-yaml's message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError {
    problem: &'static str,
    offset: usize,
}

impl DecodeError {
    /// go-yaml's message, for example `"invalid leading UTF-8 octet"`.
    #[must_use]
    pub fn problem(&self) -> &'static str {
        self.problem
    }

    /// The byte offset in the input where go-yaml reports the problem.
    #[must_use]
    pub fn offset(&self) -> usize {
        self.offset
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.problem, self.offset)
    }
}

impl std::error::Error for DecodeError {}

/// Decodes the bytes of a YAML file into text, as go-yaml does before it
/// parses them.
///
/// A file that starts with the byte order mark `FF FE` is UTF-16LE, and one
/// that starts with `FE FF` is UTF-16BE. Any other file is UTF-8, with or
/// without a byte order mark. UTF-8 text is returned as it is, with its byte
/// order mark, which the parser skips. UTF-16 text is returned without its
/// byte order mark.
///
/// Bytes that go-yaml rejects give an error with go-yaml's message. go-yaml
/// decodes the input in chunks while it parses, so it reports a syntax error
/// before an encoding error that comes later in the file. This function
/// decodes the whole input first, so it reports the encoding error.
///
/// ```
/// use workflint_yaml::decode;
///
/// assert_eq!(decode(b"a: 1\n").unwrap(), "a: 1\n");
/// assert_eq!(decode(b"\xFF\xFEa\0:\0 \x001\0").unwrap(), "a: 1");
/// assert_eq!(decode(b"a: \xFF").unwrap_err().problem(), "invalid leading UTF-8 octet");
/// ```
///
/// # Errors
///
/// Returns the first problem go-yaml finds in `bytes`.
pub fn decode(bytes: &[u8]) -> Result<Cow<'_, str>, DecodeError> {
    if let Some(units) = bytes.strip_prefix(UTF16LE_BOM) {
        decode_utf16(units, u16::from_le_bytes).map(Cow::Owned)
    } else if let Some(units) = bytes.strip_prefix(UTF16BE_BOM) {
        decode_utf16(units, u16::from_be_bytes).map(Cow::Owned)
    } else {
        match std::str::from_utf8(bytes) {
            Ok(text) => Ok(Cow::Borrowed(text)),
            Err(err) => Err(utf8_error(bytes, err.valid_up_to())),
        }
    }
}

fn error(problem: &'static str, offset: usize) -> DecodeError {
    DecodeError { problem, offset }
}

/// Finds go-yaml's message for the invalid UTF-8 sequence at `offset`, the
/// first byte that `std::str::from_utf8` rejects.
fn utf8_error(bytes: &[u8], offset: usize) -> DecodeError {
    // go-yaml skips a UTF-8 byte order mark before it decodes, and Rust
    // accepts the mark, so `offset` is never inside it.
    let rest = bytes.get(offset..).unwrap_or_default();
    let Some(&lead) = rest.first() else {
        return error("incomplete UTF-8 octet sequence", offset);
    };
    let width = match lead {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => return error("invalid leading UTF-8 octet", offset),
    };
    if width > rest.len() {
        return error("incomplete UTF-8 octet sequence", offset);
    }
    let mut value = u32::from(lead & (0xFF >> (width + 1)));
    for (k, &octet) in rest.iter().enumerate().take(width).skip(1) {
        if octet & 0xC0 != 0x80 {
            return error("invalid trailing UTF-8 octet", offset + k);
        }
        value = (value << 6) + u32::from(octet & 0x3F);
    }
    let shortest = match width {
        1 => true,
        2 => value >= 0x80,
        3 => value >= 0x800,
        _ => value >= 0x1_0000,
    };
    if !shortest {
        return error("invalid length of a UTF-8 sequence", offset);
    }
    // What remains is a surrogate or a value above U+10FFFF, the only other
    // sequences that Rust rejects.
    error("invalid Unicode character", offset)
}

/// Decodes UTF-16 code units after the byte order mark, as go-yaml does.
/// Offsets count from the start of the file, including the mark.
fn decode_utf16(units: &[u8], unit: fn([u8; 2]) -> u16) -> Result<String, DecodeError> {
    let mut text = String::with_capacity(units.len() / 2);
    let mut pos = 0;
    let offset = |pos: usize| pos + UTF16LE_BOM.len();
    while pos < units.len() {
        let Some(&[a, b]) = units.get(pos..pos + 2) else {
            return Err(error("incomplete UTF-16 character", offset(pos)));
        };
        let value = unit([a, b]);
        if value & 0xFC00 == 0xDC00 {
            return Err(error("unexpected low surrogate area", offset(pos)));
        }
        let ch = if value & 0xFC00 == 0xD800 {
            let Some(&[c, d]) = units.get(pos + 2..pos + 4) else {
                return Err(error("incomplete UTF-16 surrogate pair", offset(pos)));
            };
            let low = unit([c, d]);
            if low & 0xFC00 != 0xDC00 {
                return Err(error("expected low surrogate area", offset(pos + 2)));
            }
            pos += 4;
            0x1_0000 + ((u32::from(value) & 0x3FF) << 10) + (u32::from(low) & 0x3FF)
        } else {
            pos += 2;
            u32::from(value)
        };
        // Every value here is a scalar value: a surrogate pair gives at most
        // U+10FFFF, and a lone surrogate returned above.
        text.push(char::from_u32(ch).unwrap_or(char::REPLACEMENT_CHARACTER));
    }
    Ok(text)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn problem(bytes: &[u8]) -> (&'static str, usize) {
        let err = decode(bytes).unwrap_err();
        (err.problem(), err.offset())
    }

    fn utf16le(text: &str) -> Vec<u8> {
        let mut bytes = UTF16LE_BOM.to_vec();
        bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        bytes
    }

    fn utf16be(text: &str) -> Vec<u8> {
        let mut bytes = UTF16BE_BOM.to_vec();
        bytes.extend(text.encode_utf16().flat_map(u16::to_be_bytes));
        bytes
    }

    #[test]
    fn utf8_is_returned_as_it_is_with_its_byte_order_mark() {
        assert!(matches!(decode(b"a: 1\n"), Ok(Cow::Borrowed("a: 1\n"))));
        assert_eq!(decode(b"\xEF\xBB\xBFa: 1\n").unwrap(), "\u{feff}a: 1\n");
        assert_eq!(decode(b"").unwrap(), "");
    }

    #[test]
    fn utf16_by_byte_order_mark_is_decoded_without_the_mark() {
        let text = "a: \u{e9}\nb: \u{1f600}\n";
        assert_eq!(decode(&utf16le(text)).unwrap(), text);
        assert_eq!(decode(&utf16be(text)).unwrap(), text);
    }

    #[test]
    fn utf16_without_a_byte_order_mark_is_read_as_utf8() {
        // go-yaml then rejects the NUL bytes as control characters.
        assert_eq!(decode(b"a\0:\0").unwrap(), "a\0:\0");
    }

    #[test]
    fn invalid_utf8_gives_go_yaml_messages() {
        assert_eq!(problem(b"a: \xFF"), ("invalid leading UTF-8 octet", 3));
        assert_eq!(problem(b"a: \x80"), ("invalid leading UTF-8 octet", 3));
        assert_eq!(problem(b"a: \xC3"), ("incomplete UTF-8 octet sequence", 3));
        assert_eq!(
            problem(b"a: \xE2\x82"),
            ("incomplete UTF-8 octet sequence", 3)
        );
        assert_eq!(problem(b"a: \xC3\x28"), ("invalid trailing UTF-8 octet", 4));
        assert_eq!(
            problem(b"a: \xE2\x82\x28"),
            ("invalid trailing UTF-8 octet", 5)
        );
        assert_eq!(
            problem(b"a: \xC0\x80"),
            ("invalid length of a UTF-8 sequence", 3)
        );
        assert_eq!(
            problem(b"a: \xE0\x80\x80"),
            ("invalid length of a UTF-8 sequence", 3)
        );
        assert_eq!(
            problem(b"a: \xED\xA0\x80"),
            ("invalid Unicode character", 3)
        );
        assert_eq!(
            problem(b"a: \xF4\x90\x80\x80"),
            ("invalid Unicode character", 3)
        );
        assert_eq!(
            problem(b"a: \xF8\x88\x80\x80"),
            ("invalid leading UTF-8 octet", 3)
        );
    }

    #[test]
    fn invalid_utf16_gives_go_yaml_messages() {
        let mut odd = utf16le("a: 1");
        odd.push(b'x');
        assert_eq!(problem(&odd), ("incomplete UTF-16 character", 10));
        assert_eq!(
            problem(b"\xFF\xFEa\0\x00\xDC"),
            ("unexpected low surrogate area", 4)
        );
        assert_eq!(
            problem(b"\xFF\xFEa\0\x00\xD8"),
            ("incomplete UTF-16 surrogate pair", 4)
        );
        assert_eq!(
            problem(b"\xFF\xFEa\0\x00\xD8a\0"),
            ("expected low surrogate area", 6)
        );
        assert_eq!(
            problem(b"\xFE\xFF\0a\xD8\x00\0a"),
            ("expected low surrogate area", 6)
        );
    }
}
