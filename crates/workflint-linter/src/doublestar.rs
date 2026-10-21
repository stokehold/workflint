//! Glob patterns of `paths` in the config, matched as the reference matches them with
//! `doublestar.MatchUnvalidated` and checked with `doublestar.ValidatePattern` of
//! github.com/bmatcuk/doublestar v4.10.0, with `/` as the separator.
//!
//! `*` matches any text without `/`, `**` as a whole path segment matches any number of
//! segments, `?` matches one character, `[...]` and `[^...]` or `[!...]` match a character in
//! or out of a class, `{a,b}` matches either alternative, and `\` escapes the next character.
//! The functions follow doublestar's code, so that a pattern matches the same paths.

/// A pattern that doublestar finds malformed while matching, its `ErrBadPattern`.
struct BadPattern;

/// Returns whether `pattern` is well formed, as `doublestar.ValidatePattern` does: every `[`
/// opens a non-empty class that a `]` closes, every `{` is closed, no `}` comes without a `{`,
/// and the pattern does not end in a `\`.
pub(crate) fn validate(pattern: &str) -> bool {
    let s = pattern.as_bytes();
    let len = s.len();
    let mut alt_depth = 0_usize;
    let mut i = 0;
    'validate: while i < len {
        match s[i] {
            b'\\' => {
                // Skip the next byte, which must exist.
                i += 1;
                if i >= len {
                    return false;
                }
            }
            b'[' => {
                i += 1;
                if i >= len {
                    return false;
                }
                if s[i] == b'^' || s[i] == b'!' {
                    i += 1;
                }
                if i >= len || s[i] == b']' {
                    return false;
                }
                while i < len {
                    if s[i] == b'\\' {
                        i += 1;
                    } else if s[i] == b']' {
                        i += 1;
                        continue 'validate;
                    }
                    i += 1;
                }
                return false;
            }
            b'{' => alt_depth += 1,
            b'}' => {
                if alt_depth == 0 {
                    return false;
                }
                alt_depth -= 1;
            }
            _ => {}
        }
        i += 1;
    }
    alt_depth == 0
}

/// Returns whether `name` matches `pattern`, as `doublestar.MatchUnvalidated` does. A pattern
/// that turns out malformed while matching does not match.
pub(crate) fn is_match(pattern: &str, name: &str) -> bool {
    let start = Backtrack {
        doublestar_pattern: None,
        doublestar_name: 0,
        star_pattern: None,
        star_name: 0,
    };
    matches!(
        do_match(pattern.as_bytes(), name.as_bytes(), start, 0, 0),
        Ok(true)
    )
}

/// Where `do_match` goes back to after a mismatch: after the last `**/`, and after the last
/// `*`, in the pattern and in the name.
#[derive(Clone, Copy)]
struct Backtrack {
    doublestar_pattern: Option<usize>,
    doublestar_name: usize,
    star_pattern: Option<usize>,
    star_name: usize,
}

const SEPARATOR: char = '/';

/// Decodes the character at byte `index` of `s`, as Go's `utf8.DecodeRuneInString` does: a byte
/// that starts no valid character is U+FFFD of length 1, and the end of `s` is U+FFFD of
/// length 0.
fn decode(s: &[u8], index: usize) -> (char, usize) {
    let Some(rest) = s.get(index..).filter(|rest| !rest.is_empty()) else {
        return (char::REPLACEMENT_CHARACTER, 0);
    };
    match rest.utf8_chunks().next() {
        Some(chunk) => match chunk.valid().chars().next() {
            Some(c) => (c, c.len_utf8()),
            None => (char::REPLACEMENT_CHARACTER, 1),
        },
        None => (char::REPLACEMENT_CHARACTER, 1),
    }
}

