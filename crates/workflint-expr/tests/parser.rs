//! Ports of the parser tests in the reference's `expr_parser_test.go`, plus tests for operator
//! precedence, number literals, case folding, and the visitor.
//!
//! The reference's `TestParseExpressionSyntaxError` checks only that the message contains a
//! fragment. These tests compare whole messages and offsets, taken from the reference's parser.
//! Trees are compared as S-expressions: `(op operand...)` for operators, `(. receiver name)`
//! for property access, `(.* receiver)` for object filters, `([] operand index)` for index
//! access, and `(callee() arg...)` for function calls. Keywords print as `#null`, `#true`, and
//! `#false`, and float literals with an `f:` prefix. A chain of `&&` or `||` is one node with
//! all its operands, and prints nested to the right as the reference's tree is: `a && b && c`
//! prints as `(&& a (&& b c))`.

#![allow(clippy::expect_used, clippy::panic)]

use workflint_expr::rules::{INVALID_EXPRESSION_SYNTAX, INVALID_NUMBER_LITERAL};
use workflint_expr::{CompareOp, Expr, ExprError, ExprKind, LogicalOp, Visit, parse_expression};

/// Parses `input` followed by `}}`.
fn parse(input: &str) -> Expr {
    let src = format!("{input}}}}}");
    match parse_expression(&src) {
        Ok((expr, _)) => expr,
        Err(err) => panic!("parsing {input:?} failed at {}: {err}", err.start),
    }
}

/// Parses `input` followed by `}}` and returns the error.
fn parse_err(input: &str) -> ExprError {
    let src = format!("{input}}}}}");
    match parse_expression(&src) {
        Ok((expr, _)) => panic!("parsing {input:?} succeeded: {}", sexpr(&expr)),
        Err(err) => err,
    }
}

/// Parses `input` followed by `}}` and returns the tree as an S-expression.
fn tree(input: &str) -> String {
    sexpr(&parse(input))
}

fn sexpr(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Variable(name) => name.as_str().to_owned(),
        ExprKind::Null => "#null".to_owned(),
        ExprKind::Bool(value) => format!("#{value}"),
        ExprKind::Int(value) => format!("{value}"),
        ExprKind::Float(value) => format!("f:{value}"),
        ExprKind::String(lit) => format!("{:?}", lit.value()),
        ExprKind::Property { receiver, name } => {
            format!("(. {} {})", sexpr(receiver), name.as_str())
        }
        ExprKind::ObjectFilter { receiver } => format!("(.* {})", sexpr(receiver)),
        ExprKind::Index { operand, index } => format!("([] {} {})", sexpr(operand), sexpr(index)),
        ExprKind::Not { operand } => format!("(! {})", sexpr(operand)),
        ExprKind::Compare { op, left, right } => {
            format!("({} {} {})", op.as_str(), sexpr(left), sexpr(right))
        }
        ExprKind::Logical { op, operands } => {
            // The reference nests a chain to the right: `a && b && c` is `a && (b && c)`.
            let mut operands = operands.iter().rev().map(sexpr);
            let last = operands.next().unwrap_or_default();
            operands.fold(last, |right, left| format!("({op} {left} {right})"))
        }
        ExprKind::Call { callee, args } => {
            let mut out = format!("({}()", callee.original());
            for arg in args {
                out.push(' ');
                out.push_str(&sexpr(arg));
            }
            out.push(')');
            out
        }
    }
}

/// `TestParseExpressionSyntaxOK`: each input parses to the given tree.
mod syntax_ok {
    use super::*;

    macro_rules! cases {
        ($($name:ident: $input:expr => $tree:expr,)*) => {$(
            #[test]
            fn $name() {
                assert_eq!(tree($input), $tree);
            }
        )*};
    }

