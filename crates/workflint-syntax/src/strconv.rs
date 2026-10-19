//! Go's `strconv.Atoi` and `strconv.ParseFloat`, with their error texts,
//! and the text that Go's `%v` prints for a float.
//!
//! The reference embeds the error of these functions in its messages, as in
//! `invalid integer value: "1.5": strconv.Atoi: parsing "1.5": invalid
//! syntax`, and prints numbers with `%v`, as in `must be greater than zero:
//! -1e+06`. So the parser needs the same results and the same texts. The
//! functions are public because the `events` rule of `workflint-rules` puts
//! their results in messages too. The tests compare the functions with a
//! table that Go 1.26 printed, in `tests/data/strconv.tsv`.

use workflint_diagnostics::quote::quote;

/// Parses `s` as Go's `strconv.Atoi` does on a 64-bit platform: an
/// optional sign and decimal digits, with no base prefix and no
/// underscores.
///
/// The error is Go's error text, such as `strconv.Atoi: parsing "1.5":
/// invalid syntax`.
///
/// # Errors
///
/// Returns Go's error text if `s` is not an integer in Go's syntax or is
/// out of range.
///
/// ```
/// use workflint_syntax::strconv::atoi;
///
/// assert_eq!(atoi("+5"), Ok(5));
/// assert_eq!(
///     atoi("x").unwrap_err(),
///     r#"strconv.Atoi: parsing "x": invalid syntax"#
/// );
/// ```
pub fn atoi(s: &str) -> Result<i64, String> {
    use std::num::IntErrorKind;

    // Rust's grammar for `i64` is Go's for base 10, and both stop at the
    // first digit that overflows, so `99999999999999999999x` is out of
    // range in both.
    s.parse::<i64>().map_err(|err| {
        let reason = match err.kind() {
            IntErrorKind::PosOverflow | IntErrorKind::NegOverflow => "value out of range",
            _ => "invalid syntax",
        };
        format!("strconv.Atoi: parsing {}: {reason}", quote(s))
    })
}

/// Formats `value` as Go's `fmt` package prints a `float64` with `%v`.
///
/// The digits are the fewest that read back as `value`. The exponent form,
/// such as `-1.5e+06` or `1e-05`, is used when the decimal exponent is less
/// than -4 or at least 6. Otherwise the number has no exponent, such as
/// `-3.5` or `123456.7`. Infinities are `+Inf` and `-Inf`. `%g` prints the
/// same text.
///
/// ```
/// use workflint_syntax::strconv::format_float;
///
/// assert_eq!(format_float(240.0), "240");
/// assert_eq!(format_float(-62135596800.0), "-6.21355968e+10");
/// ```
#[must_use]
pub fn format_float(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value > 0.0 { "+Inf" } else { "-Inf" }.to_owned();
    }
    // Rust's `{:e}` and `{}` both print the shortest digits that read back
    // as the value, as Go does.
    let scientific = format!("{:e}", value.abs());
    let Some((mantissa, exponent)) = scientific.split_once('e') else {
        return value.to_string();
    };
    let Ok(exponent) = exponent.parse::<i32>() else {
        return value.to_string();
    };
    if (-4..6).contains(&exponent) {
        return value.to_string();
    }
    let sign = if value.is_sign_negative() { "-" } else { "" };
    let exponent_sign = if exponent < 0 { '-' } else { '+' };
    format!(
        "{sign}{mantissa}e{exponent_sign}{:02}",
        exponent.unsigned_abs()
    )
}

/// Parses `s` as Go's `strconv.ParseFloat(s, 64)` does.
///
/// Go accepts decimal and hexadecimal floats, `_` between digits, and the
/// words `Inf`, `Infinity`, and `NaN` in any case. A value too large for
/// `f64` is out of range. The error is Go's error text, such as
/// `strconv.ParseFloat: parsing "1e309": value out of range`.
///
/// # Errors
///
/// Returns Go's error text if `s` is not a float in Go's syntax or is out
/// of range.
///
/// ```
/// use workflint_syntax::strconv::parse_float;
///
/// assert_eq!(parse_float("1_000"), Ok(1000.0));
/// assert_eq!(
///     parse_float(":8080").unwrap_err(),
///     r#"strconv.ParseFloat: parsing ":8080": invalid syntax"#
/// );
/// ```
pub fn parse_float(s: &str) -> Result<f64, String> {
    let error = |reason: &str| format!("strconv.ParseFloat: parsing {}: {reason}", quote(s));
    if let Some(value) = special(s) {
        return Ok(value);
    }
    let Some(float) = read_float(s.as_bytes()) else {
        return Err(error("invalid syntax"));
    };
    let value = if float.hex {
        hex_to_f64(&float)
    } else {
        // `read_float` accepted Go's decimal grammar. Without the
        // underscores it is also Rust's, and both round to nearest even.
        let digits: String = s.chars().filter(|&ch| ch != '_').collect();
        digits.parse::<f64>().ok()
    };
    match value {
        Some(value) if value.is_infinite() => Err(error("value out of range")),
        Some(value) => Ok(value),
        None => Err(error("invalid syntax")),
    }
}

