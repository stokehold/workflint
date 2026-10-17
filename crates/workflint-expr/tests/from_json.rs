//! Tests of `fromJSON()` with a string literal: the type of the JSON value, and the errors of
//! broken JSON.
//!
//! `type_of_json_value` ports the reference's `TestExprTypeTypeOfJSONValue` through
//! `fromJSON()`, and `nesting` replaces `TestExprTypePanicTypeOfJSONValue`. The expected
//! offsets and messages of broken JSON are the ones Go 1.26.4's `json.Unmarshal` returns for
//! the same input, as the reference calls it.

#![allow(clippy::panic)]

use workflint_expr::rules::{INVALID_JSON_LITERAL, UNDEFINED_PROPERTY};
use workflint_expr::{Expr, Ty, TypeEnv, parse_expression};

/// Parses `input` followed by `}}`.
fn parse(input: &str) -> Expr {
    let src = format!("{input}}}}}");
    match parse_expression(&src) {
        Ok((expr, _)) => expr,
        Err(err) => panic!("parsing {input:?} failed at {}: {err}", err.start),
    }
}

/// The call `fromJSON('<json>')`, with each `'` in `json` escaped.
fn from_json(json: &str) -> String {
    format!("fromJSON('{}')", json.replace('\'', "''"))
}

/// Checks `input` and returns its type. Fails if the check reports an error.
fn ty(input: &str) -> Ty {
    let (ty, errors) = TypeEnv::new().check(&parse(input));
    assert!(errors.is_empty(), "checking {input:?} failed: {errors:?}");
    ty
}

/// The type of `fromJSON('<json>')`.
fn json_ty(json: &str) -> Ty {
    ty(&from_json(json))
}