    cases! {
        null_literal: "null" => "#null",
        boolean_literal_true: "true" => "#true",
        boolean_literal_false: "false" => "#false",
        integer_literal: "711" => "711",
        negative_integer_literal: "-10" => "-10",
        zero_integer_literal: "0" => "0",
        hex_integer_literal: "0x1f" => "31",
        negative_hex_integer_literal: "-0xaf" => "-175",
        hex_integer_zero: "0x0" => "0",
        float_literal: "1234.567" => "f:1234.567",
        float_literal_smaller_than_1: "0.567" => "f:0.567",
        float_literal_zero: "0.0" => "f:0",
        negative_float_literal: "-1234.567" => "f:-1234.567",
        float_literal_with_exponent_part: "12e3" => "f:12000",
        float_literal_with_negative_exponent_part: "-99e-1" => "f:-9.9",
        float_literal_with_fraction_and_exponent_part: "1.2e3" => "f:1200",
        float_literal_with_fraction_and_negative_exponent_part: "-0.123e-12" => "f:-0.000000000000123",
        float_zero_value_with_exponent_part: "0e3" => "f:0",
        string_literal: "'hello, world'" => r#""hello, world""#,
        empty_string_literal: "''" => r#""""#,
        string_literal_with_escapes: "'''hello''world'''" => r#""'hello'world'""#,
        string_literal_with_non_ascii_chars: "'こんにちは＼(^o^)／世界😊'" => r#""こんにちは＼(^o^)／世界😊""#,
        variable: "github" => "github",
        variable_with_uppercase_name: "GITHUB" => "github",
        func_call: "success()" => "(success())",
        func_call_with_1_argument: "fromJSON(object)" => "(fromJSON() object)",
        func_call_with_multiple_arguments: "contains('hello, world', 'o, w')" => r#"(contains() "hello, world" "o, w")"#,
        index_access: "obj['key']" => r#"([] obj "key")"#,
        index_access_with_variable: "obj[a.b]" => "([] obj (. a b))",
        less_operator: "0 < 1" => "(< 0 1)",
        not_operator: "!true" => "(! #true)",
        not_operator_twice: "!!true" => "(! (! #true))",
        less_eq_operator: "0 <= 1" => "(<= 0 1)",
        greater_operator: "0 > 1" => "(> 0 1)",
        greater_eq_operator: "0 >= 1" => "(>= 0 1)",
        eq_operator: "0 == 1" => "(== 0 1)",
        not_eq_operator: "0 != 1" => "(!= 0 1)",
        and_operator: "true && false" => "(&& #true #false)",
        or_operator: "true || false" => "(|| #true #false)",
        nested_value: "(42)" => "42",
        very_nested_value: "((((((((((((((((((42))))))))))))))))))" => "42",
        object_property_dereference: "a.b" => "(. a b)",
        object_property_dereference_with_uppercase_name: "github.EVENT" => "(. github event)",
        nested_object_property_dereference: "a.b.c.d" => "(. (. (. a b) c) d)",
        property_includes_uppercase_character: "a.doSomething" => "(. a dosomething)",
        property_includes_hyphen: "a.foo-bar" => "(. a foo-bar)",
        array_element_dereference: "a.*" => "(.* a)",
        nested_array_element_dereference: "a.*.*.*" => "(.* (.* (.* a)))",
        logical_expressions:
            "0 == 0.1 && a < b || x >= !y && true != false"
            => "(|| (&& (== 0 f:0.1) (< a b)) (&& (>= x (! y)) (!= #true #false)))",
        logical_expressions_with_nested_expressions:
            "(0 == 0.1) && (a < b || x >= !y) && (true != false)"
            => "(&& (== 0 f:0.1) (&& (|| (< a b) (>= x (! y))) (!= #true #false)))",
        logical_expressions_with_more_nested_expressions:
            "((0 == 0.1) && (a < b || x >= !y)) && (true != false)"
            => "(&& (&& (== 0 f:0.1) (|| (< a b) (>= x (! y)))) (!= #true #false))",
        nested_function_calls:
            "!contains(some.value, 'foo') && endsWith(join(x.*.y, ', '), 'bar')"
            => r#"(&& (! (contains() (. some value) "foo")) (endsWith() (join() (. (.* x) y) ", ") "bar"))"#,
        nested_function_calls_with_nested_expressions:
            "!((contains((some.value), ('foo'))) && (endsWith((join((x.*.y), (', '))), ('bar'))))"
            => r#"(! (&& (contains() (. some value) "foo") (endsWith() (join() (. (.* x) y) ", ") "bar")))"#,
        nested_dereferences:
            "contains(github.event['issue'].labels.*.name, 'bug')"
            => r#"(contains() (. (.* (. ([] (. github event) "issue") labels)) name) "bug")"#,
        nested_dereferences_with_nested_expressions:
            "(((((github.event)['issue']).labels).*).name)"
            => r#"(. (.* (. ([] (. github event) "issue") labels)) name)"#,
    }
}

/// `TestParseExpressionSyntaxError`: each input fails with the given message at the given byte
/// offset, both taken from the reference's parser.
mod syntax_error {
    use super::*;

    const PRIMARY: &str = "variable access, function call, null, bool, int, float or string. \
                           expecting \"IDENT\", \"(\", \"INTEGER\", \"FLOAT\", \"STRING\"";
    const DEREF: &str = "object property dereference like 'a.b' or array element dereference \
                         like 'a.*'. expecting \"IDENT\", \"*\"";
    const ARGS: &str = "arguments of function call. expecting \",\", \")\"";
    const BRACKET: &str = "closing bracket ']' for index access. expecting \"]\"";
    const PAREN: &str = "closing ')' of nested expression (...). expecting \")\"";

    macro_rules! cases {
        ($($name:ident: $input:expr => $offset:expr, $what:expr, $where_:expr,)*) => {$(
            #[test]
            fn $name() {
                let err = parse_err($input);
                assert_eq!(err.message, format!("unexpected {} while parsing {}", $what, $where_));
                assert_eq!(err.start, $offset);
                assert_eq!(err.rule, &INVALID_EXPRESSION_SYNTAX);
            }
        )*};
    }

    #[test]
    fn remaining_inputs() {
        let err = parse_err("42 foo bar");
        assert_eq!(
            err.message,
            "parser did not reach end of input after parsing the expression. 2 remaining \
             token(s) in the input: \"IDENT\", \"IDENT\""
        );
        assert_eq!((err.start, err.end), (3, 6));
    }

