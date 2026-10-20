//! Values under a key: absent, invalid, or valid, and for [`Value`] a
//! literal or an expression.

use std::fmt;
use std::ops::Deref;

use workflint_diagnostics::Span;

use crate::{Expr, Str};

/// A key of a section whose value may be a literal or a `${{ }}`
/// expression, such as `continue-on-error`, `timeout-minutes`, or `env`.
///
/// The four variants keep apart what the reference conflates with `nil`
/// pointers and zero values: a key that is not written, a value that is
/// written but invalid, a literal, and an expression.
///
/// `key` is the key the value is written under. It is `None` when the value
/// comes from a short form with no key of its own, such as the labels of
/// `runs-on: ${{ inputs.runner }}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Value<'a, T> {
    /// The key is not written.
    #[default]
    Absent,
    /// The key is written, but its value has the wrong type or is otherwise
    /// invalid. The parser has reported it.
    Invalid {
        /// The key.
        key: Option<Str<'a>>,
        /// The span of the invalid value.
        span: Span,
    },
    /// A literal value.
    Literal {
        /// The key.
        key: Option<Str<'a>>,
        /// The span of the value.
        span: Span,
        /// The value.
        value: T,
    },
    /// A value written as a single `${{ }}` expression.
    Expression {
        /// The key.
        key: Option<Str<'a>>,
        /// The expression. Its span is the span of the value.
        expr: Expr<'a>,
    },
}

impl<'a, T> Value<'a, T> {
    /// Returns the key, or `None` for an absent value or a value with no key.
    #[must_use]
    pub fn key(&self) -> Option<&Str<'a>> {
        match self {
            Self::Absent => None,
            Self::Invalid { key, .. }
            | Self::Literal { key, .. }
            | Self::Expression { key, .. } => key.as_ref(),
        }
    }

    /// Returns the span of the value, or `None` for an absent value.
    #[must_use]
    pub fn span(&self) -> Option<Span> {
        match self {
            Self::Absent => None,
            Self::Invalid { span, .. } | Self::Literal { span, .. } => Some(*span),
            Self::Expression { expr, .. } => Some(expr.span()),
        }
    }

    /// Returns the literal, or `None` if the value is not a literal.
    #[must_use]
    pub fn literal(&self) -> Option<&T> {
        match self {
            Self::Literal { value, .. } => Some(value),
            Self::Absent | Self::Invalid { .. } | Self::Expression { .. } => None,
        }
    }

    /// Returns the expression, or `None` if the value is not an expression.
    #[must_use]
    pub fn expression(&self) -> Option<&Expr<'a>> {
        match self {
            Self::Expression { expr, .. } => Some(expr),
            Self::Absent | Self::Invalid { .. } | Self::Literal { .. } => None,
        }
    }

    /// Returns whether the key is written, with a valid value or not.
    #[must_use]
    pub fn is_present(&self) -> bool {
        !matches!(self, Self::Absent)
    }

    /// Returns whether the value is a literal or an expression.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Literal { .. } | Self::Expression { .. })
    }
}

/// A key of a section whose value cannot be an expression, such as `name`,
/// `steps`, or `permissions`.
///
/// `key` is `None` when the value comes from a short form with no key of
/// its own, such as the group of `concurrency: ci` or the image of
/// `container: node:24`.
///
/// A list of strings, such as `types` of a webhook event, is a
/// `Field<Arc<[Option<Template>]>>`. The field is invalid when the value is
/// not a list, and an item is `None` when the item is not a valid string.
/// A string where a list is expected is a list of one, as in the reference,
/// so `types: ''` is a valid list whose only item is `None`.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Field<'a, T> {
    /// The key is not written.
    #[default]
    Absent,
    /// The key is written, but its value has the wrong type or is otherwise
    /// invalid. The parser has reported it.
    Invalid {
        /// The key.
        key: Option<Str<'a>>,
        /// The span of the invalid value.
        span: Span,
    },
    /// A valid value.
    Valid {
        /// The key.
        key: Option<Str<'a>>,
        /// The span of the value.
        span: Span,
        /// The value.
        value: T,
    },
}

impl<'a, T> Field<'a, T> {
    /// Returns the key, or `None` for an absent value or a value with no key.
    #[must_use]
    pub fn key(&self) -> Option<&Str<'a>> {
        match self {
            Self::Absent => None,
            Self::Invalid { key, .. } | Self::Valid { key, .. } => key.as_ref(),
        }
    }

    /// Returns the span of the value, or `None` for an absent value.
    #[must_use]
    pub fn span(&self) -> Option<Span> {
        match self {
            Self::Absent => None,
            Self::Invalid { span, .. } | Self::Valid { span, .. } => Some(*span),
        }
    }

    /// Returns the value, or `None` if it is absent or invalid.
    #[must_use]
    pub fn value(&self) -> Option<&T> {
        match self {
            Self::Valid { value, .. } => Some(value),
            Self::Absent | Self::Invalid { .. } => None,
        }
    }

    /// Returns whether the key is written, with a valid value or not.
    #[must_use]
    pub fn is_present(&self) -> bool {
        !matches!(self, Self::Absent)
    }

    /// Returns whether the value is valid.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Valid { .. })
    }

    /// Converts a valid value with `f`, and keeps the key and the span.
    pub(crate) fn map<U>(self, f: impl FnOnce(T) -> U) -> Field<'a, U> {
        match self {
            Self::Absent => Field::Absent,
            Self::Invalid { key, span } => Field::Invalid { key, span },
            Self::Valid { key, span, value } => Field::Valid {
                key,
                span,
                value: f(value),
            },
        }
    }
}

