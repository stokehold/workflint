//! `strategy.matrix` and the YAML values it holds.

use std::fmt;
use std::mem;
use std::sync::Arc;

use workflint_diagnostics::Span;

use crate::{Expr, Keyed, KeyedEntry, Template, Value};

/// The mapping form of `strategy.matrix`.
///
/// A whole `matrix: ${{ }}` is [`Value::Expression`] on
/// [`Strategy::matrix`](crate::Strategy::matrix).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Matrix<'a> {
    /// The rows, every key except `include` and `exclude`, keyed by name.
    pub rows: Keyed<'a, MatrixRow<'a>>,
    /// `include`. A whole `include: ${{ }}` is [`Value::Expression`], the
    /// reference's `MatrixCombinations.Expression`.
    pub include: Value<'a, Arc<[MatrixCombination<'a>]>>,
    /// `exclude`, as `include`.
    pub exclude: Value<'a, Arc<[MatrixCombination<'a>]>>,
}

/// The value of one matrix row.
///
/// The row's name is the key of its entry in [`Matrix::rows`]. The
/// reference's `MatrixRow.Name` is `nil` for an expression row. Here the
/// key is always there.
#[derive(Debug, Clone, PartialEq)]
pub enum MatrixRow<'a> {
    /// A sequence of values, in source order.
    Values(Arc<[RawValue<'a>]>),
    /// A whole `${{ }}` expression.
    Expression(Expr<'a>),
    /// Neither a sequence nor an expression. The parser has reported it,
    /// at the span of the row's entry.
    Invalid,
}

/// One item of `include` or `exclude`.
#[derive(Debug, Clone, PartialEq)]
pub enum MatrixCombination<'a> {
    /// A mapping of row names to values.
    Assigns {
        /// The span of the mapping.
        span: Span,
        /// The values keyed by row name. The reference's `MatrixAssign.Key`
        /// is the entry's key.
        assigns: Keyed<'a, RawValue<'a>>,
    },
    /// A whole `${{ }}` expression.
    Expression(Expr<'a>),
    /// Neither a mapping nor an expression. The parser has reported it.
    Invalid(Span),
}

/// A YAML value in a matrix, the reference's `RawYAMLValue`.
///
/// Scalars keep their tag and style, so rules can tell `1` from `"1"`
/// (`docs/differences.md#quoted-scalar-types`). The reference turns every
/// scalar into a `RawYAMLString`.
///
/// Comparing, printing with `{:?}`, and dropping a value loop over a stack
/// instead of recursing, so that a value nested as deep as the YAML layer
/// allows (ADR 0022) cannot overflow the stack.
#[derive(Clone)]
pub enum RawValue<'a> {
    /// A scalar, the reference's `RawYAMLString`.
    Scalar(Template<'a>),
    /// A sequence, the reference's `RawYAMLArray`.
    Sequence {
        /// The span of the sequence.
        span: Span,
        /// The items, in source order.
        items: Arc<[RawValue<'a>]>,
    },
    /// A mapping, the reference's `RawYAMLObject`.
    Mapping {
        /// The span of the mapping.
        span: Span,
        /// The values keyed by name, case-insensitively as in the
        /// reference.
        entries: Keyed<'a, RawValue<'a>>,
    },
}

impl<'a> RawValue<'a> {
    /// Returns the span of the value, the reference's `Pos()`.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Scalar(scalar) => scalar.span(),
            Self::Sequence { span, .. } | Self::Mapping { span, .. } => *span,
        }
    }

    /// Moves the items or entries of the value, if it has any, to `stack`.
    fn take_children(&mut self, stack: &mut Vec<Children<'a>>) {
        match self {
            Self::Scalar(_) => {}
            Self::Sequence { items, .. } => {
                if !items.is_empty() {
                    stack.push(Children::Items(mem::replace(items, Arc::new([]))));
                }
            }
            Self::Mapping { entries, .. } => {
                if !entries.is_empty() {
                    stack.push(Children::Entries(mem::take(entries)));
                }
            }
        }
    }
}

impl PartialEq for RawValue<'_> {
    fn eq(&self, other: &Self) -> bool {
        let mut pending = vec![(self, other)];
        while let Some(pair) = pending.pop() {
            match pair {
                (Self::Scalar(a), Self::Scalar(b)) => {
                    if a != b {
                        return false;
                    }
                }
                (
                    Self::Sequence { span, items },
                    Self::Sequence {
                        span: other_span,
                        items: other_items,
                    },
                ) => {
                    if span != other_span || items.len() != other_items.len() {
                        return false;
                    }
                    pending.extend(items.iter().zip(other_items.iter()));
                }
                (
                    Self::Mapping { span, entries },
                    Self::Mapping {
                        span: other_span,
                        entries: other_entries,
                    },
                ) => {
                    if span != other_span || entries.len() != other_entries.len() {
                        return false;
                    }
                    for (a, b) in entries.iter().zip(other_entries) {
                        if a.key != b.key || a.span != b.span || a.folded() != b.folded() {
                            return false;
                        }
                        pending.push((&a.value, &b.value));
                    }
                }
                _ => return false,
            }
        }
        true
    }
}

