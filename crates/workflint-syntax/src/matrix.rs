//! `strategy.matrix` and the YAML values it holds.

use std::mem;
use std::sync::Arc;

use workflint_diagnostics::Span;

use crate::{Expr, Keyed, Template, Value};

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
#[derive(Debug, Clone, PartialEq)]
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
