//! Go's `path.Match`, which matches the runner label patterns of the config.
//!
//! Ported from Go 1.26.4: `src/path/match.go`. It works on bytes, as Go does, so `?` and a
//! character class read one UTF-8 sequence, and a literal byte may stop inside one.

use std::fmt;

/// The pattern is malformed, Go's `path.ErrBadPattern`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BadPattern;

impl fmt::Display for BadPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("syntax error in pattern")
    }
}

/// Returns whether all of `name` matches the shell pattern `pattern`, as Go's `path.Match` does.
///
/// `*` matches any run of bytes other than `/`, `?` matches one character other than `/`,
/// `[...]` and `[^...]` match a character in or out of a class of characters and ranges, and
/// `\` escapes the next character. The error is [`BadPattern`] when the pattern is malformed.
/// Like Go, it may return `Ok(false)` for a malformed pattern whose error comes after the point
/// where the match failed inside a chunk.
pub(crate) fn path_match(pattern: &str, name: &str) -> Result<bool, BadPattern> {
    let mut pattern = pattern.as_bytes();
    let mut name = name.as_bytes();
    'pattern: while !pattern.is_empty() {
        let (star, chunk, rest) = scan_chunk(pattern);
        pattern = rest;
        if star && chunk.is_empty() {
            // A trailing `*` matches the rest of the name unless it has a `/`.
            return Ok(!name.contains(&b'/'));
        }
        // Look for a match at the current position. The last chunk must use up the name, or
        // a match further on through the star could be missed.
        let matched = match_chunk(chunk, name);
        if let Ok(Some(t)) = matched
            && (t.is_empty() || !pattern.is_empty())
        {
            name = t;
            continue;
        }
        matched?;
        if star {
            // Look for a match after skipping i+1 bytes, which cannot skip a `/`.
            let mut i = 0;
            while let Some(&byte) = name.get(i)
                && byte != b'/'
            {
                let skipped = name.get(i + 1..).unwrap_or_default();
                if let Some(t) = match_chunk(chunk, skipped)?
                    && (!pattern.is_empty() || t.is_empty())
                {
                    name = t;
                    continue 'pattern;
                }
                i += 1;
            }
        }
        // Before returning false, check that the rest of the pattern is well formed.
        while !pattern.is_empty() {
            let (_, chunk, rest) = scan_chunk(pattern);
            pattern = rest;
            match_chunk(chunk, b"")?;
        }
        return Ok(false);
    }
    Ok(name.is_empty())
}

/// Splits the next chunk off `pattern`: whether stars come first, then the bytes up to the next
/// star outside a class, then the rest.
fn scan_chunk(mut pattern: &[u8]) -> (bool, &[u8], &[u8]) {
    let mut star = false;
    while let [b'*', rest @ ..] = pattern {
        pattern = rest;
        star = true;
    }
    let mut in_range = false;
    let mut i = 0;
    while let Some(&byte) = pattern.get(i) {
        match byte {
            // `match_chunk` reports a `\` at the end.
            b'\\' if i + 1 < pattern.len() => i += 1,
            b'[' => in_range = true,
            b']' => in_range = false,
            b'*' if !in_range => return (star, &pattern[..i], &pattern[i..]),
            _ => {}
        }
        i += 1;
    }
    (star, pattern, &[])
}