/// Go's `special`: `[+-]inf`, `[+-]infinity`, or `nan`, ignoring case, as
/// the whole of `s`.
fn special(s: &str) -> Option<f64> {
    let lower = s.to_ascii_lowercase();
    match lower.as_str() {
        "inf" | "+inf" | "infinity" | "+infinity" => Some(f64::INFINITY),
        "-inf" | "-infinity" => Some(f64::NEG_INFINITY),
        "nan" => Some(f64::NAN),
        _ => None,
    }
}

/// What Go's `readFloat` reads from a float.
struct Float {
    /// The first 16 hexadecimal digits of a hexadecimal mantissa.
    mantissa: u64,
    /// The binary exponent of a hexadecimal float.
    exp: i64,
    neg: bool,
    /// Whether a non-zero digit after the first 16 was dropped.
    trunc: bool,
    hex: bool,
}

/// Go's `readFloat`, for all of `s`. Returns `None` if `s` is not a float
/// in Go's syntax.
fn read_float(s: &[u8]) -> Option<Float> {
    let mut i = 0;
    let mut neg = false;
    match s.first() {
        Some(b'+') => i += 1,
        Some(b'-') => {
            i += 1;
            neg = true;
        }
        _ => {}
    }
    let mut hex = false;
    if s.len() > i + 2 && s[i] == b'0' && s[i + 1].eq_ignore_ascii_case(&b'x') {
        hex = true;
        i += 2;
    }
    let (base, max_digits) = if hex { (16, 16) } else { (10, 19) };
    let mut saw_dot = false;
    let mut saw_digits = false;
    let mut digits = 0i64;
    let mut mantissa_digits = 0i64;
    let mut dot = 0i64;
    let mut mantissa = 0u64;
    let mut trunc = false;
    while let Some(&c) = s.get(i) {
        let digit = match c {
            b'_' => {
                i += 1;
                continue;
            }
            b'.' if saw_dot => break,
            b'.' => {
                saw_dot = true;
                dot = digits;
                i += 1;
                continue;
            }
            b'0'..=b'9' => u64::from(c - b'0'),
            b'a'..=b'f' | b'A'..=b'F' if hex => u64::from(c.to_ascii_lowercase() - b'a' + 10),
            _ => break,
        };
        saw_digits = true;
        i += 1;
        if digit == 0 && digits == 0 {
            // A leading zero moves the point but is not a digit.
            dot -= 1;
            continue;
        }
        digits += 1;
        if mantissa_digits < max_digits {
            mantissa = mantissa * base + digit;
            mantissa_digits += 1;
        } else if digit != 0 {
            trunc = true;
        }
    }
    if !saw_digits {
        return None;
    }
    if !saw_dot {
        dot = digits;
    }
    if hex {
        dot *= 4;
        mantissa_digits *= 4;
    }
    let exp_char = if hex { b'p' } else { b'e' };
    match s.get(i) {
        Some(c) if c.eq_ignore_ascii_case(&exp_char) => dot += read_exponent(s.get(i + 1..)?)?,
        // A hexadecimal float needs a `p` exponent.
        _ if hex => return None,
        Some(_) => return None,
        None => {}
    }
    // Go checks the underscores only if it read one, and any `_` that it
    // does not read makes the parse fail before that.
    if s.contains(&b'_') && !underscore_ok(s) {
        return None;
    }
    let exp = if mantissa == 0 {
        0
    } else {
        dot - mantissa_digits
    };
    Some(Float {
        mantissa,
        exp,
        neg,
        trunc,
        hex,
    })
}

/// Reads the exponent after the `e` or `p`, `[+-]?[0-9][0-9_]*`, as all of
/// `rest`. Like Go, it stops adding digits once the exponent reaches 10,000,
/// which is out of range anyway.
fn read_exponent(rest: &[u8]) -> Option<i64> {
    let (sign, digits) = match rest.split_first() {
        Some((b'-', digits)) => (-1, digits),
        Some((b'+', digits)) => (1, digits),
        _ => (1, rest),
    };
    if !digits.first().is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let mut exp = 0i64;
    for &c in digits {
        match c {
            b'0'..=b'9' if exp < 10_000 => exp = exp * 10 + i64::from(c - b'0'),
            b'0'..=b'9' | b'_' => {}
            _ => return None,
        }
    }
    Some(sign * exp)
}