/// Prints the value as `#[derive(Debug)]` would, except that the entries of
/// a mapping print as a list of `KeyedEntry { key, span, value }`.
impl fmt::Debug for RawValue<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let pretty = f.alternate();
        // The number of groups that are open, for the indentation of `{:#?}`.
        let mut depth = 0;
        let mut parts = vec![DebugPart::Value(self)];
        while let Some(part) = parts.pop() {
            let group = match part {
                DebugPart::Value(RawValue::Scalar(scalar)) => Group {
                    open: "Scalar(",
                    kind: GroupKind::Tuple,
                    items: vec![(None, DebugPart::Leaf(scalar))],
                },
                DebugPart::Value(RawValue::Sequence { span, items }) => Group {
                    open: "Sequence {",
                    kind: GroupKind::Struct,
                    items: vec![
                        (Some("span"), DebugPart::Leaf(span)),
                        (Some("items"), DebugPart::Items(items)),
                    ],
                },
                DebugPart::Value(RawValue::Mapping { span, entries }) => Group {
                    open: "Mapping {",
                    kind: GroupKind::Struct,
                    items: vec![
                        (Some("span"), DebugPart::Leaf(span)),
                        (Some("entries"), DebugPart::Entries(entries)),
                    ],
                },
                DebugPart::Items(items) => Group {
                    open: "[",
                    kind: GroupKind::List,
                    items: items.iter().map(|item| (None, DebugPart::Value(item))).collect(),
                },
                DebugPart::Entries(entries) => Group {
                    open: "[",
                    kind: GroupKind::List,
                    items: entries
                        .iter()
                        .map(|entry| (None, DebugPart::Entry(entry)))
                        .collect(),
                },
                DebugPart::Entry(entry) => Group {
                    open: "KeyedEntry {",
                    kind: GroupKind::Struct,
                    items: vec![
                        (Some("key"), DebugPart::Leaf(&entry.key)),
                        (Some("span"), DebugPart::Leaf(&entry.span)),
                        (Some("value"), DebugPart::Value(&entry.value)),
                    ],
                },
                DebugPart::Leaf(leaf) => {
                    if pretty {
                        let indent = format!("\n{}", "    ".repeat(depth));
                        f.write_str(&format!("{leaf:#?}").replace('\n', &indent))?;
                    } else {
                        write!(f, "{leaf:?}")?;
                    }
                    continue;
                }
                DebugPart::Name(name) => {
                    f.write_str(name)?;
                    f.write_str(": ")?;
                    continue;
                }
                DebugPart::ItemStart { first, kind } => {
                    if pretty {
                        f.write_str("\n")?;
                        f.write_str(&"    ".repeat(depth))?;
                    } else if !first {
                        f.write_str(", ")?;
                    } else if kind == GroupKind::Struct {
                        f.write_str(" ")?;
                    }
                    continue;
                }
                DebugPart::ItemEnd => {
                    if pretty {
                        f.write_str(",")?;
                    }
                    continue;
                }
                DebugPart::Close { kind, empty } => {
                    depth -= 1;
                    if pretty && !empty {
                        f.write_str("\n")?;
                        f.write_str(&"    ".repeat(depth))?;
                    } else if !pretty && !empty && kind == GroupKind::Struct {
                        f.write_str(" ")?;
                    }
                    f.write_str(match kind {
                        GroupKind::Tuple => ")",
                        GroupKind::Struct => "}",
                        GroupKind::List => "]",
                    })?;
                    continue;
                }
            };
            f.write_str(group.open)?;
            depth += 1;
            let kind = group.kind;
            parts.push(DebugPart::Close {
                kind,
                empty: group.items.is_empty(),
            });
            for (index, (name, item)) in group.items.into_iter().enumerate().rev() {
                parts.push(DebugPart::ItemEnd);
                parts.push(item);
                if let Some(name) = name {
                    parts.push(DebugPart::Name(name));
                }
                parts.push(DebugPart::ItemStart {
                    first: index == 0,
                    kind,
                });
            }
        }
        Ok(())
    }
}

