//! What go-yaml's `resolve` makes of a scalar.

use workflint_yaml::{Scalar, Tag};

use super::Fatal;

/// The value that go-yaml resolves a scalar to before it stores it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Resolved {
    /// `nil`.
    Null,
    /// A `bool`.
    Bool(bool),
    /// An `int`, which go-yaml uses for integers that fit 64 bits.
    Int(i64),
    /// A `uint64`, for integers above `i64::MAX`.
    Uint(u64),
    /// A `float64`.
    Float(f64),
    /// A `string`: the decoded text, or the bytes of a `!!binary` scalar.
    Str(String),
}

/// Resolves `scalar` as go-yaml's decoder does before storing it.
///
/// The tag comes from the YAML tree, which types plain scalars by the YAML
/// 1.2 core schema (docs/differences.md#yaml-core-schema). A scalar with an
/// explicit `!!bool`, `!!int`, `!!float`, or `!!null` tag must have a value
/// of that type, or decoding ends with go-yaml's `cannot decode` error. A
/// `!!binary` scalar is decoded from base64.
pub(crate) fn resolve(scalar: &Scalar) -> Result<Resolved, Fatal> {
    let value = &*scalar.value;
    let tag = &scalar.tag;
    match tag {
        Tag::Str | Tag::Map | Tag::Seq => Ok(Resolved::Str(value.to_owned())),
        tag if is_binary(tag) => match base64(value) {
            Some(bytes) => Ok(Resolved::Str(String::from_utf8_lossy(&bytes).into_owned())),
            None => Err(Fatal::new(
                "yaml: !!binary value contains invalid base64 data".to_owned(),
            )),
        },
        Tag::Other(_) => Ok(Resolved::Str(value.to_owned())),
        Tag::Null | Tag::Bool | Tag::Int | Tag::Float => {
            let resolved = Tag::resolve_plain(value);
            let fits = resolved == *tag || (*tag == Tag::Float && resolved == Tag::Int);
            if !fits {
                return Err(Fatal::new(format!(
                    "yaml: cannot decode {} `{value}` as a {}",
                    resolved.as_str(),
                    tag.as_str()
                )));
            }
            Ok(match tag {
                Tag::Null => Resolved::Null,
                Tag::Bool => Resolved::Bool(matches!(value, "true" | "True" | "TRUE")),
                Tag::Int => int(value),
                _ => Resolved::Float(float(value)),
            })
        }
    }
}

/// Returns whether `tag` is `!!binary`, whose scalars hold base64.
pub(crate) fn is_binary(tag: &Tag) -> bool {
    matches!(tag, Tag::Other(name) if &**name == "!!binary")
}

/// Returns whether `value` is a YAML 1.1 form of a bool that go-yaml
/// accepts as text where a bool is expected, and which bool it is.
pub(crate) fn yaml_1_1_bool(value: &str) -> Option<bool> {
    match value {
        "y" | "Y" | "yes" | "Yes" | "YES" | "on" | "On" | "ON" => Some(true),
        "n" | "N" | "no" | "No" | "NO" | "off" | "Off" | "OFF" => Some(false),
        _ => None,
    }
}

/// Parses an integer of the core schema as Go's `strconv.ParseInt` with
/// base 0 does, falling back to `ParseUint` and then to a float, as
/// go-yaml's `resolve` does.
fn int(value: &str) -> Resolved {
    let (negative, digits) = match value.as_bytes().first() {
        Some(b'-') => (true, &value[1..]),
        Some(b'+') => (false, &value[1..]),
        _ => (false, value),
    };
    let (radix, digits) = if let Some(hex) = digits.strip_prefix("0x") {
        (16, hex)
    } else if let Some(octal) = digits.strip_prefix("0o") {
        (8, octal)
    } else if digits.len() > 1 && digits.starts_with('0') {
        (8, &digits[1..])
    } else {
        (10, digits)
    };
    match u64::from_str_radix(digits, radix) {
        Ok(magnitude) if negative => match i64::try_from(magnitude) {
            Ok(n) => Resolved::Int(-n),
            Err(_) if magnitude == i64::MIN.unsigned_abs() => Resolved::Int(i64::MIN),
            Err(_) => Resolved::Float(float(value)),
        },
        Ok(magnitude) => match i64::try_from(magnitude) {
            Ok(n) => Resolved::Int(n),
            // `ParseUint` rejects a sign.
            Err(_) if value.starts_with('+') => Resolved::Float(float(value)),
            Err(_) => Resolved::Uint(magnitude),
        },
        Err(_) if radix == 16 => Resolved::Str(value.to_owned()),
        Err(_) => Resolved::Float(float(value)),
    }
}

