//! Comparing, matching, and printing matrix values, the reference's `Equals`, `isYAMLValueSubset`,
//! and `String` of `RawYAMLValue`.
//!
//! A value can nest as deep as the YAML layer allows (ADR 0022), so each function loops over a
//! stack instead of recursing.

use workflint_diagnostics::quote::quote;
use workflint_syntax::{RawValue, contains_expression};

/// Returns whether `a` and `b` are the same value, the reference's `Equals`.
///
/// Scalars compare by their text, whatever their tag or quotes. Mappings compare by their folded
/// keys, in any order. Spans do not count, unlike in `RawValue`'s `PartialEq`.
pub(super) fn equals(a: &RawValue<'_>, b: &RawValue<'_>) -> bool {
    // Most matrix values are scalars, and the duplicate check compares each pair of them.
    if let (RawValue::Scalar(a), RawValue::Scalar(b)) = (a, b) {
        return a.value() == b.value();
    }
    let mut pending = vec![(a, b)];
    while let Some(pair) = pending.pop() {
        match pair {
            (RawValue::Scalar(a), RawValue::Scalar(b)) => {
                if a.value() != b.value() {
                    return false;
                }
            }
            (RawValue::Sequence { items: a, .. }, RawValue::Sequence { items: b, .. }) => {
                if a.len() != b.len() {
                    return false;
                }
                pending.extend(a.iter().zip(b.iter()));
            }
            (RawValue::Mapping { entries: a, .. }, RawValue::Mapping { entries: b, .. }) => {
                if a.len() != b.len() {
                    return false;
                }
                for entry in a {
                    let Some(other) = b.get(entry.folded()) else {
                        return false;
                    };
                    pending.push((&entry.value, &other.value));
                }
            }
            _ => return false,
        }
    }
    true
}

/// Returns whether the `exclude` value `filter` matches the row value `value`, the reference's
/// `isYAMLValueSubset`.
///
/// A mapping matches a mapping that has each of its properties with a matching value (#249). A
/// sequence matches a sequence of the same length item by item. A scalar with a `${{ }}`
/// expression matches anything on either side (#261, #414), and any other scalar matches a scalar
/// with the same text.
pub(super) fn matches(value: &RawValue<'_>, filter: &RawValue<'_>) -> bool {
    let mut pending = vec![(value, filter)];
    while let Some((value, filter)) = pending.pop() {
        if let RawValue::Scalar(filter) = filter
            && contains_expression(filter.value())
        {
            continue;
        }
        match (value, filter) {
            (
                RawValue::Mapping { entries, .. },
                RawValue::Mapping {
                    entries: filter, ..
                },
            ) => {
                for entry in filter {
                    let Some(property) = entries.get(entry.folded()) else {
                        return false;
                    };
                    pending.push((&property.value, &entry.value));
                }
            }
            (RawValue::Sequence { items, .. }, RawValue::Sequence { items: filter, .. }) => {
                if items.len() != filter.len() {
                    return false;
                }
                pending.extend(items.iter().zip(filter.iter()));
            }
            (RawValue::Scalar(value), filter) => {
                if contains_expression(value.value()) {
                    continue;
                }
                let RawValue::Scalar(filter) = filter else {
                    return false;
                };
                if value.value() != filter.value() {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

/// A step of [`display`].
enum Part<'v, 'a> {
    /// A value to print at the end of the current buffer.
    Value(&'v RawValue<'a>),
    /// Text to add at the end of the current buffer.
    Text(&'static str),
    /// Starts the buffer of a mapping entry with its folded key.
    Entry(&'v str),
    /// Joins the last `len` buffers, one per entry, into a mapping.
    Mapping { len: usize },
}

/// Prints `value` as the reference's `String` does: scalars as Go quoted strings, sequences as
/// `[a, b]`, and mappings as `{"key": value}` with folded keys, sorted by the printed entry.
pub(super) fn display(value: &RawValue<'_>) -> String {
    let mut parts = vec![Part::Value(value)];
    // The text of the value, and one buffer for each mapping entry being printed, which is
    // sorted with its siblings once they are all printed.
    let mut buffers = vec![String::new()];
    while let Some(part) = parts.pop() {
        match part {
            Part::Value(RawValue::Scalar(scalar)) => push(&mut buffers, &quote(scalar.value())),
            Part::Value(RawValue::Sequence { items, .. }) => {
                push(&mut buffers, "[");
                parts.push(Part::Text("]"));
                for (index, item) in items.iter().enumerate().rev() {
                    parts.push(Part::Value(item));
                    if index > 0 {
                        parts.push(Part::Text(", "));
                    }
                }
            }
            Part::Value(RawValue::Mapping { entries, .. }) => {
                parts.push(Part::Mapping { len: entries.len() });
                for entry in entries.iter().rev() {
                    parts.push(Part::Value(&entry.value));
                    parts.push(Part::Entry(entry.folded()));
                }
            }
            Part::Text(text) => push(&mut buffers, text),
            Part::Entry(key) => buffers.push(format!("{}: ", quote(key))),
            Part::Mapping { len } => {
                let mut entries = buffers.split_off(buffers.len().saturating_sub(len));
                entries.sort();
                let mapping = format!("{{{}}}", entries.join(", "));
                push(&mut buffers, &mapping);
            }
        }
    }
    buffers.into_iter().next().unwrap_or_default()
}

/// Adds `text` at the end of the last buffer.
fn push(buffers: &mut [String], text: &str) {
    if let Some(buffer) = buffers.last_mut() {
        buffer.push_str(text);
    }
}
