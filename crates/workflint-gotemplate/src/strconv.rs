//! The parts of Go's `strconv` that the parser uses for literals: integers and floats in Go
//! syntax, and unquoting of string and character literals.

/// Why an integer did not parse.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum IntError {
    Syntax,
    Range,
}

/// Go's `strconv.ParseUint(s, 0, 64)`.
pub(crate) fn parse_uint(s: &str) -> Result<u64, IntError> {
    if s.is_empty() {
        return Err(IntError::Syntax);
    }
    let bytes = s.as_bytes();
    let (base, digits) = if bytes[0] == b'0' && bytes.len() >= 3 {
        match bytes[1].to_ascii_lowercase() {
            b'b' => (2, &s[2..]),
            b'o' => (8, &s[2..]),
            b'x' => (16, &s[2..]),
            _ => (8, &s[1..]),
        }
    } else if bytes[0] == b'0' && bytes.len() > 1 {
        (8, &s[1..])
    } else {
        (10, s)
    };
    let mut n: u64 = 0;
    let mut underscores = false;
    for c in digits.chars() {
        if c == '_' {
            underscores = true;
            continue;
        }
        let digit = c.to_digit(36).ok_or(IntError::Syntax)?;
        if digit >= base {
            return Err(IntError::Syntax);
        }
        n = n
            .checked_mul(u64::from(base))
            .and_then(|n| n.checked_add(u64::from(digit)))
            .ok_or(IntError::Range)?;
    }
    if underscores && !underscore_ok(s) {
        return Err(IntError::Syntax);
    }
    Ok(n)
}

/// Go's `strconv.ParseInt(s, 0, 64)`.
pub(crate) fn parse_int(s: &str) -> Result<i64, IntError> {
    let (negative, digits) = match s.as_bytes().first() {
        Some(b'+') => (false, &s[1..]),
        Some(b'-') => (true, &s[1..]),
        _ => (false, s),
    };
    let u = parse_uint(digits)?;
    if negative {
        if u > i64::MIN.unsigned_abs() {
            return Err(IntError::Range);
        }
        Ok(0_i64.wrapping_sub_unsigned(u))
    } else {
        i64::try_from(u).map_err(|_| IntError::Range)
    }
}

/// Go's `strconv.ParseFloat(s, 64)` for the numbers the lexer produces: decimal floats and
/// hexadecimal floats with a `p` exponent, with `_` between digits. A value out of range is
/// an error, as in Go.
pub(crate) fn parse_float(s: &str) -> Option<f64> {
    if s.contains('_') && !underscore_ok(s) {
        return None;
    }
    let cleaned: String = s.chars().filter(|c| *c != '_').collect();
    let (negative, body) = match cleaned.as_bytes().first() {
        Some(b'+') => (false, &cleaned[1..]),
        Some(b'-') => (true, &cleaned[1..]),
        _ => (false, cleaned.as_str()),
    };
    let value = if let Some(hex) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
        parse_hex_float(hex)?
    } else {
        if !body
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'E' | b'+' | b'-'))
        {
            return None;
        }
        body.parse::<f64>().ok()?
    };
    if value.is_infinite() {
        return None;
    }
    Some(if negative { -value } else { value })
}

/// A hexadecimal float after `0x`: hex digits, an optional fraction, and a required `p`
/// exponent.
fn parse_hex_float(s: &str) -> Option<f64> {
    let (mantissa, exponent) = s.split_once(['p', 'P'])?;
    let exponent: i32 = exponent.parse().ok()?;
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    let mut value = 0.0_f64;
    for c in whole.chars().chain(fraction.chars()) {
        value = value * 16.0 + f64::from(c.to_digit(16)?);
    }
    let fraction_bits = i32::try_from(fraction.len()).ok()?.checked_mul(4)?;
    Some(value * 2_f64.powi(exponent.checked_sub(fraction_bits)?))
}

/// Go's `underscoreOK`: underscores only between digits, or after a base prefix.
fn underscore_ok(s: &str) -> bool {
    let mut s = s.as_bytes();
    if let Some(b'-' | b'+') = s.first() {
        s = &s[1..];
    }
    // '^' at the start, '0' after a digit or base prefix, '_' after an underscore, '!' else.
    let mut saw = b'^';
    let mut i = 0;
    let mut hex = false;
    if s.len() >= 2 && s[0] == b'0' && matches!(s[1].to_ascii_lowercase(), b'b' | b'o' | b'x') {
        i = 2;
        saw = b'0';
        hex = s[1].eq_ignore_ascii_case(&b'x');
    }
    while i < s.len() {
        let c = s[i];
        if c.is_ascii_digit() || hex && matches!(c.to_ascii_lowercase(), b'a'..=b'f') {
            saw = b'0';
        } else if c == b'_' {
            if saw != b'0' {
                return false;
            }
            saw = b'_';
        } else {
            if saw == b'_' {
                return false;
            }
            saw = b'!';
        }
        i += 1;
    }
    saw != b'_'
}

