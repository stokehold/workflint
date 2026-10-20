//! The string leaves and the expression leaf of the AST.

use std::cell::OnceCell;
use std::ops::Range;
use std::sync::Arc;

use workflint_diagnostics::{SourceFile, Span};
use workflint_yaml::{Node, NodeKind, Scalar, ScalarStyle, SourceMap, Tag};

use crate::expression::{implicit, implicit_stray_closing, scan, stray_closings};
use crate::{AliasPath, Placeholder, contains_expression, is_expression_assigned};

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
///
/// A string in a subtree that the parser reached through YAML aliases also
/// has the [path](Self::alias_path) of those aliases, so a diagnostic about
/// it can point at the alias that used it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Str<'a> {
    node: &'a Node,
    scalar: &'a Scalar,
    path: Option<AliasPath>,
}

impl<'a> Str<'a> {
    /// Returns the string of a scalar node, or `None` if `node` is not a
    /// scalar.
    #[must_use]
    pub fn new(node: &'a Node) -> Option<Self> {
        match &node.kind {
            NodeKind::Scalar(scalar) => Some(Self {
                node,
                scalar,
                path: None,
            }),
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

    /// Returns the aliases through which the parser reached the string, or
    /// `None` if it reached it without one.
    /// [`AliasUses::labels`](crate::AliasUses::labels) turns the
    /// path into labels.
    #[must_use]
    pub fn alias_path(&self) -> Option<AliasPath> {
        self.path
    }

    /// Returns the string reached through the aliases `path`.
    pub(crate) fn with_alias_path(self, path: Option<AliasPath>) -> Self {
        Self { path, ..self }
    }

    /// Builds the source map that turns a byte offset in
    /// [`value`](Self::value) into a byte offset in `file`, or returns `None`
    /// if `file` is not the file the node was parsed from.
    #[must_use]
    pub fn source_map<'f>(&self, file: &'f SourceFile) -> Option<SourceMap<'f>> {
        self.node.source_map(file)
    }

    /// Returns the span in `file` of the byte range `range` of
    /// [`value`](Self::value), or `None` if `file` is not the file the node
    /// was parsed from.
    ///
    /// The span covers the source of each character in the range, so a
    /// character that an escape produces is the whole escape, and a range
    /// in a block scalar is on its own line. An empty range is where its
    /// offset maps to. At the end of the value, that is just after the last
    /// character before the trailing line feeds. The source of those line
    /// feeds ends on a later line, so an error at the end of a `|` or `>`
    /// block would otherwise point at the line after the block.
    ///
    /// Each call builds the source map again, which decodes the whole
    /// scalar. A caller that maps many ranges of one string uses
    /// [`spans`](Self::spans) instead.
    #[must_use]
    pub fn source_span(&self, file: &SourceFile, range: Range<usize>) -> Option<Span> {
        let map = self.source_map(file)?;
        Some(map_range(&map, self.value(), range))
    }

    /// Returns a [`StrSpans`] that maps byte ranges of
    /// [`value`](Self::value) to spans in `file` and builds the source map
    /// once for all of them.
    ///
    /// ```
    /// use workflint_diagnostics::{FileId, SourceFile};
    /// use workflint_syntax::Str;
    /// use workflint_yaml::parse;
    ///
    /// let file = SourceFile::new(FileId::new(0), "ci.yml", "\"a\\tb\\tc\"\n").unwrap();
    /// let document = parse(&file).document.unwrap();
    /// let source = Str::new(document.node(document.root().unwrap()).unwrap()).unwrap();
    /// let spans = source.spans(&file);
    /// for range in [0..1, 1..2, 4..5, 5..5] {
    ///     assert_eq!(Some(spans.span(range.clone())), source.source_span(&file, range));
    /// }
    /// // The tab at byte 1 of the value is the escape `\t` in the file.
    /// let tab = spans.span(1..2);
    /// assert_eq!((tab.start, tab.end), (2, 4));
    /// ```
    #[must_use]
    pub fn spans<'f>(&self, file: &'f SourceFile) -> StrSpans<'a, 'f> {
        StrSpans {
            source: *self,
            file,
            map: OnceCell::new(),
        }
    }
}