    cases! {
        missing_operand_in_or_operator: "true ||" => 7, "end of input", PRIMARY,
        missing_operand_in_and_operator: "true &&" => 7, "end of input", PRIMARY,
        missing_operand_in_less_operator: "0 <" => 3, "end of input", PRIMARY,
        missing_operand_in_less_eq_operator: "0 <=" => 4, "end of input", PRIMARY,
        missing_operand_in_greater_operator: "0 >" => 3, "end of input", PRIMARY,
        missing_operand_in_greater_eq_operator: "0 >=" => 4, "end of input", PRIMARY,
        missing_operand_in_eq_operator: "0 ==" => 4, "end of input", PRIMARY,
        missing_operand_in_not_eq_operator: "0 !=" => 4, "end of input", PRIMARY,
        missing_operand_after_not_operator: "!" => 1, "end of input", PRIMARY,
        missing_operand_after_dot_operator: "foo." => 4, "end of input", DEREF,
        ident_must_come_after_dot: "foo.42" => 4, "token \"INTEGER\"", DEREF,
        broken_index_access_part1: "foo[0" => 5, "end of input", BRACKET,
        broken_index_access_part2: "foo[" => 4, "end of input", PRIMARY,
        unexpected_closing_at_index_access: "foo[0)" => 5, "token \")\"", BRACKET,
        starting_with_invalid_token: "[" => 0, "token \"[\"", PRIMARY,
        missing_closing_paren_for_nested_expression: "(a" => 2, "end of input", PAREN,
        invalid_token_at_closing_nested_expression: "(a]" => 2, "token \"]\"", PAREN,
        unexpected_end_of_input_while_function_call_part1: "foo(" => 4, "end of input", PRIMARY,
        unexpected_end_of_input_while_function_call_part2: "foo(0" => 5, "end of input", ARGS,
        unexpected_end_of_input_while_function_call_part3: "foo(0," => 6, "end of input", PRIMARY,
        unexpected_end_of_input_while_function_call_part4: "foo(0, a" => 8, "end of input", ARGS,
        unexpected_closing_at_function_call: "foo(0]" => 5, "token \"]\"", ARGS,
        error_while_parsing_nested_expression: "([" => 1, "token \"[\"", PRIMARY,
        empty_expression: "" => 0, "end of input", PRIMARY,
        // The syntax rules of research section 2.1, checked against the reference.
        star_in_index: "a[*]" => 2, "token \"*\"", PRIMARY,
        star_alone: "*" => 0, "token \"*\"", PRIMARY,
        trailing_comma_in_call: "f(1,)" => 4, "token \")\"", PRIMARY,
        empty_parentheses: "()" => 1, "token \")\"", PRIMARY,
    }

    #[test]
    fn call_on_a_property_is_not_a_call() {
        let err = parse_err("a.b()");
        assert_eq!(
            err.message,
            "parser did not reach end of input after parsing the expression. 2 remaining \
             token(s) in the input: \"(\", \")\""
        );
        assert_eq!(err.start, 3);
    }

    #[test]
    fn call_on_a_parenthesized_name_is_not_a_call() {
        let err = parse_err("(f)(x)");
        assert!(
            err.message
                .ends_with("3 remaining token(s) in the input: \"(\", \"IDENT\", \")\"")
        );
        assert_eq!(err.start, 3);
    }

    #[test]
    fn keywords_are_case_sensitive() {
        assert_eq!(tree("TRUE"), "true");
        assert!(matches!(parse("TRUE").kind, ExprKind::Variable(_)));
        assert!(matches!(parse("Null").kind, ExprKind::Variable(_)));
    }

    #[test]
    fn keyword_followed_by_parenthesis_is_a_call() {
        assert_eq!(tree("true()"), "(true())");
    }
}

/// Which error wins when the lexer and the parser both fail, checked against the reference.
mod first_error {
    use super::*;

    /// `TestParseReturnFirstErrorOnMultipleErrors`: `.` alone fails once, and the parser
    /// stops at that error.
    #[test]
    fn parser_returns_the_first_error() {
        let err = parse_err(".");
        assert_eq!(err.start, 0);
        assert!(
            err.message
                .starts_with("unexpected token \".\" while parsing variable access")
        );
    }

    #[test]
    fn lexer_error_wins_over_the_parser_error_it_causes() {
        // The lexer fails on `"`, so the parser sees the end of the input after `a`.
        let err = parse_err("(a \"");
        assert_eq!(err.start, 3);
        assert!(
            err.message
                .starts_with("got unexpected character '\"' while lexing expression")
        );
        assert_eq!(err.rule, &INVALID_EXPRESSION_SYNTAX);
    }

    #[test]
    fn lexer_error_in_an_index_wins() {
        let err = parse_err("foo[0 \"");
        assert_eq!(err.start, 6);
        assert!(err.message.starts_with("got unexpected character '\"'"));
    }

    #[test]
    fn remaining_tokens_stop_at_a_lexer_error() {
        // The reference lists the tokens up to the lexer error and reports the remaining
        // tokens, not the lexer error.
        let err = parse_err("42 foo \"");
        assert_eq!(
            err.message,
            "parser did not reach end of input after parsing the expression. 1 remaining \
             token(s) in the input: \"IDENT\""
        );
        assert_eq!(err.start, 3);
    }