/// Checks `input` and returns its type and its errors as the rule ID, the byte range, and
/// the message.
fn check(input: &str) -> (Ty, Vec<(&'static str, u32, u32, String)>) {
    let (ty, errors) = TypeEnv::new().check(&parse(input));
    let errors = errors
        .into_iter()
        .map(|err| (err.rule.id, err.start, err.end, err.message))
        .collect();
    (ty, errors)
}

/// The byte offset of the end of `input`.
fn end_of(input: &str) -> u32 {
    u32::try_from(input.len()).unwrap_or(u32::MAX)
}

/// Asserts that `fromJSON('<json>')` reports Go's syntax error `message` at `offset`, at the
/// literal, and has type `any`.
fn assert_broken(json: &str, offset: usize, message: &str) {
    let input = from_json(json);
    let (ty, errors) = check(&input);
    // The literal ends before the closing parenthesis.
    let end = end_of(&input) - 1;
    assert_eq!(
        errors,
        [(
            INVALID_JSON_LITERAL.id,
            9,
            end,
            format!("broken JSON string is passed to fromJSON() at offset {offset}: {message}")
        )],
        "{json:?}"
    );
    assert_eq!(ty, Ty::Any, "{json:?}");
}

/// `TestExprTypeTypeOfJSONValue`.
mod type_of_json_value {
    use super::*;

    #[test]
    fn null() {
        assert_eq!(json_ty("null"), Ty::Null);
    }

    #[test]
    fn num() {
        assert_eq!(json_ty("1.0"), Ty::Number);
    }

    #[test]
    fn string() {
        assert_eq!(json_ty(r#""hello""#), Ty::String);
    }

    #[test]
    fn bool() {
        assert_eq!(json_ty("true"), Ty::Bool);
    }

    #[test]
    fn empty_array() {
        assert_eq!(json_ty("[]"), Ty::array(Ty::Any));
    }

    #[test]
    fn array() {
        assert_eq!(json_ty(r#"["hello", "world"]"#), Ty::array(Ty::String));
    }

    #[test]
    fn nested_array() {
        assert_eq!(
            json_ty(r#"[[[["hi"]]]]"#),
            Ty::array(Ty::array(Ty::array(Ty::array(Ty::String))))
        );
    }

    #[test]
    fn merged_array_element() {
        assert_eq!(json_ty(r#"["hello", 1.0, true]"#), Ty::array(Ty::String));
    }

    #[test]
    fn recursively_merged_array_element() {
        assert_eq!(
            json_ty(r#"[["hello"], [1.0], [true]]"#),
            Ty::array(Ty::array(Ty::String))
        );
    }

    #[test]
    fn array_element_fallback_to_any() {
        assert_eq!(json_ty(r#"["hello", null]"#), Ty::array(Ty::Any));
    }

    #[test]
    fn empty_object() {
        assert_eq!(json_ty("{}"), Ty::empty_strict());
    }

    #[test]
    fn object() {
        assert_eq!(
            json_ty(r#"{"hello": 1.0, "world": true}"#),
            Ty::strict([("hello", Ty::Number), ("world", Ty::Bool)])
        );
    }

    #[test]
    fn nested_object() {
        assert_eq!(
            json_ty(r#"{"hello": [1.0], "world": {"foo": true, "bar": "x"}}"#),
            Ty::strict([
                ("hello", Ty::array(Ty::Number)),
                (
                    "world",
                    Ty::strict([("foo", Ty::Bool), ("bar", Ty::String)])
                ),
            ])
        );
    }
}

/// Values that Go decodes differently from the obvious reading.
mod values {
    use super::*;

    #[test]
    fn duplicate_key_keeps_its_last_value() {
        assert_eq!(
            json_ty(r#"{"a": 1, "a": "x"}"#),
            Ty::strict([("a", Ty::String)])
        );
    }

    #[test]
    fn escaped_key_is_decoded() {
        assert_eq!(
            json_ty(r#"{"a\"bé😀\ud800": 1}"#),
            Ty::strict([("a\"b\u{e9}\u{1f600}\u{fffd}", Ty::Number)])
        );
    }

    #[test]
    fn whitespace_around_values_is_skipped() {
        assert_eq!(
            json_ty(" \t\r\n[ 1 , {\"a\" : [ ] } ]\n"),
            Ty::array(Ty::Any)
        );
    }

    /// Go's decoder fails with an `UnmarshalTypeError` that is not a syntax error, so the
    /// reference reports nothing and the call has its declared type.
    #[test]
    fn number_beyond_the_range_of_a_double_gives_any() {
        assert_eq!(json_ty("1e400"), Ty::Any);
        assert_eq!(json_ty(r#"{"a": [1, -1e400]}"#), Ty::Any);
    }

    #[test]
    fn number_that_underflows_is_a_number() {
        assert_eq!(json_ty("1e-400"), Ty::Number);
    }

    #[test]
    fn argument_that_is_not_a_literal_gives_any() {
        assert_eq!(ty("fromJSON(github.event_name)"), Ty::Any);
        assert_eq!(ty("fromJSON(format('{0}', '[1]'))"), Ty::Any);
    }

    /// From the fixture `err/invalid_json_in_fromjson`.
    #[test]
    fn fixture_values() {
        assert_eq!(json_ty(r#"["foo", 1.2]"#), Ty::array(Ty::String));
        assert_eq!(
            json_ty(r#"{"bool":true,"array":[false]}"#),
            Ty::strict([("array", Ty::array(Ty::Bool)), ("bool", Ty::Bool)])
        );
    }
}

/// `docs/differences.md#fromjson-key-case`.
mod key_case {
    use super::*;

    #[test]
    fn dot_access_matches_a_key_without_regard_to_case() {
        assert_eq!(ty(r#"fromJSON('{"Foo":1}').foo"#), Ty::Number);
        assert_eq!(ty(r#"fromJSON('{"Foo":1}').Foo"#), Ty::Number);
        assert_eq!(ty(r#"fromJSON('{"Foo":1}').FOO"#), Ty::Number);
    }

    #[test]
    fn literal_index_matches_a_key_without_regard_to_case() {
        assert_eq!(ty(r#"fromJSON('{"Foo":1}')['foo']"#), Ty::Number);
        assert_eq!(ty(r#"fromJSON('{"Ä":1}')['ä']"#), Ty::Number);
    }

    #[test]
    fn filtered_array_matches_a_key_without_regard_to_case() {
        assert_eq!(
            ty(r#"fromJSON('[{"Foo":1}]').*.foo"#),
            Ty::filtered_array(Ty::Number)
        );
    }

    /// The exact spelling wins when two keys differ only in case. A dot access reads the key
    /// in lower case.
    #[test]
    fn exact_spelling_wins_over_other_case() {
        let json = r#"fromJSON('{"Foo":1,"foo":"x"}')"#;
        assert_eq!(ty(&format!("{json}.foo")), Ty::String);
        assert_eq!(ty(&format!("{json}.FOO")), Ty::String);
        assert_eq!(ty(&format!("{json}['Foo']")), Ty::Number);
    }

    #[test]
    fn printed_type_keeps_the_key_spelling() {
        let input = r#"fromJSON('{"Foo":1}').bar"#;
        let (ty, errors) = check(input);
        assert_eq!(ty, Ty::Any);
        assert_eq!(
            errors,
            [(
                UNDEFINED_PROPERTY.id,
                0,
                end_of(input),
                r#"property "bar" is not defined in object type {Foo: number}"#.to_owned()
            )]
        );
    }

    /// From the fixture `examples/builtin_func_special_checks`.
    #[test]
    fn missing_key_is_reported() {
        let input = r#"fromJSON('{"win":"windows-latest","linux":"ubuntul-latest"}')['mac']"#;
        let (_, errors) = check(input);
        assert_eq!(
            errors,
            [(
                UNDEFINED_PROPERTY.id,
                0,
                end_of(input),
                r#"property "mac" is not defined in object type {linux: string; win: string}"#
                    .to_owned()
            )]
        );
    }
}

/// Go's `SyntaxError` for broken JSON. The offset counts the bytes up to and including the
/// unexpected one, or all bytes at the end of the input.
mod syntax_errors {
    use super::*;

    /// From the fixtures `err/invalid_json_in_fromjson` and
    /// `examples/builtin_func_special_checks`.
    #[test]
    fn end_of_input_in_fixtures() {
        assert_broken(r#""foo"#, 4, "unexpected end of JSON input");
        assert_broken(r#"["foo""#, 6, "unexpected end of JSON input");
        assert_broken("", 0, "unexpected end of JSON input");
        assert_broken(
            r#"["main","release","dev""#,
            23,
            "unexpected end of JSON input",
        );
    }

    #[test]
    fn truncated_input() {
        assert_broken(r#"{"a":"#, 5, "unexpected end of JSON input");
        assert_broken("   ", 3, "unexpected end of JSON input");
        assert_broken(r#"{"a":"\"}"#, 9, "unexpected end of JSON input");
    }

    /// Go feeds a space to the scanner at the end of the input, so a literal that the end
    /// cuts short reports that space.
    #[test]
    fn truncated_literal_reports_a_space() {
        assert_broken(
            "tru",
            3,
            "invalid character ' ' in literal true (expecting 'e')",
        );
        assert_broken(
            "fals",
            4,
            "invalid character ' ' in literal false (expecting 'e')",
        );
        assert_broken(
            "nul",
            3,
            "invalid character ' ' in literal null (expecting 'l')",
        );
        assert_broken("-", 1, "invalid character ' ' in numeric literal");
        assert_broken(
            "1.",
            2,
            "invalid character ' ' after decimal point in numeric literal",
        );
        assert_broken(
            "1e",
            2,
            "invalid character ' ' in exponent of numeric literal",
        );
        assert_broken(
            "1e+",
            3,
            "invalid character ' ' in exponent of numeric literal",
        );
    }

    #[test]
    fn control_character_in_a_string() {
        assert_broken(
            "\"a\u{1}\"",
            3,
            r"invalid character '\x01' in string literal",
        );
        assert_broken(
            "\"a\u{1f}\"",
            3,
            r"invalid character '\x1f' in string literal",
        );
        assert_broken("\"a\nb\"", 3, r"invalid character '\n' in string literal");
        assert_broken("\"a\tb\"", 3, r"invalid character '\t' in string literal");
    }

    #[test]
    fn trailing_comma() {
        assert_broken(
            "[1,]",
            4,
            "invalid character ']' looking for beginning of value",
        );
        assert_broken(
            r#"{"a":1,}"#,
            8,
            "invalid character '}' looking for beginning of object key string",
        );
    }

    /// The offset counts bytes, not characters.
    #[test]
    fn non_ascii_text_before_the_error() {
        assert_broken(r#"["é" 1]"#, 7, "invalid character '1' after array element");
        assert_broken(
            r#"{"日本": tru}"#,
            15,
            "invalid character '}' in literal true (expecting 'e')",
        );
        assert_broken(r#""é" x"#, 6, "invalid character 'x' after top-level value");
    }

    /// Go quotes the first byte of the character as if it were a code point, so `é`, which is
    /// `C3 A9` in UTF-8, prints as `Ã`.
    #[test]
    fn non_ascii_character_at_the_error() {
        assert_broken(
            "é",
            1,
            "invalid character 'Ã' looking for beginning of value",
        );
    }

    #[test]
    fn after_top_level_value() {
        assert_broken("{} x", 4, "invalid character 'x' after top-level value");
        assert_broken("1 2", 3, "invalid character '2' after top-level value");
        assert_broken("01", 2, "invalid character '1' after top-level value");
        assert_broken("truex", 5, "invalid character 'x' after top-level value");
        assert_broken("[1]]", 4, "invalid character ']' after top-level value");
    }

    #[test]
    fn objects() {
        assert_broken(r#"{"a" 1}"#, 6, "invalid character '1' after object key");
        assert_broken(
            r#"{"a":1 "b":2}"#,
            8,
            r#"invalid character '"' after object key:value pair"#,
        );
        assert_broken(
            "{1:2}",
            2,
            "invalid character '1' looking for beginning of object key string",
        );
    }

    #[test]
    fn arrays() {
        assert_broken("[1 2]", 4, "invalid character '2' after array element");
    }

    #[test]
    fn string_escapes() {
        assert_broken(r#""\q""#, 3, "invalid character 'q' in string escape code");
        assert_broken(
            r#""\u12g4""#,
            6,
            r"invalid character 'g' in \u hexadecimal character escape",
        );
    }

    #[test]
    fn numbers() {
        assert_broken("-a", 2, "invalid character 'a' in numeric literal");
        assert_broken(
            "1.e",
            3,
            "invalid character 'e' after decimal point in numeric literal",
        );
        assert_broken(
            "1ex",
            3,
            "invalid character 'x' in exponent of numeric literal",
        );
    }

    #[test]
    fn literals() {
        assert_broken(
            "tx",
            2,
            "invalid character 'x' in literal true (expecting 'r')",
        );
        assert_broken(
            "trx",
            3,
            "invalid character 'x' in literal true (expecting 'u')",
        );
        assert_broken(
            "fx",
            2,
            "invalid character 'x' in literal false (expecting 'a')",
        );
        assert_broken(
            "nx",
            2,
            "invalid character 'x' in literal null (expecting 'u')",
        );
    }

    #[test]
    fn character_quoting() {
        assert_broken(
            "'a'",
            1,
            r"invalid character '\'' looking for beginning of value",
        );
        assert_broken(
            r"\",
            1,
            r"invalid character '\\' looking for beginning of value",
        );
        assert_broken(
            "\u{7f}",
            1,
            r"invalid character '\x7f' looking for beginning of value",
        );
        assert_broken(
            "@",
            1,
            "invalid character '@' looking for beginning of value",
        );
    }

    /// From the fixture `examples/builtin_func_special_checks`: the error is at the literal,
    /// and the call has type `any`, which `contains()` accepts.
    #[test]
    fn error_is_at_the_literal() {
        let (ty, errors) =
            check(r#"contains(fromJson('["main","release","dev"'), github.ref_name)"#);
        assert_eq!(ty, Ty::Bool);
        assert_eq!(
            errors,
            [(
                INVALID_JSON_LITERAL.id,
                18,
                43,
                "broken JSON string is passed to fromJSON() at offset 23: unexpected end of JSON input"
                    .to_owned()
            )]
        );
    }
}

/// Go stops at 10000 levels of nesting. The reference's `TestExprTypePanicTypeOfJSONValue`
/// checks a value that JSON cannot produce, which has no equivalent here. These check instead
/// that deep nesting neither panics nor overflows the stack.
mod nesting {
    use super::*;

    #[test]
    fn nesting_beyond_go_limit_is_an_error() {
        let json = "[".repeat(10_001);
        assert_broken(&json, 10_001, "invalid character '[' exceeded max depth");
        let json = r#"{"a":"#.repeat(10_001);
        assert_broken(&json, 50_001, "invalid character '{' exceeded max depth");
    }

    /// Values nested deeper than 128 levels have type `any`
    /// (`docs/differences.md#fromjson-nesting`).
    #[test]
    fn deep_nesting_within_go_limit_is_typed_up_to_128_levels() {
        let json = format!("{}{}", "[".repeat(10_000), "]".repeat(10_000));
        let mut want = Ty::array(Ty::Any);
        for _ in 1..128 {
            want = Ty::array(want);
        }
        assert_eq!(json_ty(&json), want);
    }

    /// The type stops at 128 levels, but Go still decodes the numbers below them.
    #[test]
    fn number_beyond_a_double_below_the_typed_levels_gives_any() {
        let json = format!("{}1e400{}", "[".repeat(150), "]".repeat(150));
        assert_eq!(json_ty(&json), Ty::Any);
        let json = format!(
            r#"{}{{"a": [true, "1e400", -1e400]}}{}"#,
            "[".repeat(150),
            "]".repeat(150)
        );
        assert_eq!(json_ty(&json), Ty::Any);
        let json = format!(
            r#"{}{{"1e400": [-1.5e300]}}{}"#,
            "[".repeat(150),
            "]".repeat(150)
        );
        assert_ne!(json_ty(&json), Ty::Any);
    }
}
