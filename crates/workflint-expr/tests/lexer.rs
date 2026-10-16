//! Ports of the lexer tests in the reference's `expr_lexer_test.go`, plus the cases where
//! workflint's lexer differs or the reference has no test.
//!
//! The reference's `TestLexExprError` checks only that the message contains a fragment. These
//! tests compare whole messages, taken from the reference's lexer, and turn the error's
//! byte offset into the reference's 1-based code-point column through a line index.

#![allow(clippy::expect_used, clippy::panic)]

use workflint_diagnostics::{FileId, SourceFile};
use workflint_expr::{ExprError, TokenKind, lex_expression};

const PUNCT: &str =
    "''', '}', '(', ')', '[', ']', '.', '!', '<', '>', '=', '&', '|', '*', ',', ' '";
const ALL: &str = "'a'..'z', 'A'..'Z', '_', '0'..'9', ''', '}', '(', ')', '[', ']', '.', '!', \
                   '<', '>', '=', '&', '|', '*', ',', ' '";

/// Lexes `input` and returns the kind and text of each token before the `}}` token, and the
/// offset just past `}}`.
fn lex_ok(input: &str) -> (Vec<(TokenKind, &str)>, u32) {
    let (tokens, offset) = match lex_expression(input) {
        Ok(lexed) => lexed,
        Err(err) => panic!("lexing {input:?} failed: {err}"),
    };
    let (last, rest) = tokens.split_last().expect("at least the `}}` token");
    assert_eq!(last.kind, TokenKind::End, "{input:?}");
    assert_eq!(&input[last.range()], "}}", "{input:?}");
    let tokens = rest
        .iter()
        .map(|token| (token.kind, &input[token.range()]))
        .collect();
    (tokens, offset)
}

fn lex_err(input: &str) -> ExprError {
    match lex_expression(input) {
        Ok(tokens) => panic!("lexing {input:?} succeeded: {tokens:?}"),
        Err(err) => err,
    }
}

/// The 1-based line and code-point column of a byte offset in `text`.
fn line_col(text: &str, offset: u32) -> (usize, usize) {
    let file = SourceFile::new(FileId::new(0), "expr", text).expect("small input");
    let pos = file.line_col(offset);
    (pos.line, pos.column)
}

/// `TestLexOneToken`: each input followed by `}}` lexes to one token of the given kind.
mod one_token {
    use super::*;

    macro_rules! cases {
        ($($name:ident: $input:expr => $kind:ident,)*) => {$(
            #[test]
            fn $name() {
                let input = concat!($input, "}}");
                let (tokens, offset) = lex_ok(input);
                assert_eq!(tokens, [(TokenKind::$kind, $input)]);
                assert_eq!(offset as usize, input.len());
            }
        )*};
    }

    cases! {
        identifier: "foo" => Ident,
        identifier_with_underscore: "foo_bar" => Ident,
        identifier_with_hyphen: "foo_bar" => Ident,
        identifier_with_underscore_and_hyphen: "foo_bar-piyo" => Ident,
        underscore: "_" => Ident,
        underscore_hyphen: "_-" => Ident,
        null: "null" => Ident,
        bool: "true" => Ident,
        string: "'hello world'" => String,
        empty_string: "''" => String,
        string_with_escapes: "'''hello''world'''" => String,
        string_with_braces: "'braces {in} string {{is}} ok!'" => String,
        string_with_non_ascii_chars: "'こんにちは＼(^o^)／世界😊'" => String,
        int: "42" => Int,
        zero: "0" => Int,
        negative_int: "-42" => Int,
        negative_zero: "-0" => Int,
        hex_int: "0x1e" => Int,
        hex_int_part2: "0xf" => Int,
        hex_int_with_upper_case: "0x1E" => Int,
        hex_int_part3: "0xa" => Int,
        negative_hex_int: "-0x1e" => Int,
        hex_zero: "0x0" => Int,
        float: "1.0" => Float,
        float_smaller_than_1: "0.123" => Float,
        float_zero: "0.0" => Float,
        float_exp_part: "1.0e3" => Float,
        float_exp_part_with_upper_e: "1.0E3" => Float,
        float_negative_exp_part: "1.0e-99" => Float,
        float_negative_exp_part_with_upper_e: "1.0E-99" => Float,
        float_zero_with_negative_exp_part: "0.0e-99" => Float,
        float_zero_exp_part: "1.0e0" => Float,
        int_with_exp_part: "3e42" => Float,
        int_with_exp_part_with_big_e: "3E42" => Float,
        int_zero_with_exp_part: "0e42" => Float,
        int_with_zero_exp_part: "3e0" => Float,
        int_with_negative_exp_part: "3e-9" => Float,
        left_paren: "(" => LeftParen,
        right_paren: ")" => RightParen,
        left_bracket: "[" => LeftBracket,
        right_bracket: "]" => RightBracket,
        dot_operator: "." => Dot,
        not_operator: "!" => Not,
        less: "<" => Less,
        less_equal: "<=" => LessEq,
        greater: ">" => Greater,
        greater_equal: ">=" => GreaterEq,
        equal_operator: "==" => Eq,
        not_equal_operator: "!=" => NotEq,
        and_operator: "&&" => And,
        or_operator: "||" => Or,
        array_access: "*" => Star,
        // Not in the reference, which rejects `+` after `e` (docs/differences.md#exponent-plus-sign).
        int_with_plus_exp_part: "1e+5" => Float,
        int_with_plus_exp_part_with_big_e: "1E+5" => Float,
        float_with_plus_exp_part: "1.5e+10" => Float,
        float_with_plus_zero_exp_part: "1.0e+0" => Float,
        negative_int_with_plus_exp_part: "-2e+3" => Float,
    }
}