    #[test]
    fn remaining_tokens_continue_past_a_nul_in_a_string() {
        // Go's scanner records the NUL error and keeps scanning, so the reference lists the
        // tokens after it.
        let err = parse_err("42 a 'x\0' b ");
        assert_eq!(
            err.message,
            "parser did not reach end of input after parsing the expression. 3 remaining \
             token(s) in the input: \"IDENT\", \"STRING\", \"IDENT\""
        );
        assert_eq!(err.start, 3);
    }

    #[test]
    fn remaining_tokens_stop_at_a_nul_outside_a_string() {
        let err = parse_err("42 a \0 b ");
        assert_eq!(
            err.message,
            "parser did not reach end of input after parsing the expression. 1 remaining \
             token(s) in the input: \"IDENT\""
        );
        assert_eq!(err.start, 3);
    }

    #[test]
    fn nul_lexed_before_the_parser_stops_wins() {
        let err = parse_err("42 'a\0' 'b ");
        assert_eq!(
            err.message,
            "scan error while lexing expression: invalid character NUL"
        );
        assert_eq!(err.start, 5);
    }

    #[test]
    fn missing_end_marker_is_a_lexer_error() {
        let err = parse_expression("a.b").expect_err("no `}}`");
        assert_eq!(err.message, "unexpected EOF while lexing expression");
        assert_eq!(err.start, 3);
    }
}

/// Number literals: the reference's `TestParseExpressionNumberLiteralsError` and the integer
/// range of `docs/differences.md#int-literal-range`.
mod numbers {
    use super::*;

    fn int(input: &str) -> f64 {
        match parse(input).kind {
            ExprKind::Int(value) => value,
            kind => panic!("{input:?} is not an integer: {kind:?}"),
        }
    }

    #[test]
    fn integer_beyond_32_bits_is_a_number() {
        assert_eq!(int("2147483648"), 2_147_483_648.0);
        assert_eq!(int("-2147483649"), -2_147_483_649.0);
    }

    #[test]
    fn hex_integer_beyond_32_bits_is_a_number() {
        assert_eq!(int("0xFFFFFFFFFF"), 1_099_511_627_775.0);
        assert_eq!(int("-0xFFFFFFFFFF"), -1_099_511_627_775.0);
    }

    #[test]
    fn integer_beyond_53_bits_rounds_to_the_nearest_double() {
        assert_eq!(int("9007199254740993"), 9_007_199_254_740_992.0);
        assert_eq!(int("0x20000000000001"), 9_007_199_254_740_992.0);
        assert_eq!(int("0x20000000000003"), 9_007_199_254_740_996.0);
    }

    #[test]
    fn long_hex_integer_rounds_to_the_nearest_double() {
        // 2^128 + 2^75 + 1 is just above the halfway point between 2^128 and 2^128 + 2^76, so
        // it rounds up. Without its last digit it would round down to the even 2^128.
        let hex = format!("0x1{}8{}1", "0".repeat(13), "0".repeat(17));
        assert_eq!(int(&hex), 2f64.powi(128) + 2f64.powi(76));
    }

    #[test]
    fn largest_double_is_a_number() {
        let max = format!("{}", f64::MAX);
        assert_eq!(int(&max), f64::MAX);
        let hex = format!("0x{}{}", "F".repeat(13), "8".to_owned() + &"0".repeat(242));
        assert_eq!(int(&hex), f64::MAX);
    }

    #[test]
    fn integer_beyond_a_double_is_an_error() {
        let digits = format!("1{}", "0".repeat(309));
        let err = parse_err(&digits);
        assert_eq!(
            err.message,
            format!(
                "parsing invalid integer literal \"{digits}\": strconv.ParseInt: parsing \
                 \"{digits}\": value out of range"
            )
        );
        assert_eq!((err.start, err.end), (0, 310));
        assert_eq!(err.rule, &INVALID_NUMBER_LITERAL);
    }

    #[test]
    fn hex_integer_beyond_a_double_is_an_error() {
        let hex = format!("-0x{}", "F".repeat(256));
        let err = parse_err(&hex);
        assert_eq!(
            err.message,
            format!(
                "parsing invalid integer literal \"{hex}\": strconv.ParseInt: parsing \
                 \"{hex}\": value out of range"
            )
        );
        assert_eq!(err.rule, &INVALID_NUMBER_LITERAL);
    }

    #[test]
    fn float_beyond_a_double_is_an_error() {
        let err = parse_err("-1e400");
        assert_eq!(
            err.message,
            "parsing invalid float literal \"-1e400\": strconv.ParseFloat: parsing \"-1e400\": \
             value out of range"
        );
        assert_eq!((err.start, err.end), (0, 6));
        assert_eq!(err.rule, &INVALID_NUMBER_LITERAL);
    }

    #[test]
    fn float_below_the_smallest_double_is_zero() {
        assert_eq!(tree("1e-400"), "f:0");
    }

    #[test]
    fn exponent_with_plus_sign_is_a_float() {
        assert_eq!(tree("1e+5"), "f:100000");
    }