/// Matches `chunk`, which has no star, at the start of `s`, and returns the rest of `s`, or
/// `None` if it does not match. After a mismatch it reads the rest of the chunk to check that it
/// is well formed.
fn match_chunk<'s>(mut chunk: &[u8], mut s: &'s [u8]) -> Result<Option<&'s [u8]>, BadPattern> {
    let mut failed = false;
    while let Some(&first) = chunk.first() {
        failed = failed || s.is_empty();
        match first {
            b'[' => {
                let mut r = 0;
                if !failed {
                    let (decoded, n) = decode_rune(s);
                    r = decoded;
                    s = s.get(n..).unwrap_or_default();
                }
                chunk = chunk.get(1..).unwrap_or_default();
                let negated = chunk.first() == Some(&b'^');
                if negated {
                    chunk = chunk.get(1..).unwrap_or_default();
                }
                let mut matched = false;
                let mut ranges = 0;
                loop {
                    if chunk.first() == Some(&b']') && ranges > 0 {
                        chunk = chunk.get(1..).unwrap_or_default();
                        break;
                    }
                    let (lo, rest) = get_esc(chunk)?;
                    chunk = rest;
                    let mut hi = lo;
                    if chunk.first() == Some(&b'-') {
                        let (escaped, rest) = get_esc(chunk.get(1..).unwrap_or_default())?;
                        hi = escaped;
                        chunk = rest;
                    }
                    matched = matched || (lo <= r && r <= hi);
                    ranges += 1;
                }
                failed = failed || matched == negated;
            }
            b'?' => {
                if !failed {
                    failed = s.first() == Some(&b'/');
                    let (_, n) = decode_rune(s);
                    s = s.get(n..).unwrap_or_default();
                }
                chunk = chunk.get(1..).unwrap_or_default();
            }
            _ => {
                let mut literal = first;
                if first == b'\\' {
                    chunk = chunk.get(1..).unwrap_or_default();
                    let Some(&escaped) = chunk.first() else {
                        return Err(BadPattern);
                    };
                    literal = escaped;
                }
                if !failed {
                    failed = s.first() != Some(&literal);
                    s = s.get(1..).unwrap_or_default();
                }
                chunk = chunk.get(1..).unwrap_or_default();
            }
        }
    }
    Ok((!failed).then_some(s))
}

/// Reads a character of a class, which may be escaped, and returns it with the rest of the
/// chunk. A class must not end here, and the character must not be `-` or `]` unescaped.
fn get_esc(mut chunk: &[u8]) -> Result<(u32, &[u8]), BadPattern> {
    match chunk.first() {
        None | Some(b'-' | b']') => return Err(BadPattern),
        Some(b'\\') => {
            chunk = chunk.get(1..).unwrap_or_default();
            if chunk.is_empty() {
                return Err(BadPattern);
            }
        }
        Some(_) => {}
    }
    let (r, n) = decode_rune(chunk);
    let rest = chunk.get(n..).unwrap_or_default();
    if (r == REPLACEMENT && n == 1) || rest.is_empty() {
        return Err(BadPattern);
    }
    Ok((r, rest))
}

/// Go's `utf8.RuneError`.
const REPLACEMENT: u32 = 0xFFFD;