/// Parses a float of the core schema.
fn float(value: &str) -> f64 {
    let unsigned = value.trim_start_matches(['-', '+']);
    let negative = value.starts_with('-');
    let magnitude = match unsigned {
        ".inf" | ".Inf" | ".INF" => f64::INFINITY,
        ".nan" | ".NaN" | ".NAN" => return f64::NAN,
        _ => unsigned.parse().unwrap_or(f64::NAN),
    };
    if negative { -magnitude } else { magnitude }
}

/// Decodes standard base64 with padding as Go's
/// `base64.StdEncoding.DecodeString` does, which skips line breaks.
pub(crate) fn base64(text: &str) -> Option<Vec<u8>> {
    let symbols: Vec<u8> = text
        .bytes()
        .filter(|b| !matches!(b, b'\r' | b'\n'))
        .collect();
    let (quanta, rest) = symbols.as_chunks::<4>();
    if !rest.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(quanta.len() * 3);
    for (i, quantum) in quanta.iter().enumerate() {
        let padding = quantum.iter().rev().take_while(|&&b| b == b'=').count();
        let last = i + 1 == quanta.len();
        if padding > 2 || (padding > 0 && !last) {
            return None;
        }
        let mut bits = 0u32;
        for &symbol in &quantum[..4 - padding] {
            bits = (bits << 6) | u32::from(sextet(symbol)?);
        }
        bits <<= 6 * padding;
        let bytes = bits.to_be_bytes();
        out.extend_from_slice(&bytes[1..4 - padding]);
    }
    Some(out)
}

fn sextet(symbol: u8) -> Option<u8> {
    match symbol {
        b'A'..=b'Z' => Some(symbol - b'A'),
        b'a'..=b'z' => Some(symbol - b'a' + 26),
        b'0'..=b'9' => Some(symbol - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_decodes_like_go() {
        assert_eq!(base64("aGVsbG8=").as_deref(), Some(&b"hello"[..]));
        assert_eq!(base64("aGVs\nbG8=").as_deref(), Some(&b"hello"[..]));
        assert_eq!(base64("aGk=").as_deref(), Some(&b"hi"[..]));
        assert_eq!(base64("aA==").as_deref(), Some(&b"h"[..]));
        assert_eq!(base64("").as_deref(), Some(&b""[..]));
        assert_eq!(base64("aGVsbG8"), None);
        assert_eq!(base64("aA==aA=="), None);
        assert_eq!(base64("a==="), None);
        assert_eq!(base64("a*=="), None);
    }

    #[test]
    fn integers_are_parsed_like_strconv_with_base_0() {
        assert_eq!(int("12"), Resolved::Int(12));
        assert_eq!(int("-12"), Resolved::Int(-12));
        assert_eq!(int("+12"), Resolved::Int(12));
        assert_eq!(int("017"), Resolved::Int(15));
        assert_eq!(int("0o17"), Resolved::Int(15));
        assert_eq!(int("0x1F"), Resolved::Int(31));
        assert_eq!(int("09"), Resolved::Float(9.0));
        assert_eq!(int("-9223372036854775808"), Resolved::Int(i64::MIN));
        assert_eq!(int("18446744073709551615"), Resolved::Uint(u64::MAX));
        assert_eq!(
            int("18446744073709551616"),
            Resolved::Float(18_446_744_073_709_551_616.0)
        );
    }
}