/// `TestLexExpression`: each input followed by `}}` lexes to these tokens.
mod expression {
    use super::*;
    use TokenKind::*;

    macro_rules! cases {
        ($($name:ident: $input:expr => [$(($kind:ident, $text:expr)),* $(,)?],)*) => {$(
            #[test]
            fn $name() {
                let input = concat!($input, "}}");
                let (tokens, offset) = lex_ok(input);
                let want: &[(TokenKind, &str)] = &[$(($kind, $text)),*];
                assert_eq!(tokens, want);
                assert_eq!(offset as usize, input.len());
            }
        )*};
    }

    cases! {
        property_dereference: "github.action_path" => [
            (Ident, "github"), (Dot, "."), (Ident, "action_path"),
        ],
        property_dereference_with_hyphen: "job.services.foo-bar.id" => [
            (Ident, "job"), (Dot, "."), (Ident, "services"), (Dot, "."),
            (Ident, "foo-bar"), (Dot, "."), (Ident, "id"),
        ],
        index_syntax: "github['sha']" => [
            (Ident, "github"), (LeftBracket, "["), (String, "'sha'"), (RightBracket, "]"),
        ],
        array_elements_dereference: "labels.*.name" => [
            (Ident, "labels"), (Dot, "."), (Star, "*"), (Dot, "."), (Ident, "name"),
        ],
        starts_with: "startsWith('hello, world', 'hello')" => [
            (Ident, "startsWith"), (LeftParen, "("), (String, "'hello, world'"),
            (Comma, ","), (String, "'hello'"), (RightParen, ")"),
        ],
        join: "join(labels.*.name, ', ')" => [
            (Ident, "join"), (LeftParen, "("), (Ident, "labels"), (Dot, "."), (Star, "*"),
            (Dot, "."), (Ident, "name"), (Comma, ","), (String, "', '"), (RightParen, ")"),
        ],
        success: "success()" => [
            (Ident, "success"), (LeftParen, "("), (RightParen, ")"),
        ],
        operator_twice: "!!success()" => [
            (Not, "!"), (Not, "!"), (Ident, "success"), (LeftParen, "("), (RightParen, ")"),
        ],
        nested_expression: "((a || b) && (c || d))" => [
            (LeftParen, "("), (LeftParen, "("), (Ident, "a"), (Or, "||"), (Ident, "b"),
            (RightParen, ")"), (And, "&&"), (LeftParen, "("), (Ident, "c"), (Or, "||"),
            (Ident, "d"), (RightParen, ")"), (RightParen, ")"),
        ],
        equal_expression: "0 == 1" => [
            (Int, "0"), (Eq, "=="), (Int, "1"),
        ],
        skip_white_spaces: " \ta .b .c\tx ( 'foo bar' ,\t42\t) [ true ]\t" => [
            (Ident, "a"), (Dot, "."), (Ident, "b"), (Dot, "."), (Ident, "c"), (Ident, "x"),
            (LeftParen, "("), (String, "'foo bar'"), (Comma, ","), (Int, "42"),
            (RightParen, ")"), (LeftBracket, "["), (Ident, "true"), (RightBracket, "]"),
        ],
        empty_expression: "" => [],
        // The cases below are not in the reference test. Their tokens come from the reference's
        // lexer.
        braces_in_a_string_do_not_end_the_expression: "'}}' " => [(String, "'}}'")],
        carriage_return_and_newline_are_white_space: "\t\r\n a" => [(Ident, "a")],
        leading_dot_lexes_as_dot_then_number: ".5" => [(Dot, "."), (Int, "5")],
        underscore_after_number_starts_an_identifier: "1_x" => [(Int, "1"), (Ident, "_x")],
        hyphen_after_number_starts_a_negative_number: "1-2" => [(Int, "1"), (Int, "-2")],
    }

