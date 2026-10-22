//! Tags of nodes: explicit tags and the YAML 1.2 core schema for plain
//! scalars (ADR 0005, `docs/differences.md#yaml-core-schema`).

use crate::Tag;

const YAML_TAG_PREFIX: &str = "tag:yaml.org,2002:";

/// Converts a tag as the parser expands it into the form messages print.
///
/// Returns `None` for no tag and for the non-specific tag `!`, which go-yaml
/// also treats as no tag.
pub(crate) fn explicit(tag: Option<&str>) -> Option<Tag> {
    let tag = tag.filter(|tag| !tag.is_empty() && *tag != "!")?;
    Some(match tag.strip_prefix(YAML_TAG_PREFIX) {
        Some("str") => Tag::Str,
        Some("int") => Tag::Int,
        Some("float") => Tag::Float,
        Some("bool") => Tag::Bool,
        Some("null") => Tag::Null,
        Some("map") => Tag::Map,
        Some("seq") => Tag::Seq,
        Some(name) => Tag::Other(Box::new(format!("!!{name}"))),
        None => Tag::Other(Box::new(tag.to_owned())),
    })
}

/// Resolves the tag of an untagged plain scalar from its value under the
/// YAML 1.2 core schema.
pub(crate) fn plain(value: &str) -> Tag {
    // Every null, bool, int, and float starts with one of these bytes, and
    // most strings do not, so they skip the checks below.
    if !matches!(
        value.as_bytes().first(),
        None | Some(
            b'0'..=b'9' | b'-' | b'+' | b'.' | b'~' | b'n' | b'N' | b't' | b'T' | b'f' | b'F'
        )
    ) {
        return Tag::Str;
    }
    match value {
        "" | "~" | "null" | "Null" | "NULL" => Tag::Null,
        "true" | "True" | "TRUE" | "false" | "False" | "FALSE" => Tag::Bool,
        ".nan" | ".NaN" | ".NAN" => Tag::Float,
        _ if is_int(value) => Tag::Int,
        _ if is_float(value) => Tag::Float,
        _ => Tag::Str,
    }
}

/// `[-+]?[0-9]+`, `0o[0-7]+`, or `0x[0-9a-fA-F]+`.
fn is_int(value: &str) -> bool {
    if let Some(octal) = value.strip_prefix("0o") {
        return !octal.is_empty() && octal.bytes().all(|b| matches!(b, b'0'..=b'7'));
    }
    if let Some(hex) = value.strip_prefix("0x") {
        return !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit());
    }
    let digits = value.strip_prefix(['-', '+']).unwrap_or(value);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

/// `[-+]?(\.[0-9]+|[0-9]+(\.[0-9]*)?)([eE][-+]?[0-9]+)?` or
/// `[-+]?\.(inf|Inf|INF)`.
fn is_float(value: &str) -> bool {
    let unsigned = value.strip_prefix(['-', '+']).unwrap_or(value);
    if matches!(unsigned, ".inf" | ".Inf" | ".INF") {
        return true;
    }
    let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
        Some(at) => (&unsigned[..at], Some(&unsigned[at + 1..])),
        None => (unsigned, None),
    };
    let (whole, fraction) = match mantissa.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (mantissa, None),
    };
    let all_digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    let mantissa_ok = match fraction {
        // `.5` needs digits after the point, `1.` and `1.5` need digits
        // before it.
        Some(fraction) if whole.is_empty() => !fraction.is_empty() && all_digits(fraction),
        Some(fraction) => all_digits(whole) && all_digits(fraction),
        None => !whole.is_empty() && all_digits(whole),
    };
    let exponent_ok = exponent.is_none_or(|exponent| {
        let digits = exponent.strip_prefix(['-', '+']).unwrap_or(exponent);
        !digits.is_empty() && all_digits(digits)
    });
    mantissa_ok && exponent_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_schema_null_bool_int_and_float() {
        let cases: &[(&str, Tag)] = &[
            ("", Tag::Null),
            ("~", Tag::Null),
            ("null", Tag::Null),
            ("NULL", Tag::Null),
            ("nULL", Tag::Str),
            ("true", Tag::Bool),
            ("False", Tag::Bool),
            ("tRUE", Tag::Str),
            ("yes", Tag::Str),
            ("on", Tag::Str),
            ("0", Tag::Int),
            ("-0", Tag::Int),
            ("+12", Tag::Int),
            ("017", Tag::Int),
            ("0o17", Tag::Int),
            ("0o8", Tag::Str),
            ("0o", Tag::Str),
            ("0x1F", Tag::Int),
            ("0x", Tag::Str),
            ("-0x1F", Tag::Str),
            ("0b10", Tag::Str),
            ("1_000", Tag::Str),
            ("1.5", Tag::Float),
            (".5", Tag::Float),
            ("1.", Tag::Float),
            ("-1e3", Tag::Float),
            ("1E+3", Tag::Float),
            ("1e", Tag::Str),
            ("e3", Tag::Str),
            (".", Tag::Str),
            ("-.", Tag::Str),
            ("+.inf", Tag::Float),
            ("-.Inf", Tag::Float),
            (".nan", Tag::Float),
            ("-.nan", Tag::Str),
            ("2026-10-15", Tag::Str),
            ("1:20", Tag::Str),
            ("<<", Tag::Str),
            ("1.2.3", Tag::Str),
        ];
        for (value, want) in cases {
            assert_eq!(plain(value), *want, "{value:?}");
        }
    }

    #[test]
    fn explicit_tags_are_shortened_like_go_yaml() {
        assert_eq!(explicit(None), None);
        assert_eq!(explicit(Some("!")), None);
        assert_eq!(explicit(Some("tag:yaml.org,2002:str")), Some(Tag::Str));
        assert_eq!(
            explicit(Some("tag:yaml.org,2002:binary")),
            Some(Tag::Other(Box::new("!!binary".to_owned())))
        );
        assert_eq!(
            explicit(Some("!custom")),
            Some(Tag::Other(Box::new("!custom".to_owned())))
        );
        assert_eq!(
            explicit(Some("tag:example.com,2000:foo")),
            Some(Tag::Other(Box::new("tag:example.com,2000:foo".to_owned())))
        );
    }
}
