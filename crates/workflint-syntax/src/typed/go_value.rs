//! The Go values that go-yaml decodes into `interface{}`, as far as map
//! keys and messages need them.

use std::cmp::Ordering;
use std::fmt::Write as _;

use workflint_diagnostics::quote::quote;

use super::scalar::Resolved;

/// A value that go-yaml stores in an `interface{}`.
pub(crate) enum GoValue {
    /// A scalar.
    Scalar(Resolved),
    /// A `[]interface{}`.
    Slice(Vec<GoValue>),
    /// A `map[string]interface{}` if `string_keys`, else a
    /// `map[interface{}]interface{}`. Each key appears once.
    Map {
        string_keys: bool,
        entries: Vec<(GoValue, GoValue)>,
    },
}

/// A hashable Go value, which can be a map key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Key {
    Nil,
    Bool(bool),
    Int(i64),
    Uint(u64),
    /// The bits of a float that is not NaN, with -0 stored as 0, because Go
    /// compares floats with `==`.
    Float(u64),
    Str(String),
}

/// What [`GoValue::key`] returns for a value that is not a usable key.
pub(crate) enum KeyError {
    /// NaN, which equals no value, so it never matches a key.
    NaN,
    /// A slice or map, which Go cannot hash. The text is its type.
    Unhashable(&'static str),
}

impl GoValue {
    /// Returns the value as a map key.
    pub(crate) fn key(&self) -> Result<Key, KeyError> {
        match self {
            Self::Scalar(Resolved::Null) => Ok(Key::Nil),
            Self::Scalar(Resolved::Bool(b)) => Ok(Key::Bool(*b)),
            Self::Scalar(Resolved::Int(n)) => Ok(Key::Int(*n)),
            Self::Scalar(Resolved::Uint(n)) => Ok(Key::Uint(*n)),
            Self::Scalar(Resolved::Float(f)) if f.is_nan() => Err(KeyError::NaN),
            Self::Scalar(Resolved::Float(f)) => Ok(Key::Float((f + 0.0).to_bits())),
            Self::Scalar(Resolved::Str(s)) => Ok(Key::Str(s.clone())),
            Self::Slice(_) | Self::Map { .. } => Err(KeyError::Unhashable(self.type_name())),
        }
    }

    /// Returns whether the value is a slice or a map, which go-yaml does
    /// not accept as a map key.
    pub(crate) fn is_collection(&self) -> bool {
        matches!(self, Self::Slice(_) | Self::Map { .. })
    }

    fn type_name(&self) -> &'static str {
        match self {
            Self::Slice(_) => "[]interface {}",
            Self::Map {
                string_keys: true, ..
            } => "map[string]interface {}",
            Self::Map { .. } => "map[interface {}]interface {}",
            Self::Scalar(_) => "interface {}",
        }
    }

