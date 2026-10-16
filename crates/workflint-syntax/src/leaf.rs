//! The string leaf and the expression leaf of the AST.

use workflint_diagnostics::{SourceFile, Span};
use workflint_yaml::{Node, NodeKind, Scalar, ScalarStyle, SourceMap, Tag};

/// A YAML scalar as a string: its decoded value and the node it comes from.
///
/// Every key and every string value of the AST is a `Str`. It borrows the
/// node from the [`Document`](workflint_yaml::Document), so copying it is
/// cheap and the AST never copies a scalar's text.
///
/// [`span`](Self::span) starts where the reference's `posAt(n)` points: at
/// the `&` or `!` of the node's first property, at the opening quote of a
/// quoted scalar, at the `|` or `>` of a block scalar, and right after the
/// colon for an implicit null such as `key:`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Str<'a> {
    node: &'a Node,
    scalar: &'a Scalar,
}

impl<'a> Str<'a> {
    /// Returns the string of a scalar node, or `None` if `node` is not a
    /// scalar.
    #[must_use]
    pub fn new(node: &'a Node) -> Option<Self> {
        match &node.kind {
            NodeKind::Scalar(scalar) => Some(Self { node, scalar }),
            NodeKind::Sequence(_) | NodeKind::Mapping(_) | NodeKind::Alias(_) => None,
        }
    }

    /// Returns the decoded value: unquoted, unescaped, folded, and chomped.
    #[must_use]
    pub fn value(&self) -> &'a str {
        &self.scalar.value
    }

    /// Returns the span of the node, from its first property to its end.
    #[must_use]
    pub fn span(&self) -> Span {
        self.node.span
    }

    /// Returns how the scalar is written.
    #[must_use]
    pub fn style(&self) -> ScalarStyle {
        self.scalar.style
    }

    /// Returns whether the scalar is single-quoted or double-quoted, which is
    /// what the reference's `String.Quoted` records. Block scalars are not
    /// quoted.
    #[must_use]
    pub fn is_quoted(&self) -> bool {
        matches!(
            self.scalar.style,
            ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted
        )
    }

    /// Returns the tag of the scalar.
    #[must_use]
    pub fn tag(&self) -> &'a Tag {
        &self.scalar.tag
    }

    /// Returns the YAML node.
    #[must_use]
    pub fn node(&self) -> &'a Node {
        self.node
    }

    /// Builds the source map that turns a byte offset in
    /// [`value`](Self::value) into a byte offset in `file`, or returns `None`
    /// if `file` is not the file the node was parsed from.
    #[must_use]
    pub fn source_map<'f>(&self, file: &'f SourceFile) -> Option<SourceMap<'f>> {
        self.node.source_map(file)
    }
}

/// A value written as a single `${{ }}` expression, such as
/// `continue-on-error: ${{ matrix.experimental }}`.
///
/// It holds the scalar the expression is written in. Strings that contain
/// expressions among other text, such as `run:` scripts, stay plain [`Str`]
/// values.
// TODO(#73): add the parsed expression, shared through `Arc` so that an
// aliased scalar's expression is parsed once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Expr<'a> {
    /// The scalar that holds the expression, `${{` and `}}` included.
    pub source: Str<'a>,
}

impl<'a> Expr<'a> {
    /// Returns the expression written in `source`.
    #[must_use]
    pub const fn new(source: Str<'a>) -> Self {
        Self { source }
    }

    /// Returns the span of the scalar that holds the expression.
    #[must_use]
    pub fn span(&self) -> Span {
        self.source.span()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;
    use crate::testing::{document, root_entries};

    #[test]
    fn a_string_points_at_the_anchor_of_its_node() {
        let document = document("on: &a push\n");
        let [(key, value)] = root_entries(&document)[..] else {
            panic!()
        };

        assert_eq!((key.span().start, key.span().end), (0, 2));
        assert_eq!(value.value(), "push");
        assert_eq!(value.span().start, 4);
    }

    #[test]
    fn only_single_and_double_quoted_strings_are_quoted() {
        let document = document("a: plain\nb: 'single'\nc: \"double\"\nd: |\n  block\n");
        let quoted: Vec<_> = root_entries(&document)
            .iter()
            .map(|(_, value)| value.is_quoted())
            .collect();

        assert_eq!(quoted, [false, true, true, false]);
    }

    #[test]
    fn a_string_keeps_the_tag_of_its_scalar() {
        let document = document("a: 1\nb: '1'\nc: true\n");
        let tags: Vec<_> = root_entries(&document)
            .iter()
            .map(|(_, value)| value.tag().as_str())
            .collect();

        assert_eq!(tags, ["!!int", "!!str", "!!bool"]);
    }

    #[test]
    fn only_a_scalar_node_is_a_string() {
        let document = document("a: [b]\n");
        let root = document.node(document.root().unwrap()).unwrap();

        assert!(Str::new(root).is_none());
    }
}
