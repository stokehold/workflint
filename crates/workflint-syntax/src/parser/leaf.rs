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
use crate::{Expr, Field, Str, Value, is_expression_assigned, strconv};

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
    /// An item that is not a valid string is reported and left out. The
    /// reference keeps it as `""`, which no rule checks.
    pub(crate) fn parse_string_sequence(
        &mut self,
        key: Option<Str<'a>>,
        section: &str,
        node: &'a Node,
        allow_empty: bool,
        allow_item_empty: bool,
    ) -> Field<'a, Arc<[Str<'a>]>> {
        let span = node.span;
        let Some(items) = self.check_sequence(section, node, allow_empty) else {
            return Field::Invalid { key, span };
        };
        let mut strings = Vec::with_capacity(items.len());
        for &id in items {
            if let Some(item) = self.node(id)
                && let Some(string) = self.check_string(item, allow_item_empty)
            {
                strings.push(string);
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
    /// scalar is a sequence of one string.
    pub(crate) fn parse_string_or_string_sequence(
        &mut self,
        key: Option<Str<'a>>,
        section: &str,
        node: &'a Node,
        allow_empty: bool,
        allow_item_empty: bool,
    ) -> Field<'a, Arc<[Str<'a>]>> {
        let span = node.span;
        match &node.kind {
            NodeKind::Scalar(scalar) if allow_empty && scalar.tag == Tag::Null => Field::Valid {
                key,
                span,
                value: Arc::new([]),
            },
            NodeKind::Scalar(_) => match self.check_string(node, allow_item_empty) {
                Some(string) => Field::Valid {
                    key,
                    span,
                    value: Arc::new([string]),
                },
                None => Field::Invalid { key, span },
            },
            NodeKind::Sequence(_) | NodeKind::Mapping(_) | NodeKind::Alias(_) => {
                self.parse_string_sequence(key, section, node, allow_empty, allow_item_empty)
            }
        }
    }

    /// Parses `node` as a single `${{ }}` expression, and otherwise reports
    /// that it should be one or `expecting`. The reference's
    /// `parseExpression`, which does not check the tag.
    pub(crate) fn parse_expression(&mut self, node: &'a Node, expecting: &str) -> Option<Expr<'a>> {
        match Str::new(node) {
            Some(source) if is_expression_assigned(source.value()) => Some(Expr::new(source)),
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
    pub(crate) fn may_parse_expression(node: &'a Node) -> Option<Expr<'a>> {
        Str::new(node)
            .filter(|source| *source.tag() == Tag::Str && is_expression_assigned(source.value()))
            .map(Expr::new)
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;
    use crate::testing::parse_with;

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

    fn string(field: &Field<'_, Str<'_>>) -> String {
        match field {
            Field::Valid { value, .. } => format!("valid {:?}", value.value()),
            Field::Invalid { .. } => "invalid".to_owned(),
            Field::Absent => "absent".to_owned(),
        }
    }

    fn strings(field: &Field<'_, Arc<[Str<'_>]>>) -> String {
        match field {
            Field::Valid { value, .. } => {
                let values: Vec<_> = value.iter().map(Str::value).collect();
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
    fn a_string_sequence_keeps_its_valid_items() {
        assert_eq!(
            leaf("v: [a, '', [b], 1]\n", |p, n| strings(
                &p.parse_string_sequence(None, "branches", n, false, false)
            )),
            [
                "1:8: string should not be empty [empty-string]",
                r#"1:12: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
                r#"valid ["a", "1"]"#,
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
        assert_eq!(
            leaf("v:\n", |p, n| strings(&p.parse_string_or_string_sequence(
                None, "needs", n, false, false
            ))),
            ["1:3: string should not be empty [empty-string]", "invalid"]
        );
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
            leaf(text, |_, n| {
                format!(
                    "{:?}",
                    Parser::may_parse_expression(n).map(|e| e.source.value())
                )
            })
        };
        assert_eq!(parse("v: ${{ a }}\n"), [r#"Some("${{ a }}")"#]);
        assert_eq!(parse("v: '${{ a }}'\n"), [r#"Some("${{ a }}")"#]);
        assert_eq!(parse("v: !custom ${{ a }}\n"), ["None"]);
        assert_eq!(parse("v: a ${{ b }}\n"), ["None"]);
        assert_eq!(parse("v: [x]\n"), ["None"]);
    }
}