/// A [`Field`] or [`Value`] of a key that few sections write, such as
/// `timeout-minutes` of a step.
///
/// An absent value takes one word instead of the size of the value, and a
/// present one is boxed. It dereferences to the value, so a rule reads it as
/// it reads a [`Field`] or [`Value`]. A large workflow has hundreds of steps,
/// and most of their keys are absent.
#[derive(Clone, Default)]
pub struct Rare<T>(Option<Box<T>>);

impl<'a, T> From<Field<'a, T>> for Rare<Field<'a, T>> {
    fn from(field: Field<'a, T>) -> Self {
        Self(field.is_present().then(|| Box::new(field)))
    }
}

impl<'a, T> From<Value<'a, T>> for Rare<Value<'a, T>> {
    fn from(value: Value<'a, T>) -> Self {
        Self(value.is_present().then(|| Box::new(value)))
    }
}

impl<'a, T> Deref for Rare<Field<'a, T>> {
    type Target = Field<'a, T>;

    fn deref(&self) -> &Self::Target {
        self.0.as_deref().unwrap_or(&Field::Absent)
    }
}

impl<'a, T> Deref for Rare<Value<'a, T>> {
    type Target = Value<'a, T>;

    fn deref(&self) -> &Self::Target {
        self.0.as_deref().unwrap_or(&Value::Absent)
    }
}

impl<T: fmt::Debug> fmt::Debug for Rare<T>
where
    Self: Deref<Target = T>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        T::fmt(self, f)
    }
}

impl<T: PartialEq> PartialEq for Rare<T>
where
    Self: Deref<Target = T>,
{
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;
    use crate::Template;
    use crate::testing::{document, root_entries};

    #[test]
    fn a_literal_has_its_key_span_and_value() {
        let document = document("continue-on-error: true\n");
        let [(key, value)] = root_entries(&document)[..] else {
            panic!()
        };
        let field = Value::Literal {
            key: Some(key),
            span: value.span(),
            value: true,
        };

        assert_eq!(field.literal(), Some(&true));
        assert!(field.expression().is_none());
        assert_eq!(field.key().map(Str::value), Some("continue-on-error"));
        assert_eq!(field.span(), Some(value.span()));
        assert!(field.is_present());
        assert!(field.is_valid());
    }

    #[test]
    fn an_expression_is_not_a_literal() {
        let document = document("timeout-minutes: ${{ inputs.timeout }}\n");
        let [(key, value)] = root_entries(&document)[..] else {
            panic!()
        };
        let field: Value<'_, f64> = Value::Expression {
            key: Some(key),
            expr: Expr::new(value).unwrap(),
        };

        assert!(field.literal().is_none());
        assert_eq!(
            field.expression().map(|e| e.source.value()),
            Some("${{ inputs.timeout }}")
        );
        assert_eq!(field.span(), Some(value.span()));
        assert!(field.is_valid());
    }

    #[test]
    fn an_absent_value_has_no_key_and_no_span() {
        let field: Value<'_, bool> = Value::default();

        assert!(matches!(field, Value::Absent));
        assert!(!field.is_present());
        assert!(!field.is_valid());
        assert!(field.key().is_none());
        assert!(field.span().is_none());
        assert!(field.literal().is_none());
    }

    #[test]
    fn an_invalid_value_is_present_but_has_no_literal_or_expression() {
        // `yes` is a string in YAML 1.2, so it is not a boolean literal.
        let document = document("fail-fast: yes\n");
        let [(key, value)] = root_entries(&document)[..] else {
            panic!()
        };
        let field: Value<'_, bool> = Value::Invalid {
            key: Some(key),
            span: value.span(),
        };

        assert!(field.is_present());
        assert!(!field.is_valid());
        assert_eq!(field.span(), Some(value.span()));
        assert!(field.literal().is_none());
        assert!(field.expression().is_none());
    }

    #[test]
    fn a_field_tells_absent_invalid_and_valid_apart() {
        let document = document("name: build\n");
        let [(key, value)] = root_entries(&document)[..] else {
            panic!()
        };

        let absent: Field<'_, Str<'_>> = Field::default();
        let invalid: Field<'_, Str<'_>> = Field::Invalid {
            key: Some(key),
            span: value.span(),
        };
        let valid = Field::Valid {
            key: Some(key),
            span: value.span(),
            value,
        };

        assert!(!absent.is_present() && absent.value().is_none());
        assert!(invalid.is_present() && !invalid.is_valid() && invalid.value().is_none());
        assert_eq!(valid.value().map(Str::value), Some("build"));
        assert_eq!(valid.key().map(Str::value), Some("name"));
    }

    #[test]
    fn an_absent_rare_field_reads_as_absent_and_takes_one_word() {
        let field: Rare<Field<'_, Template<'_>>> = Field::Absent.into();

        assert_eq!(*field, Field::Absent);
        assert_eq!(field, Rare::default());
        assert_eq!(size_of_val(&field), size_of::<usize>());
    }

    #[test]
    fn a_present_rare_value_reads_as_its_value() {
        let document = document("continue-on-error: true\n");
        let [(key, value)] = root_entries(&document)[..] else {
            panic!()
        };
        let literal = Value::Literal {
            key: Some(key),
            span: value.span(),
            value: true,
        };
        let rare: Rare<Value<'_, bool>> = literal.clone().into();

        assert_eq!(*rare, literal);
        assert_eq!(rare.literal(), Some(&true));
        assert_eq!(format!("{rare:?}"), format!("{literal:?}"));
    }
}