/// A part of the `Debug` output of a [`RawValue`] still to write.
enum DebugPart<'v, 'a> {
    Value(&'v RawValue<'a>),
    Items(&'v [RawValue<'a>]),
    Entries(&'v Keyed<'a, RawValue<'a>>),
    Entry(&'v KeyedEntry<'a, RawValue<'a>>),
    /// A value with a `Debug` implementation that does not recurse into
    /// a `RawValue`.
    Leaf(&'v dyn fmt::Debug),
    /// The name of a struct field.
    Name(&'static str),
    ItemStart {
        first: bool,
        kind: GroupKind,
    },
    ItemEnd,
    Close {
        kind: GroupKind,
        empty: bool,
    },
}

/// A tuple, struct, or list in the `Debug` output of a [`RawValue`].
struct Group<'v, 'a> {
    open: &'static str,
    kind: GroupKind,
    /// The items, each with its field name in a struct.
    items: Vec<(Option<&'static str>, DebugPart<'v, 'a>)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GroupKind {
    Tuple,
    Struct,
    List,
}

/// The items or entries that [`RawValue`]'s `drop` has taken from a value.
enum Children<'a> {
    Items(Arc<[RawValue<'a>]>),
    Entries(Keyed<'a, RawValue<'a>>),
}

/// Drops the nested values with a loop, so that a value nested as deep as
/// the YAML layer allows (ADR 0022) cannot overflow the stack.
impl Drop for RawValue<'_> {
    fn drop(&mut self) {
        let mut stack = Vec::new();
        self.take_children(&mut stack);
        while let Some(mut children) = stack.pop() {
            // Shared children belong to a clone too, and only lose a count.
            match &mut children {
                Children::Items(items) => {
                    for item in Arc::get_mut(items).into_iter().flatten() {
                        item.take_children(&mut stack);
                    }
                }
                Children::Entries(entries) => {
                    for entry in entries.unique_entries_mut().into_iter().flatten() {
                        entry.value.take_children(&mut stack);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::Str;
    use crate::testing::{document, root_entries};

    /// The YAML layer accepts 10,000 levels of nesting (ADR 0022).
    const DEPTH: usize = 10_000;

    /// Runs `f` on a thread with a 2 MiB stack.
    fn on_a_small_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
        std::thread::Builder::new()
            .stack_size(2 << 20)
            .spawn(f)
            .unwrap()
            .join()
            .unwrap()
    }

    /// Wraps `leaf` in `DEPTH` levels, alternately a sequence and a mapping
    /// with the key `key`.
    fn nest<'a>(leaf: RawValue<'a>, key: Str<'a>) -> RawValue<'a> {
        let span = key.span();
        let mut value = leaf;
        for level in 0..DEPTH {
            value = if level % 2 == 0 {
                RawValue::Sequence {
                    span,
                    items: Arc::new([value]),
                }
            } else {
                RawValue::Mapping {
                    span,
                    entries: Keyed::new(vec![KeyedEntry::new(key, span, value)]),
                }
            };
        }
        value
    }

    #[test]
    fn deep_values_compare_and_print_on_a_small_stack() {
        let (equal, different, printed) = on_a_small_stack(|| {
            let document = document("k: a\nother: b\n");
            let entries = root_entries(&document);
            let (key, a) = entries[0];
            let b = entries[1].1;
            let first = nest(RawValue::Scalar(Template::new(a)), key);
            let second = nest(RawValue::Scalar(Template::new(a)), key);
            let third = nest(RawValue::Scalar(Template::new(b)), key);
            let printed = format!("{first:?}");
            (first == second, first == third, printed)
        });
        assert!(equal);
        assert!(!different);
        assert_eq!(printed.matches("Sequence {").count(), DEPTH / 2);
        assert_eq!(printed.matches("KeyedEntry {").count(), DEPTH / 2);
        assert!(printed.starts_with("Mapping { span: "));
    }

    #[test]
    fn values_print_as_a_derived_debug_would() {
        let document = document("k: a\n");
        let (key, a) = root_entries(&document)[0];
        let span = key.span();
        let template = Template::new(a);
        let scalar = RawValue::Scalar(template.clone());
        let value = RawValue::Mapping {
            span,
            entries: Keyed::new(vec![KeyedEntry::new(
                key,
                a.span(),
                RawValue::Sequence {
                    span: a.span(),
                    items: Arc::new([scalar.clone()]),
                },
            )]),
        };
        let inner = a.span();
        assert_eq!(
            format!("{value:?}"),
            format!(
                "Mapping {{ span: {span:?}, entries: [KeyedEntry {{ key: {key:?}, \
                 span: {inner:?}, value: Sequence {{ span: {inner:?}, items: \
                 [Scalar({template:?})] }} }}] }}"
            )
        );
        let empty = RawValue::Sequence {
            span,
            items: Arc::new([]),
        };
        assert_eq!(
            format!("{empty:?}"),
            format!("Sequence {{ span: {span:?}, items: [] }}")
        );
        let indent = |text: String| text.replace('\n', "\n    ");
        assert_eq!(
            format!("{empty:#?}"),
            format!(
                "Sequence {{\n    span: {},\n    items: [],\n}}",
                indent(format!("{span:#?}"))
            )
        );
        assert_eq!(
            format!("{scalar:#?}"),
            format!("Scalar(\n    {},\n)", indent(format!("{template:#?}")))
        );
    }
}