/// Maps byte ranges of the value of a [`Str`] to spans in its file, as
/// [`Str::source_span`] does. It builds the source map the first time it
/// maps a range and reuses it after that, so mapping the ranges of N
/// diagnostics in one scalar decodes the scalar once instead of N times.
#[derive(Debug)]
pub struct StrSpans<'a, 'f> {
    source: Str<'a>,
    file: &'f SourceFile,
    /// The source map, or `None` inside if `file` is not the file the
    /// string was parsed from.
    map: OnceCell<Option<SourceMap<'f>>>,
}

impl<'a> StrSpans<'a, '_> {
    /// Returns the string whose ranges this maps.
    #[must_use]
    pub fn source(&self) -> Str<'a> {
        self.source
    }

    /// Returns the span in the file of the byte range `range` of the
    /// value, as [`Str::source_span`] does, or the span of the whole string
    /// if the file is not the one the string was parsed from.
    #[must_use]
    pub fn span(&self, range: Range<usize>) -> Span {
        let map = self
            .map
            .get_or_init(|| self.source.source_map(self.file))
            .as_ref();
        map.map_or_else(
            || self.source.span(),
            |map| map_range(map, self.source.value(), range),
        )
    }
}

/// Returns the span that `map` gives the byte range `range` of `value`.
/// See [`Str::source_span`] for an empty range at the end of the value.
fn map_range(map: &SourceMap<'_>, value: &str, range: Range<usize>) -> Span {
    let range = if range.start >= value.len() && range.end <= range.start {
        let end = value.trim_end_matches('\n').len();
        end..end
    } else {
        range
    };
    map.source_span(range)
}

/// A string in a place where GitHub evaluates the `${{ }}` placeholders in
/// it, such as `name`, `run`, or an `env` value, with each placeholder
/// parsed.
///
/// The places are those the reference's `expression` rule checks. Strings
/// elsewhere, such as a job ID or a `permissions` scope, are plain [`Str`]
/// values, because the reference never parses them.
#[derive(Debug, Clone, PartialEq)]
pub struct Template<'a> {
    /// The scalar.
    pub source: Str<'a>,
    /// The placeholders and stray `}}`, or `None` if there are none. One
    /// thin pointer keeps the many copies of a template small.
    parts: Option<Arc<Parts>>,
}

/// The parsed parts of a [`Template`].
#[derive(Debug, PartialEq)]
struct Parts {
    /// The placeholders in source order.
    placeholders: Vec<Placeholder>,
    /// The offsets of the `}}` with no opening `${{`, in source order.
    stray_closings: Vec<u32>,
}

impl<'a> Template<'a> {
    /// Parses every `${{ }}` in `source`.
    ///
    /// ```
    /// use workflint_diagnostics::{FileId, SourceFile};
    /// use workflint_syntax::{Str, Template};
    /// use workflint_yaml::parse;
    ///
    /// let file = SourceFile::new(FileId::new(0), "ci.yml", "echo ${{ a }} ${{ b. }}\n").unwrap();
    /// let document = parse(&file).document.unwrap();
    /// let source = Str::new(document.node(document.root().unwrap()).unwrap()).unwrap();
    ///
    /// let template = Template::new(source);
    /// let [a, b] = template.placeholders() else { panic!() };
    /// assert_eq!(&template.value()[a.range()], "${{ a }}");
    /// assert!(a.expr.is_ok());
    /// let err = b.expr.as_ref().unwrap_err();
    /// let span = template.source.source_span(&file, b.to_value(err.range())).unwrap();
    /// assert_eq!(file.line_col(span.start).column, 22);
    /// ```
    #[must_use]
    pub fn new(source: Str<'a>) -> Self {
        let value = source.value();
        let placeholders = scan(value);
        let strays = stray_closings(value, &placeholders);
        Self::with(source, placeholders, strays)
    }