    /// `TestParseExpressionNumberLiteralsError` injects a number token with the text `abc`. The
    /// only number token whose text is not a number starts with a byte order mark, which the
    /// lexer keeps in the first token.
    #[test]
    fn integer_with_byte_order_mark_is_an_invalid_integer_literal() {
        let err = parse_err("\u{feff}1");
        assert_eq!(
            err.message,
            "parsing invalid integer literal \"\\ufeff1\": strconv.ParseInt: parsing \
             \"\\ufeff1\": invalid syntax"
        );
        assert_eq!(err.start, 0);
        assert_eq!(err.rule, &INVALID_NUMBER_LITERAL);
    }

    #[test]
    fn float_with_byte_order_mark_is_an_invalid_float_literal() {
        let err = parse_err("\u{feff}1.5");
        assert_eq!(
            err.message,
            "parsing invalid float literal \"\\ufeff1.5\": strconv.ParseFloat: parsing \
             \"\\ufeff1.5\": invalid syntax"
        );
        assert_eq!(err.rule, &INVALID_NUMBER_LITERAL);
    }

    #[test]
    fn invalid_number_literal_rule_has_the_expression_kind() {
        assert_eq!(INVALID_NUMBER_LITERAL.id, "invalid-number-literal");
        assert_eq!(INVALID_NUMBER_LITERAL.kind, "expression");
    }
}

/// Precedence and associativity, following the expression parser of `actions/languageservices`
/// (`docs/differences.md#comparison-precedence`).
mod precedence {
    use super::*;

    /// Binary operators with their precedence level. A higher level binds tighter.
    const OPERATORS: [(&str, u8); 8] = [
        ("||", 1),
        ("&&", 2),
        ("==", 3),
        ("!=", 3),
        ("<", 4),
        ("<=", 4),
        (">", 4),
        (">=", 4),
    ];

    #[test]
    fn each_pair_of_binary_operators_groups_by_precedence() {
        for (a, a_level) in OPERATORS {
            for (b, b_level) in OPERATORS {
                let input = format!("x {a} y {b} z");
                // Comparisons and equality group to the left. `&&` and `||` group to the
                // right, as in the reference.
                let left_first = a_level > b_level || (a_level == b_level && a_level > 2);
                let expected = if left_first {
                    format!("({b} ({a} x y) z)")
                } else {
                    format!("({a} x ({b} y z))")
                };
                assert_eq!(tree(&input), expected, "{input}");
            }
        }
    }

    #[test]
    fn not_binds_tighter_than_every_binary_operator() {
        for (op, _) in OPERATORS {
            assert_eq!(tree(&format!("!x {op} y")), format!("({op} (! x) y)"));
            assert_eq!(tree(&format!("x {op} !y")), format!("({op} x (! y))"));
        }
    }

    #[test]
    fn property_access_binds_tighter_than_not() {
        assert_eq!(tree("!a.b"), "(! (. a b))");
        assert_eq!(tree("!a[0].*"), "(! (.* ([] a 0)))");
        assert_eq!(tree("!f(x).y"), "(! (. (f() x) y))");
    }