/// doublestar's `doMatchWithSeparator`, from byte `pat_idx` of `pattern` and byte `name_idx`
/// of `name`.
#[allow(clippy::too_many_lines)] // One function, as in doublestar.
fn do_match(
    pattern: &[u8],
    name: &[u8],
    mut back: Backtrack,
    mut pat_idx: usize,
    mut name_idx: usize,
) -> Result<bool, BadPattern> {
    let pat_len = pattern.len();
    let name_len = name.len();
    let mut start_of_segment = true;
    'outer: while name_idx < name_len {
        'mismatch: {
            if pat_idx >= pat_len {
                break 'mismatch;
            }
            match pattern[pat_idx] {
                b'*' => {
                    pat_idx += 1;
                    if pat_idx < pat_len && pattern[pat_idx] == b'*' {
                        // `**`
                        pat_idx += 1;
                        if start_of_segment {
                            if pat_idx >= pat_len {
                                // `**` at the end of the pattern matches the rest of the name.
                                return Ok(true);
                            }
                            let (pat_rune, pat_rune_len) = decode(pattern, pat_idx);
                            if pat_rune == SEPARATOR {
                                pat_idx += pat_rune_len;
                                back.doublestar_pattern = Some(pat_idx);
                                back.doublestar_name = name_idx;
                                back.star_pattern = None;
                                back.star_name = 0;
                                continue 'outer;
                            }
                        }
                    }
                    start_of_segment = false;
                    back.star_pattern = Some(pat_idx);
                    back.star_name = name_idx;
                    continue 'outer;
                }
                b'?' => {
                    start_of_segment = false;
                    let (name_rune, name_rune_len) = decode(name, name_idx);
                    if name_rune == SEPARATOR {
                        break 'mismatch;
                    }
                    pat_idx += 1;
                    name_idx += name_rune_len;
                    continue 'outer;
                }
                b'[' => {
                    start_of_segment = false;
                    pat_idx += 1;
                    if pat_idx >= pat_len {
                        return Err(BadPattern);
                    }
                    let (name_rune, name_rune_len) = decode(name, name_idx);
                    let mut matched = false;
                    let negate = pattern[pat_idx] == b'!' || pattern[pat_idx] == b'^';
                    if negate {
                        pat_idx += 1;
                    }
                    if pat_idx >= pat_len || pattern[pat_idx] == b']' {
                        return Err(BadPattern);
                    }
                    let mut last: Option<char> = None;
                    while pat_idx < pat_len && pattern[pat_idx] != b']' {
                        let (mut pat_rune, mut pat_rune_len) = decode(pattern, pat_idx);
                        pat_idx += pat_rune_len;
                        // A range, whose low end is `last`.
                        if let Some(low) = last
                            && pat_rune == '-'
                            && pat_idx < pat_len
                            && pattern[pat_idx] != b']'
                        {
                            if pattern[pat_idx] == b'\\' {
                                pat_idx += 1;
                            }
                            (pat_rune, pat_rune_len) = decode(pattern, pat_idx);
                            pat_idx += pat_rune_len;
                            if low <= name_rune && name_rune <= pat_rune {
                                matched = true;
                                break;
                            }
                            last = None;
                            continue;
                        }
                        if pat_rune == '\\' {
                            (pat_rune, pat_rune_len) = decode(pattern, pat_idx);
                            pat_idx += pat_rune_len;
                        }
                        if pat_rune == name_rune {
                            matched = true;
                            break;
                        }
                        // Go marks "no low end" with `utf8.MaxRune`, so that character
                        // starts no range.
                        last = (pat_rune != char::MAX).then_some(pat_rune);
                    }
                    if matched == negate {
                        if pat_idx >= pat_len {
                            return Err(BadPattern);
                        }
                        break 'mismatch;
                    }
                    let Some(closing) = index_unescaped_byte(&pattern[pat_idx..], b']') else {
                        return Err(BadPattern);
                    };
                    pat_idx += closing + 1;
                    name_idx += name_rune_len;
                    continue 'outer;
                }
                b'{' => {
                    let before_idx = pat_idx;
                    pat_idx += 1;
                    let Some(closing) = index_matched_closing_alt(&pattern[pat_idx..]) else {
                        return Err(BadPattern);
                    };
                    let closing = closing + pat_idx;
                    while let Some(comma) = index_next_alt(&pattern[pat_idx..closing]) {
                        let comma = comma + pat_idx;
                        let alternative = [
                            &pattern[..before_idx],
                            &pattern[pat_idx..comma],
                            &pattern[closing + 1..],
                        ]
                        .concat();
                        if do_match(&alternative, name, back, before_idx, name_idx)? {
                            return Ok(true);
                        }
                        pat_idx = comma + 1;
                    }
                    let alternative = [
                        &pattern[..before_idx],
                        &pattern[pat_idx..closing],
                        &pattern[closing + 1..],
                    ]
                    .concat();
                    return do_match(&alternative, name, back, before_idx, name_idx);
                }
                byte => {
                    if byte == b'\\' {
                        pat_idx += 1;
                        if pat_idx >= pat_len {
                            return Err(BadPattern);
                        }
                    }
                    let (pat_rune, pat_rune_len) = decode(pattern, pat_idx);
                    let (name_rune, name_rune_len) = decode(name, name_idx);
                    if pat_rune != name_rune {
                        break 'mismatch;
                    }
                    pat_idx += pat_rune_len;
                    name_idx += name_rune_len;
                    start_of_segment = pat_rune == SEPARATOR;
                    continue 'outer;
                }
            }
        }

        // A mismatch: try more of the name for the last `*`, then for the last `**/`.
        if let Some(star_pattern) = back.star_pattern {
            let (name_rune, name_rune_len) = decode(name, back.star_name);
            if name_rune != SEPARATOR {
                back.star_name += name_rune_len;
                pat_idx = star_pattern;
                name_idx = back.star_name;
                start_of_segment = false;
                continue 'outer;
            }
        }
        if let Some(doublestar_pattern) = back.doublestar_pattern {
            name_idx = back.doublestar_name;
            while name_idx < name_len {
                let (name_rune, name_rune_len) = decode(name, name_idx);
                name_idx += name_rune_len;
                if name_rune == SEPARATOR {
                    back.doublestar_name = name_idx;
                    pat_idx = doublestar_pattern;
                    start_of_segment = true;
                    continue 'outer;
                }
            }
        }
        return Ok(false);
    }
    is_zero_length_pattern(pattern.get(pat_idx..).unwrap_or_default())
}