    /// Parses `source` as an `if` condition, as the reference's
    /// `checkIfCondition` does: every `${{ }}` in it if it
    /// [contains an expression](contains_expression), and otherwise the
    /// whole string as one [implicit](Placeholder::is_implicit) expression.
    #[must_use]
    pub fn condition(source: Str<'a>) -> Self {
        if contains_expression(source.value()) {
            Self::new(source)
        } else {
            let value = source.value();
            let strays = implicit_stray_closing(value).into_iter().collect();
            Self::with(source, vec![implicit(value)], strays)
        }
    }

    /// Returns `source` with no placeholders.
    pub(crate) fn plain(source: Str<'a>) -> Self {
        Self {
            source,
            parts: None,
        }
    }

    fn with(source: Str<'a>, placeholders: Vec<Placeholder>, stray_closings: Vec<u32>) -> Self {
        let parts = (!placeholders.is_empty() || !stray_closings.is_empty()).then(|| {
            Arc::new(Parts {
                placeholders,
                stray_closings,
            })
        });
        Self { source, parts }
    }

    /// Returns the decoded value of the scalar.
    #[must_use]
    pub fn value(&self) -> &'a str {
        self.source.value()
    }

    /// Returns the span of the scalar.
    #[must_use]
    pub fn span(&self) -> Span {
        self.source.span()
    }

    /// Returns the placeholders in source order.
    #[must_use]
    pub fn placeholders(&self) -> &[Placeholder] {
        self.parts
            .as_deref()
            .map_or(&[], |parts| &parts.placeholders[..])
    }

    /// Returns the byte offset in the value of each `}}` that has no
    /// opening `${{` and closes no `{` in the text, in source order. For an
    /// `if` condition without `${{ }}`, it is the `}}` that ends the
    /// condition before the end of the value, if there is one.
    ///
    /// ```
    /// use workflint_diagnostics::{FileId, SourceFile};
    /// use workflint_syntax::{Str, Template};
    /// use workflint_yaml::parse;
    ///
    /// let file = SourceFile::new(FileId::new(0), "ci.yml", "echo ${{ a }} b }}\n").unwrap();
    /// let document = parse(&file).document.unwrap();
    /// let source = Str::new(document.node(document.root().unwrap()).unwrap()).unwrap();
    ///
    /// assert_eq!(Template::new(source).stray_closings(), [16]);
    /// ```
    #[must_use]
    pub fn stray_closings(&self) -> &[u32] {
        self.parts
            .as_deref()
            .map_or(&[], |parts| &parts.stray_closings[..])
    }
}

/// A value written as a single `${{ }}` expression, such as
/// `continue-on-error: ${{ matrix.experimental }}`.
///
/// It holds the scalar the expression is written in and the parsed
/// expression. Strings that contain expressions among other text, such as
/// `run:` scripts, are [`Template`] values.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr<'a> {
    /// The scalar that holds the expression, `${{` and `}}` included.
    pub source: Str<'a>,
    placeholder: Arc<Placeholder>,
}

impl<'a> Expr<'a> {
    /// Parses the expression written in `source`, or returns `None` if
    /// `source` is not a [single expression](is_expression_assigned).
    #[must_use]
    pub fn new(source: Str<'a>) -> Option<Self> {
        if !is_expression_assigned(source.value()) {
            return None;
        }
        let placeholder = scan(source.value()).into_iter().next()?;
        Some(Self::with(source, Arc::new(placeholder)))
    }

    pub(crate) fn with(source: Str<'a>, placeholder: Arc<Placeholder>) -> Self {
        Self {
            source,
            placeholder,
        }
    }

    /// Returns the expression's placeholder.
    #[must_use]
    pub fn placeholder(&self) -> &Placeholder {
        &self.placeholder
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