    /// Formats the value as Go's `%#v` does.
    pub(crate) fn go_syntax(&self) -> String {
        /// A part of the output still to write.
        enum Part<'v> {
            /// A value. The flag is true for an element of a container,
            /// whose static type is `interface{}`, so `nil` prints as
            /// `interface {}(nil)`.
            Value(&'v GoValue, bool),
            Text(&'static str),
        }
        let mut out = String::new();
        // A stack instead of recursion, so that a value nested as deep as
        // the YAML layer allows (ADR 0022) cannot overflow the stack.
        let mut parts = vec![Part::Value(self, false)];
        while let Some(part) = parts.pop() {
            let (value, in_interface) = match part {
                Part::Text(text) => {
                    out.push_str(text);
                    continue;
                }
                Part::Value(value, in_interface) => (value, in_interface),
            };
            match value {
                Self::Scalar(Resolved::Null) if in_interface => out.push_str("interface {}(nil)"),
                Self::Scalar(Resolved::Null) => out.push_str("<nil>"),
                Self::Scalar(Resolved::Bool(b)) => {
                    let _ = write!(out, "{b}");
                }
                Self::Scalar(Resolved::Int(n)) => {
                    let _ = write!(out, "{n}");
                }
                Self::Scalar(Resolved::Uint(n)) => {
                    let _ = write!(out, "{n:#x}");
                }
                Self::Scalar(Resolved::Float(f)) => out.push_str(&format_float(*f)),
                Self::Scalar(Resolved::Str(s)) => out.push_str(&quote(s)),
                Self::Slice(items) => {
                    out.push_str("[]interface {}{");
                    parts.push(Part::Text("}"));
                    for (i, item) in items.iter().enumerate().rev() {
                        parts.push(Part::Value(item, true));
                        if i > 0 {
                            parts.push(Part::Text(", "));
                        }
                    }
                }
                Self::Map { entries, .. } => {
                    out.push_str(value.type_name());
                    out.push('{');
                    let mut sorted: Vec<&(GoValue, GoValue)> = entries.iter().collect();
                    sorted.sort_by(|(a, _), (b, _)| compare_keys(a, b));
                    parts.push(Part::Text("}"));
                    for (i, (key, value)) in sorted.into_iter().enumerate().rev() {
                        parts.push(Part::Value(value, true));
                        parts.push(Part::Text(":"));
                        parts.push(Part::Value(key, true));
                        if i > 0 {
                            parts.push(Part::Text(", "));
                        }
                    }
                }
            }
        }
        out
    }

    /// Moves the items, or the keys and values, of the value to `stack`.
    fn take_children(&mut self, stack: &mut Vec<GoValue>) {
        match self {
            Self::Scalar(_) => {}
            Self::Slice(items) => stack.append(items),
            Self::Map { entries, .. } => {
                for (key, value) in entries.drain(..) {
                    stack.push(key);
                    stack.push(value);
                }
            }
        }
    }
}

/// Drops the nested values with a loop, so that a value nested as deep as
/// the YAML layer allows (ADR 0022) cannot overflow the stack.
impl Drop for GoValue {
    fn drop(&mut self) {
        let mut stack = Vec::new();
        self.take_children(&mut stack);
        while let Some(mut value) = stack.pop() {
            value.take_children(&mut stack);
        }
    }
}

/// Orders map keys as Go's `fmt` sorts them for printing: by value within
/// a type. Go orders keys of different types by the address of their type,
/// which is not stable, so this uses a fixed order of types instead.
fn compare_keys(a: &GoValue, b: &GoValue) -> Ordering {
    fn rank(value: &GoValue) -> u8 {
        match value {
            GoValue::Scalar(Resolved::Null) => 0,
            GoValue::Scalar(Resolved::Bool(_)) => 1,
            GoValue::Scalar(Resolved::Int(_)) => 2,
            GoValue::Scalar(Resolved::Uint(_)) => 3,
            GoValue::Scalar(Resolved::Float(_)) => 4,
            GoValue::Scalar(Resolved::Str(_)) => 5,
            GoValue::Slice(_) | GoValue::Map { .. } => 6,
        }
    }
    match (a, b) {
        (GoValue::Scalar(Resolved::Bool(a)), GoValue::Scalar(Resolved::Bool(b))) => a.cmp(b),
        (GoValue::Scalar(Resolved::Int(a)), GoValue::Scalar(Resolved::Int(b))) => a.cmp(b),
        (GoValue::Scalar(Resolved::Uint(a)), GoValue::Scalar(Resolved::Uint(b))) => a.cmp(b),
        // Go sorts NaN before every other float.
        (GoValue::Scalar(Resolved::Float(a)), GoValue::Scalar(Resolved::Float(b))) => {
            match (a.is_nan(), b.is_nan()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                (false, false) => a.total_cmp(b),
            }
        }
        (GoValue::Scalar(Resolved::Str(a)), GoValue::Scalar(Resolved::Str(b))) => a.cmp(b),
        _ => rank(a).cmp(&rank(b)),
    }
}

/// Formats a float as Go's `%v` does, which is
/// `strconv.FormatFloat(f, 'g', -1, 64)`: the shortest digits that
/// round-trip, in exponent form when the exponent is below -4 or at least 6.
fn format_float(f: f64) -> String {
    if f.is_nan() {
        return "NaN".to_owned();
    }
    if f.is_infinite() {
        return if f > 0.0 { "+Inf" } else { "-Inf" }.to_owned();
    }
    let sign = if f.is_sign_negative() { "-" } else { "" };
    // `{:e}` gives the shortest round-trip digits, such as `1.25e3`.
    let scientific = format!("{:e}", f.abs());
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let exponent: i64 = exponent.parse().unwrap_or(0);
    if !(-4..6).contains(&exponent) {
        let (first, rest) = digits.split_at(1.min(digits.len()));
        let point = if rest.is_empty() { "" } else { "." };
        let exponent_sign = if exponent < 0 { '-' } else { '+' };
        return format!(
            "{sign}{first}{point}{rest}e{exponent_sign}{:02}",
            exponent.unsigned_abs()
        );
    }
    let len = i64::try_from(digits.len()).unwrap_or(i64::MAX);
    if exponent < 0 {
        let zeros = usize::try_from(-exponent - 1).unwrap_or(0);
        return format!("{sign}0.{}{digits}", "0".repeat(zeros));
    }
    let whole = exponent + 1;
    if whole >= len {
        let zeros = usize::try_from(whole - len).unwrap_or(0);
        format!("{sign}{digits}{}", "0".repeat(zeros))
    } else {
        let at = usize::try_from(whole).unwrap_or(0);
        format!("{sign}{}.{}", &digits[..at], &digits[at..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_print_like_go() {
        let cases = [
            (1.0, "1"),
            (2.5, "2.5"),
            (1e21, "1e+21"),
            (1e20, "1e+20"),
            (123_456.0, "123456"),
            (1e-5, "1e-05"),
            (0.0001, "0.0001"),
            (f64::INFINITY, "+Inf"),
            (f64::NEG_INFINITY, "-Inf"),
            (f64::NAN, "NaN"),
            (-0.0, "-0"),
            (123_456_789.0, "1.23456789e+08"),
            (1e6, "1e+06"),
            (-1.5e-7, "-1.5e-07"),
        ];
        for (f, want) in cases {
            assert_eq!(format_float(f), want, "{f}");
        }
    }
}
