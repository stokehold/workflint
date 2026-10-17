//! The leaf parsers: strings, string sequences, booleans, numbers, and
//! single expressions, as the reference's helpers in `parse.go:136-314`.

use std::sync::Arc;

use workflint_diagnostics::quote::quote;
use workflint_yaml::{Node, NodeId, NodeKind, Tag};

use super::{Parser, kind_name, tag_name};
use crate::rules::{
    EMPTY_SECTION, EMPTY_STRING, EXPECTED_SINGLE_EXPRESSION, INVALID_LITERAL_VALUE,
    UNEXPECTED_NODE_KIND,
};
use crate::{
    Expr, Field, Keyed, KeyedEntry, Str, Template, Value, is_expression_assigned, strconv,
};

use super::MapEntry;

impl<'a> Parser<'a, '_> {
    /// Checks that `node` is a scalar, and unless `allow_empty`, that it is
    /// not empty. The reference's `checkString`.
    ///
    /// The tag is not checked, so `42`, `true`, and `~` are the strings
    /// `42`, `true`, and `~`.
    pub(crate) fn check_string(&mut self, node: &'a Node, allow_empty: bool) -> Option<Str<'a>> {
        let Some(string) = Str::new(node) else {
            self.error(
                &UNEXPECTED_NODE_KIND,
                node.span,
                format!(
                    "expected scalar node for string value but found {} node with {} tag",
                    kind_name(node),
                    quote(tag_name(node))
                ),
            );
            return None;
        };
        if !allow_empty && string.value().is_empty() {
            self.error(
                &EMPTY_STRING,
                node.span,
                "string should not be empty".to_owned(),
            );
            return None;
        }
        Some(string)
    }

    /// Parses `node`, the value of `key`, as a string. The reference's
    /// `parseString`.
    pub(crate) fn parse_string(
        &mut self,
        key: Option<Str<'a>>,
        node: &'a Node,
        allow_empty: bool,
    ) -> Field<'a, Str<'a>> {
        let span = node.span;
        match self.check_string(node, allow_empty) {
            Some(value) => Field::Valid { key, span, value },
            None => Field::Invalid { key, span },
        }
    }

    /// Checks that `node` is a sequence, and unless `allow_empty`, that it
    /// has items, and returns the items. The reference's `checkSequence`.
    pub(crate) fn check_sequence(
        &mut self,
        section: &str,
        node: &'a Node,
        allow_empty: bool,
    ) -> Option<&'a [NodeId]> {
        let NodeKind::Sequence(sequence) = &node.kind else {
            self.error(
                &UNEXPECTED_NODE_KIND,
                node.span,
                format!(
                    "{} section must be sequence node but got {} node with {} tag",
                    quote(section),
                    kind_name(node),
                    quote(tag_name(node))
                ),
            );
            return None;
        };
        let items = self.document.items(sequence);
        if !allow_empty && items.is_empty() {
            self.error(
                &EMPTY_SECTION,
                node.span,
                format!("{} section should not be empty", quote(section)),
            );
            return None;
        }
        Some(items)
    }

    /// Parses `node`, the value of `key`, as a sequence of strings. The
    /// reference's `parseStringSequence`.
    ///
    /// An item that is not a valid string is reported and kept as `None`,
    /// so the sequence keeps its length. The reference keeps it as `""`,
    /// and its rules report that string again. Rules skip `None`.
    pub(crate) fn parse_string_sequence(
        &mut self,
        key: Option<Str<'a>>,
        section: &str,
        node: &'a Node,
        allow_empty: bool,
        allow_item_empty: bool,
    ) -> Field<'a, Arc<[Option<Str<'a>>]>> {
        let span = node.span;
        let Some(items) = self.check_sequence(section, node, allow_empty) else {
            return Field::Invalid { key, span };
        };
        let mut strings = Vec::with_capacity(items.len());
        for &id in items {
            if let Some(item) = self.node(id) {
                strings.push(self.check_string(item, allow_item_empty));
            }
        }
        Field::Valid {
            key,
            span,
            value: strings.into(),
        }
    }

    /// Parses `node`, the value of `key`, as a string or a sequence of
    /// strings. The reference's `parseStringOrStringSequence`.
    ///
    /// A null scalar is an empty sequence if `allow_empty`. Any other
    /// scalar is a sequence of one item, which is `None` if the scalar is
    /// not a valid string, as in
    /// [`parse_string_sequence`](Self::parse_string_sequence).
    pub(crate) fn parse_string_or_string_sequence(
        &mut self,
        key: Option<Str<'a>>,
        section: &str,
        node: &'a Node,
        allow_empty: bool,
        allow_item_empty: bool,
    ) -> Field<'a, Arc<[Option<Str<'a>>]>> {
        let span = node.span;
        match &node.kind {
            NodeKind::Scalar(scalar) if allow_empty && scalar.tag == Tag::Null => Field::Valid {
                key,
                span,
                value: Arc::new([]),
            },
            NodeKind::Scalar(_) => Field::Valid {
                key,
                span,
                value: Arc::new([self.check_string(node, allow_item_empty)]),
            },
            NodeKind::Sequence(_) | NodeKind::Mapping(_) | NodeKind::Alias(_) => {
                self.parse_string_sequence(key, section, node, allow_empty, allow_item_empty)
            }
        }
    }

    /// Parses the placeholders of `source`, or returns them from the cache.
    /// Their errors are reported after the walk, by
    /// [`report_expressions`](Self::report_expressions).
    pub(crate) fn template(&mut self, source: Str<'a>) -> Template<'a> {
        self.cached_template(source, false)
    }

    /// Parses `source` as an `if` condition, or returns it from the cache.
    pub(crate) fn condition(&mut self, source: Str<'a>) -> Template<'a> {
        self.cached_template(source, true)
    }

    fn cached_template(&mut self, source: Str<'a>, condition: bool) -> Template<'a> {
        let key = (address(source.node()), condition);
        if let Some(template) = self.templates.get(&key) {
            return template.clone();
        }
        let template = if condition {
            Template::condition(source)
        } else {
            Template::new(source)
        };
        self.templates.insert(key, template.clone());
        template
    }

    /// Parses `source` as a single expression, or returns it from the
    /// cache. Returns `None` if `source` is not a single expression.
    fn expression(&mut self, source: Str<'a>) -> Option<Expr<'a>> {
        let key = address(source.node());
        if let Some(expr) = self.expressions.get(&key) {
            return Some(expr.clone());
        }
        let expr = Expr::new(source)?;
        self.expressions.insert(key, expr.clone());
        Some(expr)
    }

    /// Parses `node`, the value of `key`, as a string with placeholders.
    pub(crate) fn parse_template(
        &mut self,
        key: Option<Str<'a>>,
        node: &'a Node,
        allow_empty: bool,
    ) -> Field<'a, Template<'a>> {
        self.parse_string(key, node, allow_empty)
            .map(|value| self.template(value))
    }

    /// Parses `node`, the value of `key`, as an `if` condition.
    pub(crate) fn parse_condition(
        &mut self,
        key: Option<Str<'a>>,
        node: &'a Node,
    ) -> Field<'a, Template<'a>> {
        self.parse_string(key, node, false)
            .map(|value| self.condition(value))
    }

    /// Parses `node`, the value of `key`, as a sequence of strings with
    /// placeholders. See [`parse_string_sequence`](Self::parse_string_sequence).
    pub(crate) fn parse_template_sequence(
        &mut self,
        key: Option<Str<'a>>,
        section: &str,
        node: &'a Node,
        allow_empty: bool,
        allow_item_empty: bool,
    ) -> Field<'a, Arc<[Option<Template<'a>>]>> {
        self.parse_string_sequence(key, section, node, allow_empty, allow_item_empty)
            .map(|strings| self.templates(&strings))
    }

    /// Parses `node`, the value of `key`, as a string or a sequence of
    /// strings with placeholders. See
    /// [`parse_string_or_string_sequence`](Self::parse_string_or_string_sequence).
    pub(crate) fn parse_template_or_template_sequence(
        &mut self,
        key: Option<Str<'a>>,
        section: &str,
        node: &'a Node,
        allow_empty: bool,
        allow_item_empty: bool,
    ) -> Field<'a, Arc<[Option<Template<'a>>]>> {
        self.parse_string_or_string_sequence(key, section, node, allow_empty, allow_item_empty)
            .map(|strings| self.templates(&strings))
    }

    /// Parses the placeholders of each valid item of `strings`.
    fn templates(&mut self, strings: &[Option<Str<'a>>]) -> Arc<[Option<Template<'a>>]> {
        strings
            .iter()
            .map(|string| string.map(|string| self.template(string)))
            .collect()
    }

    /// Collects the entries of a case-insensitive mapping of strings with
    /// placeholders, such as `with` or `outputs`. A value that is not a
    /// string, or is empty unless `allow_empty`, is reported and kept as
    /// `None`. See [`keyed_strings`](Self::keyed_strings).
    pub(crate) fn keyed_templates(
        &mut self,
        entries: Vec<MapEntry<'a>>,
        allow_empty: bool,
    ) -> Keyed<'a, Option<Template<'a>>> {
        let mut keyed = Vec::with_capacity(entries.len());
        for entry in entries {
            let value = self
                .check_string(entry.value, allow_empty)
                .map(|value| self.template(value));
            if let Some(name) = entry.key.string {
                keyed.push(KeyedEntry::new(name, entry.value.span, value));
            }
        }
        Keyed::new(keyed)
    }

    /// Parses `node` as a single `${{ }}` expression, and otherwise reports
    /// that it should be one or `expecting`. The reference's
    /// `parseExpression`, which does not check the tag.
    pub(crate) fn parse_expression(&mut self, node: &'a Node, expecting: &str) -> Option<Expr<'a>> {
        match Str::new(node) {
            Some(source) if is_expression_assigned(source.value()) => self.expression(source),
            _ => {
                self.error(
                    &EXPECTED_SINGLE_EXPRESSION,
                    node.span,
                    format!(
                        "expecting a single ${{{{...}}}} expression or {expecting}, but found plain text node"
                    ),
                );
                None
            }
        }
    }

    /// Returns `node` as an expression if it is a `!!str` scalar that is a
    /// single `${{ }}` expression, and reports nothing otherwise. The
    /// reference's `mayParseExpression`.
    pub(crate) fn may_parse_expression(&mut self, node: &'a Node) -> Option<Expr<'a>> {
        Str::new(node)
            .filter(|source| *source.tag() == Tag::Str && is_expression_assigned(source.value()))
            .and_then(|source| self.expression(source))
    }

    /// Parses `node`, the value of `key`, as a boolean: a `!!bool` scalar or
    /// a `!!str` scalar with a single expression. The reference's
    /// `parseBool`.
    ///
    /// `true`, `True`, and `TRUE` are true, as YAML defines them
    /// (`docs/differences.md#capitalized-booleans`). Any other `!!bool`
    /// value, such as `!!bool yes`, is false, as in the reference.
    pub(crate) fn parse_bool(&mut self, key: Option<Str<'a>>, node: &'a Node) -> Value<'a, bool> {
        let span = node.span;
        match Str::new(node).map(|string| (string, string.tag())) {
            Some((string, Tag::Bool)) => Value::Literal {
                key,
                span,
                value: matches!(string.value(), "true" | "True" | "TRUE"),
            },
            Some((_, Tag::Str)) => {
                self.literal_expression(key, node, "boolean literal \"true\" or \"false\"")
            }
            _ => {
                self.wrong_literal(node, "expected bool value but found");
                Value::Invalid { key, span }
            }
        }
    }

    /// Parses `node`, the value of `key`, as an integer: a `!!int` scalar
    /// that Go's `strconv.Atoi` accepts, or a `!!str` scalar with a single
    /// expression. The reference's `parseInt`.
    pub(crate) fn parse_int(&mut self, key: Option<Str<'a>>, node: &'a Node) -> Value<'a, i64> {
        let span = node.span;
        match Str::new(node).map(|string| (string, string.tag())) {
            Some((string, Tag::Int)) => match strconv::atoi(string.value()) {
                Ok(value) => Value::Literal { key, span, value },
                Err(err) => {
                    self.invalid_number(node, "integer", string.value(), &err);
                    Value::Invalid { key, span }
                }
            },
            Some((_, Tag::Str)) => self.literal_expression(key, node, "integer literal"),
            _ => {
                self.wrong_literal(node, "expected scalar node for integer value but found");
                Value::Invalid { key, span }
            }
        }
    }

    /// Parses `node`, the value of `key`, as a float: a `!!float` or
    /// `!!int` scalar that Go's `strconv.ParseFloat` accepts, or a `!!str`
    /// scalar with a single expression. The reference's `parseFloat`.
    ///
    /// NaN, which only an explicit tag such as `!!float NaN` can give, is
    /// invalid (`docs/differences.md#nan-panic`).
    pub(crate) fn parse_float(&mut self, key: Option<Str<'a>>, node: &'a Node) -> Value<'a, f64> {
        let span = node.span;
        match Str::new(node).map(|string| (string, string.tag())) {
            Some((string, Tag::Float | Tag::Int)) => {
                let value = string.value();
                match strconv::parse_float(value) {
                    Ok(value) if !value.is_nan() => Value::Literal { key, span, value },
                    // The reference crashes on NaN. The message is the one
                    // Go gives for other text that is not a number.
                    Ok(_) => {
                        let err = format!(
                            "strconv.ParseFloat: parsing {}: invalid syntax",
                            quote(value)
                        );
                        self.invalid_number(node, "float", value, &err);
                        Value::Invalid { key, span }
                    }
                    Err(err) => {
                        self.invalid_number(node, "float", value, &err);
                        Value::Invalid { key, span }
                    }
                }
            }
            Some((_, Tag::Str)) => self.literal_expression(key, node, "float number literal"),
            _ => {
                self.wrong_literal(node, "expected scalar node for float value but found");
                Value::Invalid { key, span }
            }
        }
    }

    /// Parses `node`, the value of `key`, as a single expression, as
    /// [`parse_expression`](Self::parse_expression) does, and returns an
    /// invalid value if it is not one.
    pub(crate) fn literal_expression<T>(
        &mut self,
        key: Option<Str<'a>>,
        node: &'a Node,
        expecting: &str,
    ) -> Value<'a, T> {
        match self.parse_expression(node, expecting) {
            Some(expr) => Value::Expression { key, expr },
            None => Value::Invalid {
                key,
                span: node.span,
            },
        }
    }

    fn wrong_literal(&mut self, node: &Node, prefix: &str) {
        self.error(
            &INVALID_LITERAL_VALUE,
            node.span,
            format!(
                "{prefix} {} node with {} tag",
                kind_name(node),
                quote(tag_name(node))
            ),
        );
    }

    fn invalid_number(&mut self, node: &Node, kind: &str, value: &str, err: &str) {
        self.error(
            &INVALID_LITERAL_VALUE,
            node.span,
            format!("invalid {kind} value: {}: {err}", quote(value)),
        );
    }
}