    #[test]
    fn lexing_stops_at_the_first_end_marker() {
        let (tokens, offset) = lex_ok("a }} b");
        assert_eq!(tokens, [(Ident, "a")]);
        assert_eq!(offset, 4);
    }
}

/// `TestLexExprError`: each input fails with this message at this column.
mod error {
    use super::*;

    macro_rules! cases {
        ($($name:ident: $input:expr => ($col:expr, $message:expr),)*) => {$(
            #[test]
            fn $name() {
                let input: &str = $input;
                let err = lex_err(input);
                let message: &str = &$message;
                assert_eq!(err.message, message);
                assert_eq!(line_col(input, err.start), (1, $col));
            }
        )*};
    }

    fn unexpected(what: &str, where_: &str, expected: &str) -> String {
        format!("got unexpected {what} while lexing {where_}, expecting {expected}")
    }

    cases! {
        unknown_char: "?" => (1, unexpected("character '?'", "expression", ALL)),
        unexpected_eof: "42" => (3, "unexpected EOF while lexing expression"),
        empty_string: "" => (1, "unexpected EOF while lexing expression"),
        broken_string_literal: "'foo bar" =>
            (9, unexpected("EOF", "end of string literal", "'''")),
        broken_string_literal_with_escape: "'foo bar''" =>
            (11, unexpected("EOF", "end of string literal", "'''")),
        invalid_char_after_minus: "-a" =>
            (2, unexpected("character 'a'", "integer part of number", "'0'..'9'")),
        eof_after_minus: "-" =>
            (2, unexpected("EOF", "integer part of number", "'0'..'9'")),
        invalid_char_after_0: "0d" =>
            (2, unexpected("character 'd'", "character following number 0", PUNCT)),
        invalid_char_after_number: "123d" =>
            (4, unexpected("character 'd'", "character following number 123", PUNCT)),
        invalid_char_in_fraction_part_of_float: "1.e1" =>
            (3, unexpected("character 'e'", "fraction part of float number", "'0'..'9'")),
        invalid_char_in_exponent_part_of_float: "1.0e_" =>
            (5, unexpected("character '_'", "exponent part of float number", "'0'..'9'")),
        no_number_after_minus_in_exponent_part: "1.0e-" =>
            (6, unexpected("EOF", "exponent part of float number", "'0'..'9'")),
        invalid_char_in_hex_int: "0xg" =>
            (3, unexpected("character 'g'", "hex integer", "'0'..'9', 'a'..'f', 'A'..'F'")),
        unexpected_eof_after_0x: "0x" =>
            (3, unexpected("EOF", "hex integer", "'0'..'9', 'a'..'f', 'A'..'F'")),
        invalid_char_at_end_of_input: "'in {string} it {{is}} ok'}_" =>
            (28, unexpected("character '_'", "end marker }}", "'}'")),
        invalid_char_after_eq: "=3" =>
            (2, unexpected("character '3'", "== operator", "'='")),
        invalid_char_after_amp: "&3" =>
            (2, unexpected("character '3'", "&& operator", "'&'")),
        invalid_char_after_pipe: "|3" =>
            (2, unexpected("character '3'", "|| operator", "'|'")),
        unexpected_eof_while_lexing_int: "0x" =>
            (3, unexpected("EOF", "hex integer", "'0'..'9', 'a'..'f', 'A'..'F'")),
        unexpected_eof_while_lexing_fraction_of_float: "0." =>
            (3, unexpected("EOF", "fraction part of float number", "'0'..'9'")),
        unexpected_eof_while_lexing_exponent_of_float: "0.1e" =>
            (5, unexpected("EOF", "exponent part of float number", "'0'..'9'")),
        unexpected_eof_while_lexing_end_marker: "}" =>
            (2, unexpected("EOF", "end marker }}", "'}'")),
        unexpected_eof_while_lexing_eq_operator: "=" =>
            (2, unexpected("EOF", "== operator", "'='")),
        unexpected_eof_while_lexing_and_operator: "&" =>
            (2, unexpected("EOF", "&& operator", "'&'")),
        unexpected_eof_while_lexing_or_operator: "|" =>
            (2, unexpected("EOF", "|| operator", "'|'")),
        empty_string_again: "" => (1, "unexpected EOF while lexing expression"),
        special_note_for_string_literals_with_double_quotes: "\"hello\"" => (1, format!(
            "{}. do you mean string literals? only single quotes are available for string delimiter",
            unexpected("character '\"'", "expression", ALL),
        )),
        integer_starts_with_zero: "0123" =>
            (2, unexpected("character '1'", "character following number 0", PUNCT)),
        hex_integer_starts_with_zero: "0x0123" =>
            (4, unexpected("character '1'", "character following hex integer 0x0", PUNCT)),
        hex_integer_starts_with_zero_and_followed_by_e: "0x0e1" =>
            (4, unexpected("character 'e'", "character following hex integer 0x0", PUNCT)),
        integer_exponent_part_starts_with_zero: "1e01" =>
            (4, unexpected("character '1'", "character following number 1e0", PUNCT)),
        float_number_exponent_part_starts_with_zero: "1.0e01" =>
            (6, unexpected("character '1'", "character following number 1.0e0", PUNCT)),
        integer_part_lacks_in_float_number: "-.0" =>
            (2, unexpected("character '.'", "integer part of number", "'0'..'9'")),
        fraction_part_lacks_in_float_number: "0.e1" =>
            (3, unexpected("character 'e'", "fraction part of float number", "'0'..'9'")),
        invalid_char_after_hex_number: "0x1fz" =>
            (5, unexpected("character 'z'", "character following hex integer 0x1f", PUNCT)),
        // The cases below are not in the reference test. Their messages and columns come from
        // the reference's lexer, except where a comment says otherwise.
        upper_case_hex_prefix: "0X1}}" =>
            (2, unexpected("character 'X'", "character following number 0", PUNCT)),
        space_after_minus: "- 1}}" =>
            (2, unexpected("character ' '", "integer part of number", "'0'..'9'")),
        unclosed_string_after_escaped_quote: "'''" =>
            (4, unexpected("EOF", "end of string literal", "'''")),
        backslash_is_quoted_with_an_escape: "\\" =>
            (1, unexpected(r"character '\\'", "expression", ALL)),
        non_ascii_letter_is_kept_in_the_message: "あ" =>
            (1, unexpected("character 'あ'", "expression", ALL)),
        non_printable_char_is_escaped: "\u{a0}" =>
            (1, unexpected("character '\\u00a0'", "expression", ALL)),
        nul: "a\0}}" => (2, "scan error while lexing expression: invalid character NUL"),
        nul_at_start: "\0}}" => (1, "scan error while lexing expression: invalid character NUL"),
        nul_in_string: "'a\0b'}}" =>
            (3, "scan error while lexing expression: invalid character NUL"),
        nul_after_end_marker: "a}}\0" =>
            (4, "scan error while lexing expression: invalid character NUL"),
        nul_after_incomplete_operator: "=\0" =>
            (2, "scan error while lexing expression: invalid character NUL"),
        // workflint accepts `+` after `e` (docs/differences.md#exponent-plus-sign), so these
        // fail after the sign. The reference reports the `+` at column 3.
        eof_after_plus_in_exponent: "1e+" =>
            (4, unexpected("EOF", "exponent part of float number", "'0'..'9'")),
        end_marker_after_plus_in_exponent: "1e+}}" =>
            (4, unexpected("character '}'", "exponent part of float number", "'0'..'9'")),
        two_signs_in_exponent: "1e+-5}}" =>
            (4, unexpected("character '-'", "exponent part of float number", "'0'..'9'")),
        two_plus_signs_in_exponent: "1e++5}}" =>
            (4, unexpected("character '+'", "exponent part of float number", "'0'..'9'")),
    }