/// Go's `underscoreOK`: each `_` comes after a digit or the base prefix and
/// before a digit.
fn underscore_ok(s: &[u8]) -> bool {
    let s = s
        .strip_prefix(b"+")
        .or_else(|| s.strip_prefix(b"-"))
        .unwrap_or(s);
    // `^` at the start, `0` after a digit or the prefix, `_` after an
    // underscore, and `!` after anything else.
    let mut saw = b'^';
    let mut i = 0;
    let mut hex = false;
    if s.len() >= 2 && s[0] == b'0' && matches!(s[1].to_ascii_lowercase(), b'b' | b'o' | b'x') {
        i = 2;
        saw = b'0';
        hex = s[1].eq_ignore_ascii_case(&b'x');
    }
    for &c in s.get(i..).unwrap_or_default() {
        if c.is_ascii_digit() || hex && c.is_ascii_hexdigit() {
            saw = b'0';
        } else if c == b'_' {
            if saw != b'0' {
                return false;
            }
            saw = b'_';
        } else if saw == b'_' {
            return false;
        } else {
            saw = b'!';
        }
    }
    saw != b'_'
}

/// Go's `atofHex` for `f64`: rounds `mantissa * 2^exp` to the nearest
/// `f64`, ties to even. Returns infinity when the value is out of range.
fn hex_to_f64(float: &Float) -> Option<f64> {
    const MANT_BITS: u32 = 52;
    const EXP_BITS: u32 = 11;
    const BIAS: i64 = -1023;
    const MAX_EXP: i64 = (1 << EXP_BITS) + BIAS - 2;
    const MIN_EXP: i64 = BIAS + 1;

    let mut mantissa = float.mantissa;
    let mut exp = float.exp + i64::from(MANT_BITS);
    while mantissa != 0 && mantissa >> (MANT_BITS + 2) == 0 {
        mantissa <<= 1;
        exp -= 1;
    }
    if float.trunc {
        mantissa |= 1;
    }
    while mantissa >> (1 + MANT_BITS + 2) != 0 {
        mantissa = (mantissa >> 1) | (mantissa & 1);
        exp += 1;
    }
    // Denormalize a value too small for a normal float.
    while mantissa > 1 && exp < MIN_EXP - 2 {
        mantissa = (mantissa >> 1) | (mantissa & 1);
        exp += 1;
    }
    let mut round = mantissa & 3;
    mantissa >>= 2;
    round |= mantissa & 1;
    exp += 2;
    if round == 3 {
        mantissa += 1;
        if mantissa == 1 << (1 + MANT_BITS) {
            mantissa >>= 1;
            exp += 1;
        }
    }
    if mantissa >> MANT_BITS == 0 {
        exp = BIAS;
    }
    if exp > MAX_EXP {
        return Some(if float.neg {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        });
    }
    let biased = u64::try_from(exp - BIAS).ok()?;
    let mut bits =
        (mantissa & ((1 << MANT_BITS) - 1)) | (biased & ((1 << EXP_BITS) - 1)) << MANT_BITS;
    if float.neg {
        bits |= 1 << (MANT_BITS + EXP_BITS);
    }
    Some(f64::from_bits(bits))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;

    /// The results that Go printed with `tests/data/strconv.go`.
    const TABLE: &str = include_str!("../tests/data/strconv.tsv");

    fn cases(function: &str) -> impl Iterator<Item = (&'static str, &'static str, &'static str)> {
        TABLE.lines().filter_map(move |line| {
            let fields: Vec<_> = line.split('\t').collect();
            let [name, input, status, result] = fields[..] else {
                panic!("bad line {line:?}");
            };
            (name == function).then_some((input, status, result))
        })
    }

    #[test]
    fn atoi_matches_go() {
        let mut count = 0;
        for (input, status, result) in cases("Atoi") {
            let have = match atoi(input) {
                Ok(value) => ("ok", value.to_string()),
                Err(err) => ("err", err),
            };
            assert_eq!(have, (status, result.to_owned()), "{input:?}");
            count += 1;
        }
        assert!(count > 30);
    }

    #[test]
    fn parse_float_matches_go() {
        let mut count = 0;
        for (input, status, result) in cases("ParseFloat") {
            let have = match parse_float(input) {
                // Go's NaN has the bits 7ff8000000000001.
                Ok(value) if value.is_nan() => ("ok", "7ff8000000000001".to_owned()),
                Ok(value) => ("ok", format!("{:016x}", value.to_bits())),
                Err(err) => ("err", err),
            };
            assert_eq!(have, (status, result.to_owned()), "{input:?}");
            count += 1;
        }
        assert!(count > 100);
    }

    #[test]
    fn format_float_matches_go() {
        let mut count = 0;
        for (input, _, result) in cases("FormatV") {
            let value = parse_float(input).unwrap();
            assert_eq!(format_float(value), result, "{input:?}");
            count += 1;
        }
        assert!(count > 80);
        assert_eq!(format_float(f64::NAN), "NaN");
    }
}