    #[test]
    fn less_than_binds_tighter_than_equality() {
        assert_eq!(tree("'a' < 'b' == true"), r#"(== (< "a" "b") #true)"#);
        assert_eq!(tree("true == 'a' < 'b'"), r#"(== #true (< "a" "b"))"#);
    }

    #[test]
    fn comparisons_group_to_the_left() {
        assert_eq!(tree("a < b < c"), "(< (< a b) c)");
        assert_eq!(tree("a == b != c"), "(!= (== a b) c)");
        assert_eq!(tree("a == b == c == d"), "(== (== (== a b) c) d)");
    }

    #[test]
    fn logical_operators_group_to_the_right() {
        assert_eq!(tree("a && b && c"), "(&& a (&& b c))");
        assert_eq!(tree("a || b || c"), "(|| a (|| b c))");
    }

    fn operands(expr: &Expr, want: LogicalOp) -> &[Expr] {
        match &expr.kind {
            ExprKind::Logical { op, operands } if *op == want => operands,
            kind => panic!("not a {want} chain: {kind:?}"),
        }
    }

    #[test]
    fn chain_of_one_logical_operator_is_one_node() {
        let expr = parse("a && b && c || d");
        let or = operands(&expr, LogicalOp::Or);
        assert_eq!(or.len(), 2);
        let and = operands(&or[0], LogicalOp::And);
        let names: Vec<_> = and.iter().map(sexpr).collect();
        assert_eq!(names, ["a", "b", "c"]);
    }

    #[test]
    fn parenthesized_chain_is_an_operand_of_its_own() {
        let expr = parse("a && (b && c)");
        let and = operands(&expr, LogicalOp::And);
        assert_eq!(and.len(), 2);
        assert_eq!(operands(&and[1], LogicalOp::And).len(), 2);
    }

    #[test]
    fn parentheses_override_precedence() {
        assert_eq!(tree("'a' < ('b' == true)"), r#"(< "a" (== "b" #true))"#);
        assert_eq!(tree("(a || b) && c"), "(&& (|| a b) c)");
    }

    #[test]
    fn operator_kinds_print_their_symbols() {
        let compare = [
            (CompareOp::Less, "<"),
            (CompareOp::LessEq, "<="),
            (CompareOp::Greater, ">"),
            (CompareOp::GreaterEq, ">="),
            (CompareOp::Eq, "=="),
            (CompareOp::NotEq, "!="),
        ];
        for (op, text) in compare {
            assert_eq!(op.as_str(), text);
            assert_eq!(op.to_string(), text);
        }
        assert!(CompareOp::Eq.is_equality() && CompareOp::NotEq.is_equality());
        assert!(!CompareOp::Less.is_equality());
        assert_eq!(LogicalOp::And.as_str(), "&&");
        assert_eq!(LogicalOp::Or.to_string(), "||");
    }
}

/// `TestParseExpressionTokenPosition`, and the range of each node.
mod positions {
    use super::*;

    /// The reference reports errors about a node at its token, the start of the receiver or
    /// the left operand for operators. Each input is parsed after two spaces.
    #[test]
    fn node_token_starts_where_the_reference_reports_errors() {
        let cases = [
            ("null", 0),
            ("true", 0),
            ("foo", 0),
            ("foo(1, 2, 3)", 0),
            ("(42)", 1),
            ("-10", 0),
            ("-1.0e3", 0),
            ("github.issue.label.name", 0),
            ("labels.*", 0),
            ("arr[idx]", 0),
            ("!true", 0),
            ("0 < 1", 0),
            ("0 <= 1", 0),
            ("0 > 1", 0),
            ("0 == 1", 0),
            ("0 != 1", 0),
            ("true && false", 0),
            ("true || false", 0),
        ];
        for (input, offset) in cases {
            let expr = parse(&format!("  {input}"));
            assert_eq!(expr.token.start, offset + 2, "{input}");
        }
    }

    #[test]
    fn node_token_is_the_whole_first_token() {
        let src = "github.event.x }}";
        let (expr, _) = parse_expression(src).expect("valid");
        assert_eq!(&src[expr.token.range()], "github");
        let src = "(a) == b }}";
        let (expr, _) = parse_expression(src).expect("valid");
        assert_eq!(&src[expr.token.range()], "a");
        let src = "!x }}";
        let (expr, _) = parse_expression(src).expect("valid");
        assert_eq!(&src[expr.token.range()], "!");
    }

    #[test]
    fn node_range_covers_its_own_tokens_without_outer_parentheses() {
        let cases = [
            ("(42)", "42"),
            ("( a.b ).c", "( a.b ).c"),
            ("a['x'].*", "a['x'].*"),
            ("!(a)", "!(a)"),
            ("f( 1, 2 )", "f( 1, 2 )"),
            ("(a) && (b)", "(a) && (b)"),
            ("((a == b))", "a == b"),
        ];
        for (input, text) in cases {
            let src = format!("{input} }}}}");
            let (expr, _) = parse_expression(&src).expect("valid");
            assert_eq!(&src[expr.range()], text, "{input}");
        }
    }

    #[test]
    fn child_ranges_point_at_the_children() {
        let src = "contains(github.event.labels.*.name, 'bug') }}";
        let (expr, _) = parse_expression(src).expect("valid");
        let ExprKind::Call { callee, args } = &expr.kind else {
            panic!("not a call: {expr:?}");
        };
        assert_eq!(&src[callee.range()], "contains");
        assert_eq!(&src[args[0].range()], "github.event.labels.*.name");
        assert_eq!(&src[args[1].range()], "'bug'");
        let ExprKind::Property { receiver, name } = &args[0].kind else {
            panic!("not a property: {:?}", args[0]);
        };
        assert_eq!(&src[name.range()], "name");
        assert_eq!(&src[receiver.range()], "github.event.labels.*");
    }

    #[test]
    fn parse_returns_the_offset_just_past_the_end_marker() {
        let src = "a.b }} and ${{ c }}";
        let (_, end) = parse_expression(src).expect("valid");
        assert_eq!(&src[..end as usize], "a.b }}");
    }
}

/// Names are folded for comparison and keep their original spelling for messages.
mod case_folding {
    use super::*;

    #[test]
    fn variable_and_property_names_are_lowercased_and_keep_their_spelling() {
        let expr = parse("GitHub.Event");
        let ExprKind::Property { receiver, name } = &expr.kind else {
            panic!("not a property: {expr:?}");
        };
        assert_eq!((name.as_str(), name.original()), ("event", "Event"));
        let ExprKind::Variable(variable) = &receiver.kind else {
            panic!("not a variable: {receiver:?}");
        };
        assert_eq!(
            (variable.as_str(), variable.original()),
            ("github", "GitHub")
        );
    }

    #[test]
    fn callee_keeps_its_spelling_and_has_a_folded_form() {
        let ExprKind::Call { callee, .. } = parse("fromJSON('{}')").kind else {
            panic!("not a call");
        };
        assert_eq!(
            (callee.original(), callee.as_str()),
            ("fromJSON", "fromjson")
        );
    }

    #[test]
    fn string_literal_keeps_its_spelling_and_has_a_folded_form() {
        let expr = parse("steps['MyStep']");
        let ExprKind::Index { index, .. } = &expr.kind else {
            panic!("not an index access: {expr:?}");
        };
        let ExprKind::String(key) = &index.kind else {
            panic!("not a string: {index:?}");
        };
        assert_eq!((key.value(), key.folded()), ("MyStep", "mystep"));
    }

    #[test]
    fn string_literal_folds_like_go() {
        let ExprKind::String(lit) = parse("'ÀÉ\u{130}'").kind else {
            panic!("not a string");
        };
        // Go's `strings.ToLower` maps U+0130 to a plain `i`.
        assert_eq!(lit.folded(), "àéi");
    }

    #[test]
    fn lowercase_names_are_their_own_folded_form() {
        let ExprKind::String(lit) = parse("'abc'").kind else {
            panic!("not a string");
        };
        assert_eq!((lit.value(), lit.folded()), ("abc", "abc"));
        let ExprKind::Variable(name) = parse("env").kind else {
            panic!("not a variable");
        };
        assert_eq!((name.as_str(), name.original()), ("env", "env"));
    }
}

/// The visitor, which works like the reference's `VisitExprNode`.
mod visitor {
    use super::*;

    fn events(input: &str) -> Vec<String> {
        let expr = parse(input);
        let mut out = Vec::new();
        expr.walk(&mut |node: &Expr, parent: Option<&Expr>, visit: Visit| {
            let mark = match visit {
                Visit::Enter => "enter",
                Visit::Leave => "leave",
            };
            let parent = parent.map_or_else(|| "-".to_owned(), sexpr);
            out.push(format!("{mark} {} in {parent}", sexpr(node)));
        });
        out
    }

    #[test]
    fn visitor_enters_and_leaves_each_node() {
        assert_eq!(
            events("!a.b"),
            [
                "enter (! (. a b)) in -",
                "enter (. a b) in (! (. a b))",
                "enter a in (. a b)",
                "leave a in (. a b)",
                "leave (. a b) in (! (. a b))",
                "leave (! (. a b)) in -",
            ]
        );
    }

    #[test]
    fn visitor_visits_the_index_before_the_operand() {
        let order: Vec<_> = events("x[y].*")
            .into_iter()
            .filter(|event| event.starts_with("enter"))
            .collect();
        assert_eq!(
            order,
            [
                "enter (.* ([] x y)) in -",
                "enter ([] x y) in (.* ([] x y))",
                "enter y in ([] x y)",
                "enter x in ([] x y)",
            ]
        );
    }

    #[test]
    fn visitor_visits_operands_and_arguments_in_order() {
        let leaves: Vec<_> = events("f(a, b) == c && !d || e")
            .into_iter()
            .filter_map(|event| {
                let node = event.strip_prefix("leave ")?.split(' ').next()?;
                (!node.starts_with('(')).then(|| node.to_owned())
            })
            .collect();
        assert_eq!(leaves, ["a", "b", "c", "d", "e"]);
    }
}

/// Nesting deeper than the parser's limit is an error instead of a stack overflow.
mod depth {
    use super::*;

    const MESSAGE: &str = "expression is nested more than 128 levels deep";

    #[test]
    fn nesting_up_to_the_limit_parses() {
        let input = format!("{}x{}", "(".repeat(128), ")".repeat(128));
        assert_eq!(tree(&input), "x");
        let input = format!("{}x", "!".repeat(128));
        assert!(tree(&input).starts_with("(! (! "));
    }

    #[test]
    fn nesting_one_level_beyond_the_limit_is_an_error() {
        let err = parse_err(&format!("{}x{}", "(".repeat(129), ")".repeat(129)));
        assert_eq!(err.message, MESSAGE);
        assert_eq!((err.start, err.end), (128, 129));
        assert_eq!(err.rule, &INVALID_EXPRESSION_SYNTAX);
        let err = parse_err(&format!("{}x", "!".repeat(129)));
        assert_eq!(err.message, MESSAGE);
    }

    #[test]
    fn every_kind_of_nesting_counts_toward_the_limit() {
        let cases = [
            format!("x{}", ".y".repeat(200)),
            format!("x{}", ".*".repeat(200)),
            format!("x{}", "[0]".repeat(200)),
            format!("{}x{}", "f(".repeat(200), ")".repeat(200)),
            format!("{}x{}", "x && (".repeat(200), ")".repeat(200)),
            format!("{}x{}", "x || (".repeat(200), ")".repeat(200)),
            vec!["x"; 200].join(" == "),
            vec!["x"; 200].join(" < "),
        ];
        for input in cases {
            let err = parse_err(&input);
            assert_eq!(err.message, MESSAGE, "{}", &input[..20]);
        }
    }

    #[test]
    fn property_chain_one_level_beyond_the_limit_is_an_error_at_its_last_dot() {
        assert!(tree(&format!("x{}", ".y".repeat(128))).starts_with("(. (. "));
        let err = parse_err(&format!("x{}", ".y".repeat(129)));
        assert_eq!(err.message, MESSAGE);
        assert_eq!((err.start, err.end), (257, 258));
    }

    #[test]
    fn logical_chain_of_any_length_is_one_level() {
        for op in [" && ", " || "] {
            let chain = vec!["x"; 1000].join(op);
            let input = format!("{}{chain}{}", "(".repeat(127), ")".repeat(127));
            assert!(matches!(parse(&input).kind, ExprKind::Logical { .. }), "{op}");
            let input = format!("{}{chain}{}", "(".repeat(128), ")".repeat(128));
            assert_eq!(parse_err(&input).message, MESSAGE, "{op}");
        }
    }

    #[test]
    fn logical_chain_of_comparisons_is_two_levels() {
        let input = format!("{}a == 1", "a == 1 && ".repeat(64));
        let expr = parse(&input);
        assert!(matches!(&expr.kind, ExprKind::Logical { operands, .. } if operands.len() == 65));
    }

    #[test]
    fn many_alternatives_in_a_condition_parse() {
        let input = (0..65)
            .map(|n| format!("github.actor == 'user{n}'"))
            .collect::<Vec<_>>()
            .join(" || ");
        assert!(matches!(parse(&input).kind, ExprKind::Logical { .. }));
    }

    #[test]
    fn index_counts_levels_from_its_own_node() {
        // The index access is one level above the deeper of its operand and its index.
        let input = format!("a{}[x{}]", ".b".repeat(100), ".y".repeat(30));
        assert!(matches!(parse(&input).kind, ExprKind::Index { .. }));
        let input = format!("a{}[x{}]", ".b".repeat(127), ".y".repeat(127));
        assert!(matches!(parse(&input).kind, ExprKind::Index { .. }));
        let input = format!("a{}[x]", ".b".repeat(128));
        let err = parse_err(&input);
        assert_eq!(err.message, MESSAGE);
        assert_eq!(err.start, 257);
        let err = parse_err(&format!("a[x{}]", ".y".repeat(128)));
        assert_eq!(err.message, MESSAGE);
        assert_eq!(err.start, 257);
    }

    #[test]
    fn operator_after_an_operand_at_the_limit_is_an_error_at_the_operator() {
        let operand = format!("x{}", ".y".repeat(128));
        for op in ["==", "<", "&&", "||"] {
            let err = parse_err(&format!("{operand} {op} z"));
            assert_eq!(err.message, MESSAGE, "{op}");
            assert_eq!(err.start, 258, "{op}");
            let err = parse_err(&format!("z {op} {operand}"));
            assert_eq!(err.message, MESSAGE, "{op}");
        }
    }

    #[test]
    fn deep_nesting_does_not_overflow_the_stack() {
        let err = parse_err(&format!("{}x", "(".repeat(100_000)));
        assert_eq!(err.message, MESSAGE);
        assert_eq!(err.start, 128);
    }
}

/// A chain of 100,000 operands, as a generated condition could have, is safe to parse, check,
/// walk, print, compare, clone, and drop on a thread with a 2 MiB stack, the default size of a
/// spawned thread.
mod long_chains {
    use std::io::Write as _;

    use super::*;
    use workflint_data::UNTRUSTED_INPUTS;
    use workflint_expr::{TypeEnv, find_untrusted_inputs};

    const OPERANDS: usize = 100_000;
    const STACK: usize = 2 << 20;

    fn on_small_stack(f: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new()
            .stack_size(STACK)
            .spawn(f)
            .expect("thread spawns")
            .join()
            .expect("thread does not panic");
    }

    fn check_chain(op: &'static str) {
        on_small_stack(move || {
            let operand = "github.event.issue.title == 'x'";
            let src = format!("{} }}}}", vec![operand; OPERANDS].join(op));
            let (expr, _) = parse_expression(&src).expect("valid");
            let mut env = TypeEnv::new();
            env.set_untrusted_inputs(UNTRUSTED_INPUTS);
            let (ty, errors) = env.check(&expr);
            assert_eq!(ty.to_string(), "bool");
            assert_eq!(errors.len(), OPERANDS);
            assert_eq!(find_untrusted_inputs(&expr, UNTRUSTED_INPUTS).len(), OPERANDS);
            let mut nodes = 0_usize;
            expr.walk(&mut |_: &Expr, _: Option<&Expr>, _: Visit| nodes += 1);
            assert_eq!(nodes, 2 * (1 + 6 * OPERANDS));
            assert!(!expr.is_constant());
            write!(std::io::sink(), "{expr:?}").expect("formats");
            let copy = expr.clone();
            assert_eq!(copy, expr);
            drop(copy);
            drop(expr);
        });
    }

    #[test]
    fn long_or_chain_is_safe_on_a_small_stack() {
        check_chain(" || ");
    }

    #[test]
    fn long_and_chain_is_safe_on_a_small_stack() {
        check_chain(" && ");
    }

    #[test]
    fn long_alternating_chain_is_safe_on_a_small_stack() {
        on_small_stack(|| {
            let src = format!("{}x }}}}", "x && x || ".repeat(OPERANDS / 2));
            let (expr, _) = parse_expression(&src).expect("valid");
            let (_, errors) = TypeEnv::new().check(&expr);
            assert_eq!(errors.len(), OPERANDS + 1);
            drop(expr);
        });
    }
}