    #[test]
    fn error_range_covers_the_unexpected_character() {
        let input = "'あ' == 'い' || ?}}";
        let err = lex_err(input);
        assert_eq!(&input[err.range()], "?");
        let err = lex_err("1 == あ");
        assert_eq!((err.start, err.end), (5, 8));
    }

    #[test]
    fn error_range_at_end_of_input_is_empty() {
        let err = lex_err("'foo");
        assert_eq!((err.start, err.end), (4, 4));
    }

    #[test]
    fn error_display_is_the_message() {
        let err = lex_err("?");
        assert_eq!(err.to_string(), err.message);
    }
}

/// `TestLexTokenPos`: token offsets, and columns derived from them.
mod token_pos {
    use super::*;

    #[test]
    fn token_offsets_match_the_reference() {
        let input = "foo( true && 0.1234, github.issue )}}";
        let want = [0, 3, 5, 10, 13, 19, 21, 27, 28, 34, 35];
        let (tokens, _) = lex_expression(input).expect("valid expression");
        let starts: Vec<u32> = tokens.iter().map(|token| token.start).collect();
        assert_eq!(starts, want);
        for token in &tokens {
            assert_eq!(line_col(input, token.start), (1, token.start as usize + 1));
        }
    }

    // The reference reports this error at column 15 and offset 18: its columns count code
    // points and its offsets count bytes.
    #[test]
    fn error_offset_after_non_ascii_text_converts_to_a_code_point_column() {
        let input = "'あ' == 'い' || ?}}";
        let err = lex_err(input);
        assert_eq!(err.start, 18);
        assert_eq!(line_col(input, err.start), (1, 15));
    }