/// doublestar's `isZeroLengthPattern`: whether what is left of the pattern matches the empty
/// rest of the name.
fn is_zero_length_pattern(pattern: &[u8]) -> Result<bool, BadPattern> {
    match pattern {
        b"" | b"*" | b"**" | b"/**" | b"**/" | b"/**/" => return Ok(true),
        _ => {}
    }
    if pattern.first() == Some(&b'{') {
        let Some(closing) = index_matched_closing_alt(&pattern[1..]) else {
            return Err(BadPattern);
        };
        let closing = closing + 1;
        let mut pat_idx = 1;
        while let Some(comma) = index_next_alt(&pattern[pat_idx..closing]) {
            let comma = comma + pat_idx;
            let alternative = [&pattern[pat_idx..comma], &pattern[closing + 1..]].concat();
            if is_zero_length_pattern(&alternative)? {
                return Ok(true);
            }
            pat_idx = comma + 1;
        }
        let alternative = [&pattern[pat_idx..closing], &pattern[closing + 1..]].concat();
        return is_zero_length_pattern(&alternative);
    }
    Ok(false)
}

/// The index of the first `c` in `s` that no `\` escapes.
fn index_unescaped_byte(s: &[u8], c: u8) -> Option<usize> {
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'\\' {
            i += 1;
        } else if s[i] == c {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// The index of the `}` that closes the `{` just before `s`.
fn index_matched_closing_alt(s: &[u8]) -> Option<usize> {
    let mut alts = 1_usize;
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'\\' {
            i += 1;
        } else if s[i] == b'{' {
            alts += 1;
        } else if s[i] == b'}' {
            alts -= 1;
            if alts == 0 {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// The index of the next `,` in `s` that separates alternatives of the outermost `{`.
fn index_next_alt(s: &[u8]) -> Option<usize> {
    let mut alts = 1_usize;
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'\\' {
            i += 1;
        } else if s[i] == b'{' {
            alts += 1;
        } else if s[i] == b'}' {
            alts = alts.saturating_sub(1);
        } else if s[i] == b',' && alts == 1 {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cases of `matchTests` in doublestar's `doublestar_test.go` that match with case, each
    /// with the results of `MatchUnvalidated` and `ValidatePattern` that doublestar v4.10.0
    /// gives.
    const CASES: &[(&str, &str, bool, bool)] = &[
        ("", "", true, true),
        ("*", "", true, true),
        ("*", "/", false, true),
        ("/*", "/", true, true),
        ("/*", "/debug/", false, true),
        ("/*", "//", false, true),
        ("abc", "abc", true, true),
        ("*", "abc", true, true),
        ("*c", "abc", true, true),
        ("*/", "a/", true, true),
        ("a*", "a", true, true),
        ("a*", "abc", true, true),
        ("a*", "ab/c", false, true),
        ("a*/b", "abc/b", true, true),
        ("a*/b", "a/c/b", false, true),
        ("a*/c/", "a/b", false, true),
        ("a*b*c*d*e*", "axbxcxdxe", true, true),
        ("a*b*c*d*e*/f", "axbxcxdxe/f", true, true),
        ("a*b*c*d*e*/f", "axbxcxdxexxx/f", true, true),
        ("a*b*c*d*e*/f", "axbxcxdxe/xxx/f", false, true),
        ("a*b*c*d*e*/f", "axbxcxdxexxx/fff", false, true),
        ("a*b?c*x", "abxbbxdbxebxczzx", true, true),
        ("a*b?c*x", "abxbbxdbxebxczzy", false, true),
        ("ab[c]", "abc", true, true),
        ("ab[b-d]", "abc", true, true),
        ("ab[e-g]", "abc", false, true),
        ("ab[^c]", "abc", false, true),
        ("ab[^b-d]", "abc", false, true),
        ("ab[^e-g]", "abc", true, true),
        ("a\\*b", "ab", false, true),
        ("a?b", "a☺b", true, true),
        ("a[^a]b", "a☺b", true, true),
        ("a[!a]b", "a☺b", true, true),
        ("a???b", "a☺b", false, true),
        ("a[^a][^a][^a]b", "a☺b", false, true),
        ("[a-ζ]*", "α", true, true),
        ("*[a-ζ]", "A", false, true),
        ("a?b", "a/b", false, true),
        ("a*b", "a/b", false, true),
        ("[\\]a]", "]", true, true),
        ("[\\-]", "-", true, true),
        ("[x\\-]", "x", true, true),
        ("[x\\-]", "-", true, true),
        ("[x\\-]", "z", false, true),
        ("[\\-x]", "x", true, true),
        ("[\\-x]", "-", true, true),
        ("[\\-x]", "a", false, true),
        ("[]a]", "]", false, false),
        ("[-]", "-", true, true),
        ("[x-]", "x", true, true),
        ("[x-]", "-", true, true),
        ("[x-]", "z", false, true),
        ("[-x]", "x", true, true),
        ("[-x]", "-", true, true),
        ("[-x]", "a", false, true),
        ("[a-b-d]", "a", true, true),
        ("[a-b-d]", "b", true, true),
        ("[a-b-d]", "-", true, true),
        ("[a-b-d]", "c", false, true),
        ("[a-b-x]", "x", true, true),
        ("\\", "a", false, false),
        ("[", "a", false, false),
        ("[^", "a", false, false),
        ("[^bc", "a", false, false),
        ("a[", "a", false, false),
        ("a[", "ab", false, false),
        ("ad[", "ab", false, false),
        ("*x", "xxx", true, true),
        ("[abc]", "b", true, true),
        ("[abc123]", "1", true, true),
        ("[a-z0-9]", "1", true, true),
        ("**", "", true, true),
        ("a/**", "a", true, true),
        ("a/**/", "a", true, true),
        ("a/**", "a/", true, true),
        ("a/**/", "a/", true, true),
        ("a/**", "a/b", true, true),
        ("a/**", "a/b/c", true, true),
        ("**/c", "c", true, true),
        ("**/c", "b/c", true, true),
        ("**/c", "a/b/c", true, true),
        ("**/c", "a/b", false, true),
        ("**/c", "abcd", false, true),
        ("**/c", "a/abc", false, true),
        ("a/**/b", "a/b", true, true),
        ("a/**/c", "a/b/c", true, true),
        ("a/**/d", "a/b/c/d", true, true),
        ("a/\\**", "a/b/c", false, true),
        ("a/\\[*\\]", "a/bc", false, true),
        ("a/b/c", "a/b//c", false, true),
        ("a/", "a", false, true),
        ("ab{c,d}", "abc", true, true),
        ("ab{c,d,*}", "abcde", true, true),
        ("ab{c,d}[", "abcd", false, false),
        ("a{,bc}", "a", true, true),
        ("a{,bc}", "abc", true, true),
        ("a/{b/c,c/b}", "a/b/c", true, true),
        ("a/{b/c,c/b}", "a/c/b", true, true),
        ("a/a*{b,c}", "a/abc", true, true),
        ("{a/{b,c},abc}", "a/b", true, true),
        ("{a/{b,c},abc}", "a/c", true, true),
        ("{a/{b,c},abc}", "abc", true, true),
        ("{a/{b,c},abc}", "a/b/c", false, true),
        ("{a/ab*}", "a/abc", true, true),
        ("{a/*}", "a/b", true, true),
        ("{a/abc}", "a/abc", true, true),
        ("{a/b,a/c}", "a/c", true, true),
        ("abc/**", "abc/b", true, true),
        ("**/abc", "abc", true, true),
        ("abc**", "abc/b", false, true),
        ("**/*.txt", "abc/【test】.txt", true, true),
        ("**/【*", "abc/【test】.txt", true, true),
        ("**/{a,b}", "a/b", true, true),
        ("a/*/*/d", "a/b/c/d", true, true),
        ("broken-symlink", "broken-symlink", true, true),
        ("broken-symlink/*", "a", false, true),
        ("broken*/*", "a", false, true),
        ("working-symlink/c/*", "working-symlink/c/d", true, true),
        ("working-sym*/*", "working-symlink/c", true, true),
        ("b/**/f", "b/symlink-dir/f", true, true),
        ("*/symlink-dir/*", "b/symlink-dir/f", true, true),
        ("e/\\[x\\]/*", "e/[x]/[y]", true, true),
        ("e/\\[x\\]/*/z", "e/[x]/[y]/z", true, true),
        ("e/**/{z,other}", "e/[x]/[y]/z", true, true),
        ("f/\\{a,b\\}/{c,other*}", "f/{a,b}/c", true, true),
        ("f/\\*/{a,other*}", "f/*/a", true, true),
        ("f/\\?/{a,other*}", "f/?/a", true, true),
        ("e/**", "e/**", true, true),
        ("e/**", "e/*", true, true),
        ("e/**", "e/?", true, true),
        ("e/**", "e/[", true, true),
        ("e/**", "e/]", true, true),
        ("e/**", "e/[]", true, true),
        ("e/**", "e/{", true, true),
        ("e/**", "e/}", true, true),
        ("e/**", "e/\\", true, true),
        ("e/*", "e/*", true, true),
        ("e/?", "e/?", true, true),
        ("e/?", "e/*", true, true),
        ("e/?", "e/[", true, true),
        ("e/?", "e/]", true, true),
        ("e/?", "e/{", true, true),
        ("e/?", "e/}", true, true),
        ("e/\\[", "e/[", true, true),
        ("e/[", "e/[", false, false),
        ("e/]", "e/]", true, true),
        ("e/\\]", "e/]", true, true),
        ("e/\\{", "e/{", true, true),
        ("e/\\}", "e/}", true, true),
        ("e/[\\*\\?]", "e/*", true, true),
        ("e/[\\*\\?]", "e/?", true, true),
        ("e/[\\*\\?]", "e/**", false, true),
        ("e/[\\*\\?]?", "e/**", true, true),
        ("e/{\\*,\\?}", "e/*", true, true),
        ("e/{\\*,\\?}", "e/?", true, true),
        ("e/\\*", "e/*", true, true),
        ("e/\\?", "e/?", true, true),
        ("e/\\?", "e/**", false, true),
        ("*\\}", "}", true, true),
        ("case-sensitive/fi*", "case-sensitive/file", true, true),
        ("nonexistent-path", "a", false, true),
        ("nonexistent-path/", "a", false, true),
        ("nonexistent-path/file", "a", false, true),
        ("nonexistent-path/*", "a", false, true),
        ("nonexistent-path/**", "a", false, true),
        ("nopermission/*", "nopermission/file", true, true),
        ("nopermission/dir/", "nopermission/dir", false, true),
        ("nopermission/file", "nopermission/file", true, true),
        ("hidden-tests/.*", "hidden-tests/.hidden-file", true, true),
        (
            "hidden-tests/?hidden*",
            "hidden-tests/.hidden-file",
            true,
            true,
        ),
        ("hidden-tests/*", "hidden-tests/visible-file", true, true),
        (
            "hidden-tests/**",
            "hidden-tests/.hidden-dir/.hidden-file",
            true,
            true,
        ),
        (
            "hidden-tests/**",
            "hidden-tests/hidden-dir/hidden-file",
            true,
            true,
        ),
        (
            "hidden-tests/.hidden-dir/*",
            "hidden-tests/.hidden-dir/.hidden-file",
            true,
            true,
        ),
        (
            "hidden-tests/hidden-dir/*",
            "hidden-tests/hidden-dir/hidden-file",
            true,
            true,
        ),
    ];

    #[test]
    fn patterns_match_and_validate_as_in_doublestar() {
        for &(pattern, name, matches, valid) in CASES {
            assert_eq!(is_match(pattern, name), matches, "{pattern:?} on {name:?}");
            assert_eq!(validate(pattern), valid, "{pattern:?}");
        }
    }
}