/// Go's `strconv.Unquote` for a double-quoted or backquoted literal. `None` is Go's
/// `invalid syntax`.
pub(crate) fn unquote(s: &str) -> Option<String> {
    if let Some(raw) = s.strip_prefix('`').and_then(|s| s.strip_suffix('`')) {
        if raw.contains('`') {
            return None;
        }
        return Some(raw.chars().filter(|c| *c != '\r').collect());
    }
    let mut rest = s.strip_prefix('"')?.strip_suffix('"')?;
    if rest.contains('\n') {
        return None;
    }
    let mut out = String::with_capacity(rest.len());
    while !rest.is_empty() {
        let (c, tail) = unquote_char(rest, '"')?;
        out.push(c);
        rest = tail;
    }
    Some(out)
}

/// Go's `strconv.UnquoteChar`: the first character of `s`, with escapes, and the rest.
///
/// Go turns `\x` and octal escapes into single bytes. A byte above `\x7f` is not valid UTF-8
/// on its own, so this returns U+FFFD for it.
pub(crate) fn unquote_char(s: &str, quote: char) -> Option<(char, &str)> {
    let mut chars = s.chars();
    let c = chars.next()?;
    if c == quote && (quote == '\'' || quote == '"') {
        return None;
    }
    if c != '\\' {
        return Some((c, chars.as_str()));
    }
    let escape = chars.next()?;
    let rest = chars.as_str();
    let simple = match escape {
        'a' => Some('\u{7}'),
        'b' => Some('\u{8}'),
        'f' => Some('\u{c}'),
        'n' => Some('\n'),
        'r' => Some('\r'),
        't' => Some('\t'),
        'v' => Some('\u{b}'),
        '\\' => Some('\\'),
        '\'' | '"' => {
            if escape != quote {
                return None;
            }
            Some(escape)
        }
        _ => None,
    };
    if let Some(c) = simple {
        return Some((c, rest));
    }
    match escape {
        'x' | 'u' | 'U' => {
            let n = match escape {
                'x' => 2,
                'u' => 4,
                _ => 8,
            };
            let digits = rest.get(..n)?;
            let value = u32::from_str_radix(digits, 16).ok()?;
            if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            let c = if escape == 'x' {
                byte_char(value)
            } else {
                char::from_u32(value)?
            };
            Some((c, &rest[n..]))
        }
        '0'..='7' => {
            let digits = s.get(1..4)?;
            if !digits.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
                return None;
            }
            let value = u32::from_str_radix(digits, 8).ok()?;
            if value > 255 {
                return None;
            }
            Some((byte_char(value), &s[4..]))
        }
        _ => None,
    }
}

/// The character for a byte from a `\x` or octal escape.
fn byte_char(value: u32) -> char {
    if value < 0x80 {
        char::from_u32(value).unwrap_or(char::REPLACEMENT_CHARACTER)
    } else {
        char::REPLACEMENT_CHARACTER
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_uint_reads_go_base_prefixes() {
        assert_eq!(parse_uint("0x1F"), Ok(31));
        assert_eq!(parse_uint("0o17"), Ok(15));
        assert_eq!(parse_uint("017"), Ok(15));
        assert_eq!(parse_uint("0b101"), Ok(5));
        assert_eq!(parse_uint("1_000"), Ok(1000));
        assert_eq!(parse_uint("08"), Err(IntError::Syntax));
        assert_eq!(parse_uint("1__0"), Err(IntError::Syntax));
        assert_eq!(parse_uint("18446744073709551616"), Err(IntError::Range));
    }

    #[test]
    fn parse_int_reads_signs() {
        assert_eq!(parse_int("-9223372036854775808"), Ok(i64::MIN));
        assert_eq!(parse_int("9223372036854775808"), Err(IntError::Range));
        assert_eq!(parse_int("+3"), Ok(3));
    }

    #[test]
    fn parse_float_reads_go_floats() {
        assert_eq!(parse_float("1e3"), Some(1000.0));
        assert_eq!(parse_float(".5"), Some(0.5));
        assert_eq!(parse_float("0x1p4"), Some(16.0));
        assert_eq!(parse_float("0x1.8p1"), Some(3.0));
        assert_eq!(parse_float("1e400"), None);
        assert_eq!(parse_float("0x1F"), None);
    }

    #[test]
    fn unquote_handles_go_escapes() {
        assert_eq!(
            unquote(r#""\t\x41\u00e9\U0001F600\101\\\"""#).as_deref(),
            Some("\tAé\u{1f600}A\\\"")
        );
        assert_eq!(unquote(r#""\q""#), None);
        assert_eq!(unquote("`a\\n`").as_deref(), Some("a\\n"));
        assert_eq!(unquote(r#""\'""#), None);
        assert_eq!(unquote_char(r"\'x", '\''), Some(('\'', "x")));
    }
}