    // The reference reports this error at line 2, column 3, offset 4.
    #[test]
    fn error_offset_on_a_later_line_converts_to_its_line_and_column() {
        let input = "a\n  ?";
        let err = lex_err(input);
        assert_eq!(err.start, 4);
        assert_eq!(line_col(input, err.start), (2, 3));
    }

    // The reference's scanner skips a byte order mark at the start of its input but keeps it in
    // the first token's text.
    #[test]
    fn leading_byte_order_mark_stays_in_the_first_token() {
        let input = "\u{feff}1}}";
        let (tokens, offset) = lex_ok(input);
        assert_eq!(tokens, [(TokenKind::Int, "\u{feff}1")]);
        assert_eq!(offset, 6);
        assert_eq!(lex_err("\u{feff} ?").start, 4);
        assert_eq!(lex_err("\u{feff}").start, 3);
    }
}

#[test]
fn token_kinds_print_the_reference_names() {
    use TokenKind::*;
    let names = [
        (End, "END"),
        (Ident, "IDENT"),
        (String, "STRING"),
        (Int, "INTEGER"),
        (Float, "FLOAT"),
        (LeftParen, "("),
        (RightParen, ")"),
        (LeftBracket, "["),
        (RightBracket, "]"),
        (Dot, "."),
        (Not, "!"),
        (Less, "<"),
        (LessEq, "<="),
        (Greater, ">"),
        (GreaterEq, ">="),
        (Eq, "=="),
        (NotEq, "!="),
        (And, "&&"),
        (Or, "||"),
        (Star, "*"),
        (Comma, ","),
    ];
    for (kind, name) in names {
        assert_eq!(kind.as_str(), name);
        assert_eq!(kind.to_string(), name);
    }
}

#[test]
fn lexer_returns_end_tokens_after_an_error() {
    let mut lexer = workflint_expr::Lexer::new("a ? b}}");
    assert_eq!(lexer.next_token().kind, TokenKind::Ident);
    let token = lexer.next_token();
    assert_eq!(token.kind, TokenKind::End);
    assert_eq!(token.range(), 2..2);
    assert_eq!(lexer.next_token().kind, TokenKind::End);
    let err = lexer.error().expect("an error");
    assert_eq!(err.start, 2);
}

#[test]
fn lexer_keeps_only_the_first_error() {
    let mut lexer = workflint_expr::Lexer::new("'a\0b");
    assert_eq!(lexer.next_token().kind, TokenKind::End);
    let err = lexer.into_error().expect("an error");
    assert_eq!(
        err.message,
        "scan error while lexing expression: invalid character NUL"
    );
}

#[test]
fn lexer_offset_is_just_past_the_end_marker() {
    let mut lexer = workflint_expr::Lexer::new(" a }} rest");
    while lexer.next_token().kind != TokenKind::End {}
    assert_eq!(lexer.offset(), 5);
    assert!(lexer.error().is_none());
}

#[test]
fn invalid_expression_syntax_rule_has_the_expression_kind() {
    let rule = &workflint_expr::rules::INVALID_EXPRESSION_SYNTAX;
    assert_eq!(rule.id, "invalid-expression-syntax");
    assert_eq!(rule.kind, "expression");
    assert_eq!(
        rule.docs_url,
        "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-expression-syntax.md"
    );
}