/// Decodes the first character of `s` as Go's `utf8.DecodeRuneInString` does: `(RuneError, 0)`
/// for an empty slice and `(RuneError, 1)` for a byte that does not start a valid sequence.
fn decode_rune(s: &[u8]) -> (u32, usize) {
    if s.is_empty() {
        return (REPLACEMENT, 0);
    }
    let len = s.len().min(4);
    let prefix = s.get(..len).unwrap_or_default();
    let valid = match std::str::from_utf8(prefix) {
        Ok(text) => text,
        Err(err) => std::str::from_utf8(prefix.get(..err.valid_up_to()).unwrap_or_default())
            .unwrap_or_default(),
    };
    match valid.chars().next() {
        Some(c) => (u32::from(c), c.len_utf8()),
        None => (REPLACEMENT, 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cases of Go 1.26.4's `src/path/match_test.go`: pattern, name, and the result.
    const GO_CASES: [(&str, &str, Result<bool, BadPattern>); 57] = [
        ("abc", "abc", Ok(true)),
        ("*", "abc", Ok(true)),
        ("*c", "abc", Ok(true)),
        ("a*", "a", Ok(true)),
        ("a*", "abc", Ok(true)),
        ("a*", "ab/c", Ok(false)),
        ("a*/b", "abc/b", Ok(true)),
        ("a*/b", "a/c/b", Ok(false)),
        ("a*b*c*d*e*/f", "axbxcxdxe/f", Ok(true)),
        ("a*b*c*d*e*/f", "axbxcxdxexxx/f", Ok(true)),
        ("a*b*c*d*e*/f", "axbxcxdxe/xxx/f", Ok(false)),
        ("a*b*c*d*e*/f", "axbxcxdxexxx/fff", Ok(false)),
        ("a*b?c*x", "abxbbxdbxebxczzx", Ok(true)),
        ("a*b?c*x", "abxbbxdbxebxczzy", Ok(false)),
        ("ab[c]", "abc", Ok(true)),
        ("ab[b-d]", "abc", Ok(true)),
        ("ab[e-g]", "abc", Ok(false)),
        ("ab[^c]", "abc", Ok(false)),
        ("ab[^b-d]", "abc", Ok(false)),
        ("ab[^e-g]", "abc", Ok(true)),
        ("a\\*b", "a*b", Ok(true)),
        ("a\\*b", "ab", Ok(false)),
        ("a?b", "a☺b", Ok(true)),
        ("a[^a]b", "a☺b", Ok(true)),
        ("a???b", "a☺b", Ok(false)),
        ("a[^a][^a][^a]b", "a☺b", Ok(false)),
        ("[a-ζ]*", "α", Ok(true)),
        ("*[a-ζ]", "A", Ok(false)),
        ("a?b", "a/b", Ok(false)),
        ("a*b", "a/b", Ok(false)),
        ("[\\]a]", "]", Ok(true)),
        ("[\\-]", "-", Ok(true)),
        ("[x\\-]", "x", Ok(true)),
        ("[x\\-]", "-", Ok(true)),
        ("[x\\-]", "z", Ok(false)),
        ("[\\-x]", "x", Ok(true)),
        ("[\\-x]", "-", Ok(true)),
        ("[\\-x]", "a", Ok(false)),
        ("[]a]", "]", Err(BadPattern)),
        ("[-]", "-", Err(BadPattern)),
        ("[x-]", "x", Err(BadPattern)),
        ("[x-]", "-", Err(BadPattern)),
        ("[x-]", "z", Err(BadPattern)),
        ("[-x]", "x", Err(BadPattern)),
        ("[-x]", "-", Err(BadPattern)),
        ("[-x]", "a", Err(BadPattern)),
        ("\\", "a", Err(BadPattern)),
        ("[a-b-c]", "a", Err(BadPattern)),
        ("[", "a", Err(BadPattern)),
        ("[^", "a", Err(BadPattern)),
        ("[^bc", "a", Err(BadPattern)),
        ("a[", "a", Err(BadPattern)),
        ("a[", "ab", Err(BadPattern)),
        ("a[", "x", Err(BadPattern)),
        ("a/b[", "x", Err(BadPattern)),
        ("*x", "xxx", Ok(true)),
        ("", "", Ok(true)),
    ];

    #[test]
    fn path_match_gives_the_results_of_go_path_match() {
        for (pattern, name, want) in GO_CASES {
            assert_eq!(path_match(pattern, name), want, "{pattern:?} {name:?}");
        }
    }

    #[test]
    fn a_literal_byte_can_stop_inside_a_character_and_a_class_reads_the_rest_as_rune_error() {
        // `é` is C3 A9. The literal `\xC3` cannot be written in a `&str` pattern, but `?` and a
        // class decode from wherever the previous chunk stopped.
        assert_eq!(path_match("?", "é"), Ok(true));
        assert_eq!(path_match("[é]", "é"), Ok(true));
        assert_eq!(decode_rune(&[0xA9]), (REPLACEMENT, 1));
        assert_eq!(decode_rune("é".as_bytes()), (0xE9, 2));
        assert_eq!(decode_rune(b""), (REPLACEMENT, 0));
    }

    #[test]
    fn the_error_message_is_go_err_bad_pattern() {
        assert_eq!(BadPattern.to_string(), "syntax error in pattern");
    }

    #[test]
    fn a_malformed_class_after_a_failed_literal_is_still_an_error() {
        assert_eq!(path_match("x[", "a"), Err(BadPattern));
        assert_eq!(path_match("x*[", "a"), Err(BadPattern));
    }
}