/// Returns the address of `node`, which tells nodes apart while the
/// document lives.
fn address(node: &Node) -> usize {
    std::ptr::from_ref(node).addr()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;
    use crate::testing::{Text, parse_with};

    /// Runs `leaf` on the value of `v` in `text`, and returns the
    /// diagnostics followed by the result as `Debug` text of what it holds.
    fn leaf(
        text: &str,
        leaf: impl for<'a, 'f> Fn(&mut Parser<'a, 'f>, &'a Node) -> String,
    ) -> Vec<String> {
        parse_with(text, |parser, root| {
            let NodeKind::Mapping(mapping) = &root.kind else {
                panic!("not a mapping");
            };
            let entry = parser.document.entries(mapping)[0];
            let value = parser.node(entry.value).unwrap();
            vec![leaf(parser, value)]
        })
    }

    fn string(field: &Field<'_, impl Text>) -> String {
        match field {
            Field::Valid { value, .. } => format!("valid {:?}", value.text()),
            Field::Invalid { .. } => "invalid".to_owned(),
            Field::Absent => "absent".to_owned(),
        }
    }

    fn strings(field: &Field<'_, Arc<[impl Text]>>) -> String {
        match field {
            Field::Valid { value, .. } => {
                let values: Vec<_> = value.iter().map(Text::text).collect();
                format!("valid {values:?}")
            }
            Field::Invalid { .. } => "invalid".to_owned(),
            Field::Absent => "absent".to_owned(),
        }
    }

    fn value<T: std::fmt::Debug>(value: &Value<'_, T>) -> String {
        match value {
            Value::Literal { value, .. } => format!("literal {value:?}"),
            Value::Expression { expr, .. } => format!("expression {:?}", expr.source.value()),
            Value::Invalid { .. } => "invalid".to_owned(),
            Value::Absent => "absent".to_owned(),
        }
    }

    #[test]
    fn any_scalar_is_a_string_whatever_its_tag() {
        for (text, want) in [
            ("v: 42\n", "42"),
            ("v: true\n", "true"),
            ("v: ~\n", "~"),
            ("v: null\n", "null"),
            ("v: !custom x\n", "x"),
            ("v: 2026-10-15\n", "2026-10-15"),
        ] {
            assert_eq!(
                leaf(text, |p, n| string(&p.parse_string(None, n, false))),
                [format!("valid {want:?}")],
                "{text:?}"
            );
        }
    }

    #[test]
    fn an_empty_string_is_reported_unless_empty_is_allowed() {
        assert_eq!(
            leaf("v:\n", |p, n| string(&p.parse_string(None, n, false))),
            ["1:3: string should not be empty [empty-string]", "invalid"]
        );
        assert_eq!(
            leaf("v: ''\n", |p, n| string(&p.parse_string(None, n, true))),
            [r#"valid """#]
        );
    }

    #[test]
    fn a_collection_is_not_a_string() {
        assert_eq!(
            leaf("v: [a]\n", |p, n| string(&p.parse_string(None, n, true))),
            [
                r#"1:4: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
                "invalid"
            ]
        );
        assert_eq!(
            leaf("v: !!set {a}\n", |p, n| string(
                &p.parse_string(None, n, true)
            )),
            [
                r#"1:4: expected scalar node for string value but found mapping node with "!!set" tag [unexpected-node-kind]"#,
                "invalid"
            ]
        );
    }

    #[test]
    fn a_string_sequence_keeps_an_invalid_item_as_none() {
        assert_eq!(
            leaf("v: [a, '', [b], 1]\n", |p, n| strings(
                &p.parse_string_sequence(None, "branches", n, false, false)
            )),
            [
                "1:8: string should not be empty [empty-string]",
                r#"1:12: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
                r#"valid ["a", "-", "-", "1"]"#,
            ]
        );
    }

    #[test]
    fn a_string_sequence_must_be_a_sequence_with_items() {
        assert_eq!(
            leaf("v: a\n", |p, n| strings(
                &p.parse_string_sequence(None, "branches", n, false, false)
            )),
            [
                r#"1:4: "branches" section must be sequence node but got scalar node with "!!str" tag [unexpected-node-kind]"#,
                "invalid"
            ]
        );
        assert_eq!(
            leaf("v: []\n", |p, n| strings(
                &p.parse_string_sequence(None, "branches", n, false, false)
            )),
            [
                r#"1:4: "branches" section should not be empty [empty-section]"#,
                "invalid"
            ]
        );
        assert_eq!(
            leaf("v: []\n", |p, n| strings(
                &p.parse_string_sequence(None, "branches", n, true, false)
            )),
            ["valid []"]
        );
    }

    #[test]
    fn a_string_or_sequence_takes_a_scalar_as_one_item() {
        let parse = |text| {
            leaf(text, |p, n| {
                strings(&p.parse_string_or_string_sequence(None, "needs", n, true, false))
            })
        };
        assert_eq!(parse("v: a\n"), [r#"valid ["a"]"#]);
        assert_eq!(parse("v: [a, b]\n"), [r#"valid ["a", "b"]"#]);
        // A null scalar is an empty list only when empty is allowed.
        assert_eq!(parse("v:\n"), ["valid []"]);
        // Otherwise it is a list of one invalid item, as `""` is.
        for (text, column) in [("v:\n", 3), ("v: ''\n", 4)] {
            assert_eq!(
                leaf(text, |p, n| strings(&p.parse_string_or_string_sequence(
                    None, "needs", n, false, false
                ))),
                [
                    format!("1:{column}: string should not be empty [empty-string]"),
                    r#"valid ["-"]"#.to_owned()
                ],
                "{text:?}"
            );
        }
        assert_eq!(
            leaf("v: {a: b}\n", |p, n| strings(
                &p.parse_string_or_string_sequence(None, "needs", n, true, false)
            )),
            [
                r#"1:4: "needs" section must be sequence node but got mapping node with "!!map" tag [unexpected-node-kind]"#,
                "invalid"
            ]
        );
    }

    #[test]
    fn a_bool_is_a_bool_literal_or_an_expression() {
        let parse = |text| leaf(text, |p, n| value(&p.parse_bool(None, n)));
        for (text, want) in [
            ("v: true\n", "literal true"),
            ("v: True\n", "literal true"),
            ("v: TRUE\n", "literal true"),
            ("v: false\n", "literal false"),
            ("v: FALSE\n", "literal false"),
            ("v: !!bool yes\n", "literal false"),
            ("v: ${{ inputs.x }}\n", r#"expression "${{ inputs.x }}""#),
            (
                "v: ' ${{ inputs.x }}'\n",
                r#"expression " ${{ inputs.x }}""#,
            ),
        ] {
            assert_eq!(parse(text), [want], "{text:?}");
        }
        assert_eq!(
            parse("v: yes\n"),
            [
                r#"1:4: expecting a single ${{...}} expression or boolean literal "true" or "false", but found plain text node [expected-single-expression]"#,
                "invalid"
            ]
        );
        assert_eq!(
            parse("v: 1\n"),
            [
                r#"1:4: expected bool value but found scalar node with "!!int" tag [invalid-literal-value]"#,
                "invalid"
            ]
        );
        assert_eq!(
            parse("v: [true]\n"),
            [
                r#"1:4: expected bool value but found sequence node with "!!seq" tag [invalid-literal-value]"#,
                "invalid"
            ]
        );
    }

    #[test]
    fn an_int_is_an_int_literal_or_an_expression() {
        let parse = |text| leaf(text, |p, n| value(&p.parse_int(None, n)));
        assert_eq!(parse("v: 10\n"), ["literal 10"]);
        assert_eq!(parse("v: -0\n"), ["literal 0"]);
        assert_eq!(
            parse("v: ${{ inputs.n }}\n"),
            [r#"expression "${{ inputs.n }}""#]
        );
        assert_eq!(
            parse("v: '10'\n"),
            [
                "1:4: expecting a single ${{...}} expression or integer literal, but found plain text node [expected-single-expression]",
                "invalid"
            ]
        );
        assert_eq!(
            parse("v: 1.5\n"),
            [
                r#"1:4: expected scalar node for integer value but found scalar node with "!!float" tag [invalid-literal-value]"#,
                "invalid"
            ]
        );
        assert_eq!(
            parse("v: 0x10\n"),
            [
                r#"1:4: invalid integer value: "0x10": strconv.Atoi: parsing "0x10": invalid syntax [invalid-literal-value]"#,
                "invalid"
            ]
        );
        assert_eq!(
            parse("v: !!int 1.5\n"),
            [
                r#"1:4: invalid integer value: "1.5": strconv.Atoi: parsing "1.5": invalid syntax [invalid-literal-value]"#,
                "invalid"
            ]
        );
        assert_eq!(
            parse("v: 99999999999999999999\n"),
            [
                r#"1:4: invalid integer value: "99999999999999999999": strconv.Atoi: parsing "99999999999999999999": value out of range [invalid-literal-value]"#,
                "invalid"
            ]
        );
    }

    #[test]
    fn a_float_is_a_float_or_int_literal_or_an_expression() {
        let parse = |text| leaf(text, |p, n| value(&p.parse_float(None, n)));
        assert_eq!(parse("v: 1.5\n"), ["literal 1.5"]);
        assert_eq!(parse("v: 10\n"), ["literal 10.0"]);
        assert_eq!(parse("v: !!float 0x1p4\n"), ["literal 16.0"]);
        assert_eq!(
            parse("v: ${{ inputs.t }}\n"),
            [r#"expression "${{ inputs.t }}""#]
        );
        assert_eq!(
            parse("v: true\n"),
            [
                r#"1:4: expected scalar node for float value but found scalar node with "!!bool" tag [invalid-literal-value]"#,
                "invalid"
            ]
        );
        assert_eq!(
            parse("v: .inf\n"),
            [
                r#"1:4: invalid float value: ".inf": strconv.ParseFloat: parsing ".inf": invalid syntax [invalid-literal-value]"#,
                "invalid"
            ]
        );
        assert_eq!(
            parse("v: 1e400\n"),
            [
                r#"1:4: invalid float value: "1e400": strconv.ParseFloat: parsing "1e400": value out of range [invalid-literal-value]"#,
                "invalid"
            ]
        );
        assert_eq!(
            parse("v: x\n"),
            [
                "1:4: expecting a single ${{...}} expression or float number literal, but found plain text node [expected-single-expression]",
                "invalid"
            ]
        );
    }

    #[test]
    fn nan_is_an_invalid_float_instead_of_a_crash() {
        let parse = |text| leaf(text, |p, n| value(&p.parse_float(None, n)));
        assert_eq!(
            parse("v: !!float NaN\n"),
            [
                r#"1:4: invalid float value: "NaN": strconv.ParseFloat: parsing "NaN": invalid syntax [invalid-literal-value]"#,
                "invalid"
            ]
        );
        assert_eq!(
            parse("v: !!int nan\n"),
            [
                r#"1:4: invalid float value: "nan": strconv.ParseFloat: parsing "nan": invalid syntax [invalid-literal-value]"#,
                "invalid"
            ]
        );
    }

    #[test]
    fn parse_expression_ignores_the_tag() {
        let parse = |text| {
            leaf(text, |p, n| {
                format!("{:?}", p.parse_expression(n, "x").map(|e| e.source.value()))
            })
        };
        assert_eq!(parse("v: !!int ${{ a }}\n"), [r#"Some("${{ a }}")"#]);
        assert_eq!(
            parse("v: {a: b}\n"),
            [
                "1:4: expecting a single ${{...}} expression or x, but found plain text node [expected-single-expression]",
                "None"
            ]
        );
    }

    #[test]
    fn may_parse_expression_needs_a_str_tag_and_reports_nothing() {
        let parse = |text| {
            leaf(text, |p, n| {
                format!("{:?}", p.may_parse_expression(n).map(|e| e.source.value()))
            })
        };
        assert_eq!(parse("v: ${{ a }}\n"), [r#"Some("${{ a }}")"#]);
        assert_eq!(parse("v: '${{ a }}'\n"), [r#"Some("${{ a }}")"#]);
        assert_eq!(parse("v: !custom ${{ a }}\n"), ["None"]);
        assert_eq!(parse("v: a ${{ b }}\n"), ["None"]);
        assert_eq!(parse("v: [x]\n"), ["None"]);
    }

    /// Parses the value of `v` in `text` with `parse` and reports the
    /// errors of its placeholders. Returns where each diagnostic is and its
    /// rule, without the message, followed by the number of placeholders.
    fn positions(
        text: &str,
        parse: impl for<'a, 'f> Fn(&mut Parser<'a, 'f>, &'a Node) -> Field<'a, Template<'a>>,
    ) -> Vec<String> {
        leaf(text, |p, n| {
            let field = parse(p, n);
            let placeholders = field.value().map_or(&[][..], Template::placeholders);
            for placeholder in placeholders {
                p.report_placeholder(Str::new(n).unwrap(), placeholder);
            }
            format!("{} placeholders", placeholders.len())
        })
        .into_iter()
        .map(
            |line| match (line.split_once(": "), line.rsplit_once(' ')) {
                (Some((pos, _)), Some((_, rule))) if rule.starts_with('[') => {
                    format!("{pos} {rule}")
                }
                _ => line,
            },
        )
        .collect()
    }

    fn template_positions(text: &str) -> Vec<String> {
        positions(text, |p, n| p.parse_template(None, n, true))
    }

    #[test]
    fn a_string_without_placeholders_reports_nothing() {
        assert_eq!(template_positions("v: echo hello\n"), ["0 placeholders"]);
    }

    #[test]
    fn an_error_in_a_plain_scalar_is_at_its_character() {
        // `b` is the token the parser did not expect.
        assert_eq!(
            template_positions("v: echo ${{ a b }}\n"),
            ["1:15 [invalid-expression-syntax]", "1 placeholders"]
        );
    }

    #[test]
    fn every_placeholder_with_an_error_is_reported() {
        assert_eq!(
            template_positions("v: ${{ a b }} ${{ ok }} ${{ c d }}\n"),
            [
                "1:10 [invalid-expression-syntax]",
                "1:31 [invalid-expression-syntax]",
                "3 placeholders"
            ]
        );
    }

    #[test]
    fn a_closing_pair_in_a_string_literal_does_not_end_the_placeholder() {
        assert_eq!(
            template_positions("v: ${{ x == '}}' }} ${{ y z }}\n"),
            ["1:27 [invalid-expression-syntax]", "2 placeholders"]
        );
    }

    #[test]
    fn an_unterminated_placeholder_is_reported_at_the_end_of_the_scalar() {
        assert_eq!(
            template_positions("v: echo ${{ github.ref\n"),
            ["1:23 [invalid-expression-syntax]", "1 placeholders"]
        );
    }

    #[test]
    fn an_error_in_a_single_quoted_scalar_counts_the_escaped_quote() {
        // The value is `it's ${{ a b }}`. The reference adds the offset of
        // `b` in the value to the column after the quote and reports
        // column 16, one short because of the `''`.
        assert_eq!(
            template_positions("v: 'it''s ${{ a b }}'\n"),
            ["1:17 [invalid-expression-syntax]", "1 placeholders"]
        );
    }

    #[test]
    fn an_error_from_an_escape_in_a_double_quoted_scalar_is_at_the_backslash() {
        // `\"` decodes to the `"` the lexer rejects.
        assert_eq!(
            template_positions("v: \"\\t${{ \\\"a\\\" }}\"\n"),
            ["1:11 [invalid-expression-syntax]", "1 placeholders"]
        );
    }

    #[test]
    fn an_error_in_a_literal_block_scalar_is_on_its_line() {
        assert_eq!(
            template_positions("v: |\n  echo hi\n  echo ${{ a b }}\n"),
            ["3:14 [invalid-expression-syntax]", "1 placeholders"]
        );
    }

    #[test]
    fn an_error_in_a_folded_block_scalar_is_on_its_line() {
        assert_eq!(
            template_positions("v: >\n  echo hi\n  ${{ a b }}\n"),
            ["3:9 [invalid-expression-syntax]", "1 placeholders"]
        );
    }

    #[test]
    fn an_error_after_non_ascii_text_counts_columns_in_characters() {
        assert_eq!(
            template_positions("v: æøå ${{ a b }}\n"),
            ["1:14 [invalid-expression-syntax]", "1 placeholders"]
        );
    }

    #[test]
    fn a_number_beyond_a_double_is_an_invalid_number_literal() {
        let number = "9".repeat(400);
        assert_eq!(
            template_positions(&format!("v: ${{{{ {number} }}}}\n")),
            ["1:8 [invalid-number-literal]", "1 placeholders"]
        );
    }

    #[test]
    fn a_condition_without_placeholders_is_one_implicit_expression() {
        // The reference ignores the quote and reports column 13 for this
        // `if` in a step. workflint reports the character after it.
        let condition = |text| positions(text, |p, n| p.parse_condition(None, n));
        assert_eq!(
            condition("v: \"foo bar\"\n"),
            ["1:9 [invalid-expression-syntax]", "1 placeholders"]
        );
        assert_eq!(
            condition("v: github.\n"),
            ["1:11 [invalid-expression-syntax]", "1 placeholders"]
        );
        assert_eq!(condition("v: a && ${{ b }}\n"), ["1 placeholders"]);
        assert_eq!(condition("v: ${{ a }} && ${{ b }}\n"), ["2 placeholders"]);
    }

    #[test]
    fn a_scalar_reached_through_aliases_is_parsed_once() {
        let lines = parse_with("a: &a ${{ a b }}\nb: *a\nc: *a\n", |p, root| {
            let NodeKind::Mapping(mapping) = &root.kind else {
                panic!("not a mapping");
            };
            let mut shared = Vec::new();
            for entry in p.document.entries(mapping).to_vec() {
                let node = p.node(entry.value).unwrap();
                let template = p.template(Str::new(node).unwrap());
                shared.push(template.placeholders().as_ptr());
            }
            vec![format!(
                "shared: {}",
                shared.windows(2).all(|w| w[0] == w[1])
            )]
        });
        assert_eq!(lines, ["shared: true"]);
    }

    #[test]
    fn a_single_expression_holds_its_parsed_placeholder() {
        assert_eq!(
            leaf("v: ${{ inputs.debug }}\n", |p, n| {
                let expr = p.parse_expression(n, "x").unwrap();
                let placeholder = expr.placeholder();
                let parsed = placeholder.expr.as_ref().unwrap();
                expr.source.value()[placeholder.to_value(parsed.range())].to_owned()
            }),
            ["inputs.debug"]
        );
        assert_eq!(
            leaf("v: ${{ a b }}\n", |p, n| format!(
                "{:?}",
                p.parse_bool(None, n)
                    .expression()
                    .map(|e| e.placeholder().expr.is_err())
            )),
            ["Some(true)"]
        );
    }
}
