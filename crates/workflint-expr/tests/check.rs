//! Ports of the type checker tests in the reference's `expr_sema_test.go`, plus the cases where
//! workflint's checker differs or the reference has no test.
//!
//! The modules `semantics_check_ok`, `semantics_check_error`, `compare_operands_check`,
//! `update_inputs_multiple_times`, `update_dispatch_inputs`, and `is_constant` port the
//! reference's tests of the same names. The cases that check context availability and where
//! `success()` and the other special functions are allowed move to M3.6, and the cases that
//! type a `fromJSON()` literal move to M3.9. The reference's error tests check that each
//! message contains a fragment. These tests compare whole messages, the rule, and the byte
//! offset where the reference reports each error.
//!
//! The reference's tests that call a test function, such as `test().foo.*.bar`, are ported in
//! `function_results_from_matrix` with the function's result in `matrix` instead.

#![allow(clippy::expect_used, clippy::panic)]

use workflint_data::global_context;
use workflint_diagnostics::RuleMeta;
use workflint_expr::rules::{
    CASE_ARGUMENT_COUNT, INVALID_COMPARISON, INVALID_CONFIG_VARIABLE_NAME, INVALID_INDEX_ACCESS,
    INVALID_OBJECT_FILTER, INVALID_PROPERTY_ACCESS, MISSING_FORMAT_ARGUMENT,
    UNDEFINED_CONFIG_VARIABLE, UNDEFINED_FUNCTION, UNDEFINED_PROPERTY, UNDEFINED_VARIABLE,
    UNUSED_FORMAT_ARGUMENT, WRONG_ARGUMENT_COUNT, WRONG_ARGUMENT_TYPE,
};
use workflint_expr::{Expr, ExprError, ObjectTy, Rest, Ty, TypeEnv, parse_expression};

/// The names in the message of an undefined variable in the default environment.
const GLOBAL_NAMES: &str = r#""env", "github", "inputs", "job", "matrix", "needs", "runner", "secrets", "steps", "strategy", "vars""#;

/// The names in the message of an undefined function.
const FUNCTION_NAMES: &str = r#""always", "cancelled", "case", "contains", "endswith", "failure", "format", "fromjson", "hashfiles", "join", "startswith", "success", "tojson""#;

/// The printed type of the `runner` context.
const RUNNER: &str = "{arch: string; debug: string; environment: string; name: string; os: string; temp: string; tool_cache: string}";

/// Parses `input` followed by `}}`.
fn parse(input: &str) -> Expr {
    let src = format!("{input}}}}}");
    match parse_expression(&src) {
        Ok((expr, _)) => expr,
        Err(err) => panic!("parsing {input:?} failed at {}: {err}", err.start),
    }
}

/// Checks `input` and returns its type. Fails if the check reports an error.
fn ty_in(env: &TypeEnv, input: &str) -> Ty {
    let (ty, errors) = env.check(&parse(input));
    assert!(errors.is_empty(), "checking {input:?} failed: {errors:?}");
    ty
}

/// Checks `input` in the default environment and returns its type.
fn ty(input: &str) -> Ty {
    ty_in(&TypeEnv::new(), input)
}

/// An expected error: the rule ID, the byte offset, and the message.
type Error = (&'static str, u32, String);

fn error(rule: &'static RuleMeta, start: u32, message: impl Into<String>) -> Error {
    (rule.id, start, message.into())
}

/// Checks `input` and returns its errors.
fn errors_in(env: &TypeEnv, input: &str) -> Vec<Error> {
    let (_, errors) = env.check(&parse(input));
    errors
        .iter()
        .map(|err: &ExprError| (err.rule.id, err.start, err.message.clone()))
        .collect()
}

/// Checks `input` in the default environment and returns its errors.
fn errors(input: &str) -> Vec<Error> {
    errors_in(&TypeEnv::new(), input)
}

fn strict<const N: usize>(props: [(&str, Ty); N]) -> ObjectTy {
    ObjectTy::new(props, Rest::Strict)
}

fn global(name: &str) -> Ty {
    Ty::from(global_context(name).expect("a global context"))
}

/// The `steps` type of the reference's tests: one step, `id`, with empty loose outputs.
fn steps_with(id: &str) -> ObjectTy {
    strict([(
        id,
        Ty::strict([
            ("outputs", Ty::empty_loose()),
            ("conclusion", Ty::String),
            ("outcome", Ty::String),
        ]),
    )])
}

/// The `needs` type of the reference's tests: one job, `foo`, with two outputs.
fn needs_foo() -> ObjectTy {
    strict([(
        "foo",
        Ty::strict([
            (
                "outputs",
                Ty::strict([("out1", Ty::String), ("out2", Ty::String)]),
            ),
            ("result", Ty::String),
        ]),
    )])
}

fn env_with_matrix(matrix: ObjectTy) -> TypeEnv {
    let mut env = TypeEnv::new();
    env.set_matrix(matrix);
    env
}

/// `TestExprSemanticsCheckOK`.
mod semantics_check_ok {
    use super::*;

    #[test]
    fn null() {
        assert_eq!(ty("null"), Ty::Null);
    }

    #[test]
    fn bool() {
        assert_eq!(ty("true"), Ty::Bool);
    }

    #[test]
    fn integer() {
        assert_eq!(ty("42"), Ty::Number);
    }

    #[test]
    fn float() {
        assert_eq!(ty("-3.14e16"), Ty::Number);
    }

    #[test]
    fn string() {
        assert_eq!(ty("'this is string'"), Ty::String);
    }

    #[test]
    fn variable() {
        assert_eq!(ty("github"), global("github"));
    }

    #[test]
    fn object_property_dereference_of_global_variable() {
        assert_eq!(ty("job.container.network"), Ty::String);
    }

    #[test]
    fn object_property_dereference_for_any_type() {
        assert_eq!(ty("github.event.labels"), Ty::Any);
    }

    #[test]
    fn array_element_dereference_with_any_type() {
        assert_eq!(
            ty("github.event.labels.*.name"),
            Ty::filtered_array(Ty::Any)
        );
    }

    #[test]
    fn nested_array_element_dereference() {
        assert_eq!(
            ty("github.event.issues.*.labels.*.name"),
            Ty::filtered_array(Ty::Any)
        );
    }

    #[test]
    fn array_element_dereference_with_object_type() {
        assert_eq!(ty("github.*.name"), Ty::filtered_array(Ty::Any));
    }

    #[test]
    fn object_property_index_access_with_any_type() {
        assert_eq!(ty("github.event['FOOO']"), Ty::Any);
    }

    #[test]
    fn array_element_index_access_with_any_type_fallback() {
        assert_eq!(ty("github.event.labels[0]"), Ty::Any);
    }

    #[test]
    fn index_access_to_dereferenced_array_with_any_type_fallback() {
        assert_eq!(ty("github.event.labels.*.name[0]"), Ty::Any);
    }

    #[test]
    fn not_operator() {
        assert_eq!(ty("!true"), Ty::Bool);
    }

    #[test]
    fn not_operator_with_non_bool_operand() {
        assert_eq!(ty("!null"), Ty::Bool);
    }

    #[test]
    fn less_operator() {
        assert_eq!(ty("0 < 1"), Ty::Bool);
    }

    #[test]
    fn less_eq_operator() {
        assert_eq!(ty("0 <= 1"), Ty::Bool);
    }

    #[test]
    fn greater_operator() {
        assert_eq!(ty("0 > 1"), Ty::Bool);
    }

    #[test]
    fn greater_eq_operator() {
        assert_eq!(ty("0 >= 1"), Ty::Bool);
    }

    #[test]
    fn eq_operator() {
        assert_eq!(ty("0 == 1"), Ty::Bool);
    }

    #[test]
    fn not_eq_operator() {
        assert_eq!(ty("0 != 1"), Ty::Bool);
    }

    #[test]
    fn and_operator() {
        assert_eq!(ty("true && false"), Ty::Bool);
    }

    #[test]
    fn or_operator() {
        assert_eq!(ty("true || false"), Ty::Bool);
    }

    #[test]
    fn and_operator_with_non_bool_operands() {
        assert_eq!(ty("10 && 'foo'"), Ty::String);
    }

    #[test]
    fn or_operator_with_non_bool_operands() {
        assert_eq!(ty("'foo' || 42"), Ty::String);
    }

    #[test]
    fn eq_operator_with_loose_equality_check() {
        assert_eq!(ty("true == 1.1"), Ty::Bool);
    }

    #[test]
    fn matrix_value_with_typed_matrix_values() {
        let env = env_with_matrix(strict([("foooo", Ty::String)]));
        assert_eq!(ty_in(&env, "matrix.foooo"), Ty::String);
    }

    #[test]
    fn step_output_value_with_typed_steps_outputs() {
        let mut env = TypeEnv::new();
        env.set_steps(steps_with("foo"));
        assert_eq!(ty_in(&env, "steps.foo.outputs"), Ty::empty_loose());
    }

    #[test]
    fn step_conclusion_with_typed_steps_outputs() {
        let mut env = TypeEnv::new();
        env.set_steps(steps_with("foo"));
        assert_eq!(ty_in(&env, "steps.foo.conclusion"), Ty::String);
    }

    #[test]
    fn needs_context_object() {
        assert_eq!(ty("needs"), Ty::empty_strict());
    }

    #[test]
    fn output_string_in_needs_context_object() {
        let mut env = TypeEnv::new();
        env.set_needs(needs_foo());
        assert_eq!(ty_in(&env, "needs.foo.outputs.out1"), Ty::String);
    }

    #[test]
    fn result_in_needs_context_object() {
        let mut env = TypeEnv::new();
        env.set_needs(needs_foo());
        assert_eq!(ty_in(&env, "needs.foo.result"), Ty::String);
    }

    #[test]
    fn string_is_coerced_into_bool() {
        assert_eq!(ty("!'hello'"), Ty::Bool);
    }

    #[test]
    fn coerce_number_into_bool() {
        assert_eq!(ty("!42"), Ty::Bool);
    }

    #[test]
    fn coerce_null_into_bool() {
        assert_eq!(ty("!null"), Ty::Bool);
    }

    #[test]
    fn coerce_string_into_bool() {
        assert_eq!(ty("!'hello'"), Ty::Bool);
    }

    #[test]
    fn case_insensitive_comparison_for_context_name() {
        assert_eq!(ty("JOB.CONTAINER.NETWORK"), Ty::String);
    }

    #[test]
    fn map_object_dereference() {
        assert_eq!(ty("env.FOO"), Ty::String);
    }

    #[test]
    fn loose_object_at_object_filter() {
        assert_eq!(ty("github.event.*.foo"), Ty::filtered_array(Ty::Any));
    }

    #[test]
    fn strict_object_which_has_at_least_one_object_element_at_object_filter() {
        assert_eq!(ty("github.*.foo"), Ty::filtered_array(Ty::Any));
    }

    #[test]
    fn map_object_index_access_with_string_literal() {
        assert_eq!(ty("env['FOO']"), Ty::String);
    }

    #[test]
    fn map_object_index_access_with_dynamic_value() {
        assert_eq!(ty("env[github.action]"), Ty::String);
    }

    #[test]
    fn nested_object_in_map_object() {
        assert_eq!(ty("job.services.my_service.network"), Ty::String);
    }

    #[test]
    fn narrowed_inputs_object() {
        let mut env = TypeEnv::new();
        env.add_inputs(strict([("hello", Ty::Number)]));
        assert_eq!(ty_in(&env, "inputs.hello"), Ty::Number);
    }

    #[test]
    fn narrowed_secrets_object() {
        let mut env = TypeEnv::new();
        env.set_secrets(strict([("token", Ty::String)]));
        assert_eq!(ty_in(&env, "secrets.token"), Ty::String);
    }

    #[test]
    fn default_secrets_object() {
        assert_eq!(ty("secrets.any_value"), Ty::String);
    }

    #[test]
    fn automatically_supplied_secrets() {
        let mut env = TypeEnv::new();
        env.set_secrets(strict([("foo", Ty::String)]));
        for input in [
            "secrets.github_token",
            "secrets.ACTIONS_STEP_DEBUG",
            "secrets.ACTIONS_RUNNER_DEBUG",
        ] {
            assert_eq!(ty_in(&env, input), Ty::String, "{input}");
        }
    }

    #[test]
    fn jobs_object() {
        let mut env = TypeEnv::new();
        env.set_jobs(strict([("some_job", Ty::empty_loose())]));
        assert_eq!(ty_in(&env, "jobs.some_job"), Ty::empty_loose());
    }

    /// The reference's "available context" and "available contexts" cases, without the
    /// availability, which M3.6 checks.
    #[test]
    fn strategy_property() {
        assert_eq!(ty("strategy.fail-fast"), Ty::Bool);
    }

    #[test]
    fn configuration_variable() {
        assert_eq!(ty("vars.SOME_VARIABLE"), Ty::String);
    }

    #[test]
    fn known_configuration_variable() {
        let mut env = TypeEnv::new();
        env.set_config_vars(["SOME_VARIABLE"]);
        assert_eq!(ty_in(&env, "vars.SOME_VARIABLE"), Ty::String);
    }

    #[test]
    fn configuration_variable_name_is_case_insensitive() {
        let mut env = TypeEnv::new();
        env.set_config_vars(["some_variable"]);
        assert_eq!(ty_in(&env, "vars.SOME_VARIABLE"), Ty::String);
    }

    #[test]
    fn narrow_type_of_and_operator_by_assumed_value() {
        assert_eq!(ty("('foo' && 10) || 20"), Ty::Number);
    }

    #[test]
    fn narrow_type_of_or_operator_by_assumed_value() {
        assert_eq!(ty("('foo' || 10) && 20"), Ty::Number);
    }

    #[test]
    fn narrow_type_of_nested_and_operator() {
        assert_eq!(ty("((('foo' && true) || false) && 10) || 20"), Ty::Number);
    }

    #[test]
    fn narrow_type_of_nested_or_operator() {
        assert_eq!(ty("((('foo' || true) && false) || 10) && 20"), Ty::Number);
    }

    #[test]
    fn dont_narrow_type_on_nested_and_operator() {
        assert_eq!(ty("('foo' && 10) && 20"), Ty::String);
    }

    #[test]
    fn dont_narrow_type_on_nested_or_operator() {
        assert_eq!(ty("('foo' || 10) || 20"), Ty::String);
    }

    #[test]
    fn narrowed_or_operator_at_lhs_and_and_operator_at_rhs() {
        assert_eq!(ty("('foo' || 10) && (('foo' && 10) || 20)"), Ty::Number);
    }

    #[test]
    fn narrowed_and_operator_at_lhs_and_or_operator_at_rhs() {
        assert_eq!(ty("('foo' && 10) || (('foo' || 10) && 20)"), Ty::Number);
    }

    #[test]
    fn not_operator_negates_and_operator_type_narrowing() {
        assert_eq!(ty("!('foo' && 10) && 20"), Ty::Number);
    }

    #[test]
    fn not_operator_negates_or_operator_type_narrowing() {
        assert_eq!(ty("!('foo' || 10) || 20"), Ty::Number);
    }

    #[test]
    fn double_not_operators_does_nothing_on_type_narrowing() {
        assert_eq!(ty("!!('foo' || 10) && 20"), Ty::Number);
    }
    #[test]
    fn function_call() {
        assert_eq!(ty("contains('hello', 'll')"), Ty::Bool);
    }

    #[test]
    fn function_call_overload() {
        assert_eq!(ty("contains(github.event.labels, 'foo')"), Ty::Bool);
    }

    #[test]
    fn function_call_zero_arguments() {
        assert_eq!(ty("always()"), Ty::Bool);
    }

    #[test]
    fn function_call_variable_length_parameters() {
        assert_eq!(ty("format('hello {0} {1}', 42, true)"), Ty::String);
    }

    #[test]
    fn arguments_of_format_is_not_checked_when_first_argument_is_not_a_literal() {
        assert_eq!(ty("format(github.action, 1, 2, 3)"), Ty::String);
    }

    #[test]
    fn number_is_coerced_into_string() {
        assert_eq!(ty("startsWith('42foo', 42)"), Ty::Bool);
    }

    #[test]
    fn case_insensitive_comparison_for_function_name() {
        assert_eq!(ty("toJSON(fromjson(toJson(github)))"), Ty::String);
    }

    #[test]
    fn format_function_arguments_validation() {
        assert_eq!(
            ty(
                "format('{0}{0}{0} {1}{2}{1} {1}{2}{1}{2} {0} {1}{1}{1} {2}{2}{2} {0}{0}{0}{0} {0}', 1, 'foo', true)"
            ),
            Ty::String
        );
    }

    #[test]
    fn braces_not_for_placeholders_in_format_string_of_format_call() {
        assert_eq!(ty("format('{0} {} {x} {', 1)"), Ty::String);
    }

    /// Without the special function availability of the reference's case (M3.6).
    #[test]
    fn special_function() {
        assert_eq!(ty("success()"), Ty::Bool);
    }

    /// Without the special function availability of the reference's case (M3.6).
    #[test]
    fn non_special_function() {
        assert_eq!(ty("contains('hello, world', 'o, w')"), Ty::Bool);
    }

    /// Without the special function availability of the reference's case (M3.6).
    #[test]
    fn case_insensitive_special_function_name() {
        assert_eq!(ty("hashFiles('aaa.txt')"), Ty::String);
    }

    #[test]
    fn escaped_braces_in_format_string() {
        assert_eq!(ty("format('hello {{1}} {0}', 42)"), Ty::String);
    }

    /// The first `{{` is escaped, and `{0}` is not.
    #[test]
    fn format_specifier_is_escaped() {
        assert_eq!(ty("format('hello {{{0}', 'world')"), Ty::String);
    }

    #[test]
    fn case_with_single_predicate() {
        assert_eq!(ty("case(true, 'a', 'b')"), Ty::Any);
    }

    #[test]
    fn case_with_multiple_predicates() {
        assert_eq!(ty("case(true, 1, false, 2, 3)"), Ty::Any);
    }

    #[test]
    fn case_with_number_predicate_coerced_to_bool() {
        assert_eq!(ty("case(1, 'a', 'b')"), Ty::Any);
    }

    #[test]
    fn case_with_string_predicate_coerced_to_bool() {
        assert_eq!(ty("case('yes', 'a', 'b')"), Ty::Any);
    }

    #[test]
    fn case_with_null_predicate_coerced_to_bool() {
        assert_eq!(ty("case(null, 'a', 'b')"), Ty::Any);
    }

    #[test]
    fn case_case_insensitivity() {
        assert_eq!(ty("CASE(true, 1, 2)"), Ty::Any);
    }

    #[test]
    fn case_with_expression_as_predicate() {
        assert_eq!(ty("case(startsWith('hello', 'he'), 'yes', 'no')"), Ty::Any);
    }

    #[test]
    fn case_with_many_predicates() {
        assert_eq!(
            ty("case(false, 'a', false, 'b', false, 'c', true, 'd', 'default')"),
            Ty::Any
        );
    }
}

/// The reference's cases that read the result of a test function, such as `test().foo`, with
/// the result in `matrix` instead.
mod function_results_from_matrix {
    use super::*;

    fn ty_with(result: Ty, input: &str) -> Ty {
        ty_in(&env_with_matrix(strict([("test", result)])), input)
    }

    fn errors_with(result: Ty, input: &str) -> Vec<Error> {
        errors_in(&env_with_matrix(strict([("test", result)])), input)
    }

    #[test]
    fn object_property_dereference() {
        let result = Ty::loose([("bar", Ty::loose([("piyo", Ty::Bool)]))]);
        assert_eq!(ty_with(result, "matrix.test.bar.piyo"), Ty::Bool);
    }

    #[test]
    fn array_element_dereference() {
        let result = Ty::loose([("bar", Ty::array(Ty::Bool))]);
        assert_eq!(
            ty_with(result, "matrix.test.bar.*"),
            Ty::filtered_array(Ty::Bool)
        );
    }

    #[test]
    fn filter_object_property_by_array_element_dereference() {
        let result = Ty::loose([(
            "foo",
            Ty::array(Ty::loose([("bar", Ty::loose([("piyo", Ty::String)]))])),
        )]);
        assert_eq!(
            ty_with(result, "matrix.test.foo.*.bar.piyo"),
            Ty::filtered_array(Ty::String)
        );
    }

    #[test]
    fn filter_strict_object_property_by_array_element_dereference() {
        let result = Ty::strict([(
            "foo",
            Ty::array(Ty::strict([("bar", Ty::strict([("piyo", Ty::String)]))])),
        )]);
        assert_eq!(
            ty_with(result, "matrix.test.foo.*.bar.piyo"),
            Ty::filtered_array(Ty::String)
        );
    }

    #[test]
    fn object_property_index_access() {
        let result = Ty::loose([("bar", Ty::loose([("piyo", Ty::Bool)]))]);
        assert_eq!(ty_with(result, "matrix.test['bar']['piyo']"), Ty::Bool);
    }

    #[test]
    fn array_element_index_access() {
        assert_eq!(ty_with(Ty::array(Ty::Bool), "matrix.test[0]"), Ty::Bool);
    }

    #[test]
    fn index_access_to_dereferenced_array() {
        let result = Ty::loose([("foo", Ty::array(Ty::loose([("bar", Ty::String)])))]);
        assert_eq!(ty_with(result, "matrix.test.foo.*.bar[0]"), Ty::String);
    }

    #[test]
    fn coercing_two_objects_on_and_operator() {
        let env = env_with_matrix(strict([
            ("foo", Ty::strict([("foo", Ty::Number)])),
            ("bar", Ty::strict([("bar", Ty::Bool)])),
        ]));
        assert_eq!(
            ty_in(&env, "matrix.foo && matrix.bar"),
            Ty::strict([("foo", Ty::Number), ("bar", Ty::Bool)])
        );
    }

    #[test]
    fn coercing_two_objects_on_or_operator() {
        let env = env_with_matrix(strict([
            ("foo", Ty::strict([("foo", Ty::Number)])),
            ("bar", Ty::strict([("bar", Ty::Bool)])),
        ]));
        assert_eq!(
            ty_in(&env, "matrix.foo || matrix.bar"),
            Ty::strict([("foo", Ty::Number), ("bar", Ty::Bool)])
        );
    }

    #[test]
    fn case_insensitive_comparison_for_object_property() {
        let result = Ty::loose([("foo-bar_piyo", Ty::Null)]);
        assert_eq!(ty_with(result, "matrix.test.foo-Bar_PIYO"), Ty::Null);
    }

    #[test]
    fn map_object_dereference_on_array_filter() {
        let result = Ty::array(Ty::map(Ty::Number));
        assert_eq!(
            ty_with(result, "matrix.test.*.foo"),
            Ty::filtered_array(Ty::Number)
        );
    }

    #[test]
    fn map_object_dereference_on_object_filter() {
        let result = Ty::map(Ty::map(Ty::Number));
        assert_eq!(
            ty_with(result, "matrix.test.*.foo"),
            Ty::filtered_array(Ty::Number)
        );
    }

    #[test]
    fn strict_prop_check_at_object_property_filter_for_array_dereference() {
        let result = Ty::array(Ty::strict([("foo", Ty::Bool)]));
        assert_eq!(
            errors_with(result, "matrix.test.*.bar"),
            [error(
                &UNDEFINED_PROPERTY,
                0,
                "property \"bar\" is not defined in object type {foo: bool} as element of filtered array"
            )]
        );
    }

    #[test]
    fn array_element_is_not_object_for_filtering_array_dereference() {
        assert_eq!(
            errors_with(Ty::array(Ty::String), "matrix.test.*.bar"),
            [error(
                &INVALID_PROPERTY_ACCESS,
                0,
                "property filtered by \"bar\" at object filtering must be type of object but got \"string\""
            )]
        );
    }

    #[test]
    fn index_access_to_array_with_not_a_number() {
        assert_eq!(
            errors_with(Ty::array(Ty::String), "matrix.test['hi']"),
            [error(
                &INVALID_INDEX_ACCESS,
                12,
                "index access of array must be type of number but got \"string\""
            )]
        );
    }

    #[test]
    fn index_access_to_array_dereference_with_not_a_number() {
        assert_eq!(
            errors_with(Ty::array(Ty::String), "matrix.test.*['hi']"),
            [error(
                &INVALID_INDEX_ACCESS,
                14,
                "index access of array must be type of number but got \"string\""
            )]
        );
    }
    #[test]
    fn coerce_array_dereference_into_array_at_function_parameter() {
        let result = Ty::array(Ty::loose([("x", Ty::Number)]));
        assert_eq!(ty_with(result, "contains(matrix.test.*.x, 10)"), Ty::Bool);
    }
}

/// `TestExprSemanticsCheckError`.
mod semantics_check_error {
    use super::*;

    #[test]
    fn undefined_variable() {
        assert_eq!(
            errors("fooooo"),
            [error(
                &UNDEFINED_VARIABLE,
                0,
                format!("undefined variable \"fooooo\". available variables are {GLOBAL_NAMES}")
            )]
        );
    }

    #[test]
    fn receiver_of_object_dereference_is_not_an_object() {
        assert_eq!(
            errors("true.foo"),
            [error(
                &INVALID_PROPERTY_ACCESS,
                0,
                "receiver of object dereference \"foo\" must be type of object but got \"bool\""
            )]
        );
    }

    #[test]
    fn strict_prop_check() {
        assert_eq!(
            errors("github.foo"),
            [error(
                &UNDEFINED_PROPERTY,
                0,
                format!(
                    "property \"foo\" is not defined in object type {}",
                    global("github")
                )
            )]
        );
    }

    #[test]
    fn receiver_of_array_dereference_is_not_an_array() {
        assert_eq!(
            errors("true.*"),
            [error(
                &INVALID_OBJECT_FILTER,
                0,
                "receiver of object filtering `.*` must be type of array or object but got \"bool\""
            )]
        );
    }

    #[test]
    fn receiver_of_object_filter_is_not_an_object_which_has_object_element() {
        assert_eq!(
            errors("env.*"),
            [error(
                &INVALID_OBJECT_FILTER,
                0,
                "elements of object at receiver of object filtering `.*` must be type of object but got \"string\". the type of receiver was \"{string => string}\""
            )]
        );
    }

    #[test]
    fn receiver_of_object_filter_is_an_object_which_has_no_object_element() {
        assert_eq!(
            errors("runner.*"),
            [error(
                &INVALID_OBJECT_FILTER,
                0,
                format!(
                    "object type \"{RUNNER}\" cannot be filtered by object filtering `.*` since it has no object element"
                )
            )]
        );
    }

    #[test]
    fn index_access_to_invalid_value_number() {
        assert_eq!(
            errors("true[0]"),
            [error(
                &INVALID_INDEX_ACCESS,
                0,
                "index access operand must be type of object or array but got \"bool\""
            )]
        );
    }

    #[test]
    fn index_access_to_invalid_value_string() {
        assert_eq!(
            errors("true['hello']"),
            [error(
                &INVALID_INDEX_ACCESS,
                0,
                "index access operand must be type of object or array but got \"bool\""
            )]
        );
    }

    #[test]
    fn index_access_to_object_with_not_a_string() {
        assert_eq!(
            errors("env[0]"),
            [error(
                &INVALID_INDEX_ACCESS,
                4,
                "property access of object must be type of string but got \"number\""
            )]
        );
    }

    #[test]
    fn strict_prop_check_with_string_literal_index_access_to_object() {
        assert_eq!(
            errors("github['fooooo']"),
            [error(
                &UNDEFINED_PROPERTY,
                0,
                format!(
                    "property \"fooooo\" is not defined in object type {}",
                    global("github")
                )
            )]
        );
    }

    #[test]
    fn undefined_matrix_value() {
        let env = env_with_matrix(strict([("foo", Ty::Any)]));
        assert_eq!(
            errors_in(&env, "matrix.bar"),
            [error(
                &UNDEFINED_PROPERTY,
                0,
                "property \"bar\" is not defined in object type {foo: any}"
            )]
        );
    }

    #[test]
    fn matrix_value_with_untyped_matrix_values() {
        assert_eq!(
            errors("matrix.foooo"),
            [error(
                &UNDEFINED_PROPERTY,
                0,
                "property \"foooo\" is not defined in object type {}"
            )]
        );
    }

    #[test]
    fn undefined_step_id() {
        let mut env = TypeEnv::new();
        env.set_steps(steps_with("bar"));
        assert_eq!(
            errors_in(&env, "steps.foo"),
            [error(
                &UNDEFINED_PROPERTY,
                0,
                "property \"foo\" is not defined in object type {bar: {conclusion: string; outcome: string; outputs: object}}"
            )]
        );
    }

    #[test]
    fn invalid_property_in_step_object() {
        let mut env = TypeEnv::new();
        env.set_steps(steps_with("bar"));
        assert_eq!(
            errors_in(&env, "steps.bar.foo"),
            [error(
                &UNDEFINED_PROPERTY,
                0,
                "property \"foo\" is not defined in object type {conclusion: string; outcome: string; outputs: object}"
            )]
        );
    }

    #[test]
    fn step_output_value_without_typed_steps_outputs() {
        assert_eq!(
            errors("steps.foo.outputs"),
            [error(
                &UNDEFINED_PROPERTY,
                0,
                "property \"foo\" is not defined in object type {}"
            )]
        );
    }

    #[test]
    fn undefined_job_id_in_needs_context() {
        let mut env = TypeEnv::new();
        env.set_needs(needs_foo());
        assert_eq!(
            errors_in(&env, "needs.bar"),
            [error(
                &UNDEFINED_PROPERTY,
                0,
                "property \"bar\" is not defined in object type {foo: {outputs: {out1: string; out2: string}; result: string}}"
            )]
        );
    }

    #[test]
    fn undefined_output_in_needs_context() {
        let mut env = TypeEnv::new();
        env.set_needs(needs_foo());
        assert_eq!(
            errors_in(&env, "needs.foo.outputs.out3"),
            [error(
                &UNDEFINED_PROPERTY,
                0,
                "property \"out3\" is not defined in object type {out1: string; out2: string}"
            )]
        );
    }

    #[test]
    fn undefined_prop_in_needs_context() {
        let mut env = TypeEnv::new();
        env.set_needs(needs_foo());
        assert_eq!(
            errors_in(&env, "needs.foo.bar"),
            [error(
                &UNDEFINED_PROPERTY,
                0,
                "property \"bar\" is not defined in object type {outputs: {out1: string; out2: string}; result: string}"
            )]
        );
    }

    #[test]
    fn undefined_prop_in_untyped_needs_context() {
        assert_eq!(
            errors("needs.foo"),
            [error(
                &UNDEFINED_PROPERTY,
                0,
                "property \"foo\" is not defined in object type {}"
            )]
        );
    }

    #[test]
    fn bool_literal_in_upper_case() {
        assert_eq!(
            errors("TRUE"),
            [error(
                &UNDEFINED_VARIABLE,
                0,
                format!("undefined variable \"TRUE\". available variables are {GLOBAL_NAMES}")
            )]
        );
    }

    #[test]
    fn null_literal_in_upper_case() {
        assert_eq!(
            errors("NULL"),
            [error(
                &UNDEFINED_VARIABLE,
                0,
                format!("undefined variable \"NULL\". available variables are {GLOBAL_NAMES}")
            )]
        );
    }

    #[test]
    fn inputs_context_accepts_nothing_by_default() {
        assert_eq!(
            errors("inputs.hello"),
            [error(
                &UNDEFINED_PROPERTY,
                0,
                "property \"hello\" is not defined in object type {}"
            )]
        );
    }

    #[test]
    fn no_configuration_variable_is_allowed() {
        let mut env = TypeEnv::new();
        env.set_config_vars([] as [&str; 0]);
        assert_eq!(
            errors_in(&env, "vars.UNKNOWN_VARIABLE"),
            [error(
                &UNDEFINED_CONFIG_VARIABLE,
                0,
                "no configuration variable is allowed since the variables list is empty in actionlint.yaml. you may forget adding the variable \"unknown_variable\" to the list"
            )]
        );
    }

    #[test]
    fn unknown_configuration_variable() {
        let mut env = TypeEnv::new();
        env.set_config_vars(["FOO_BAR"]);
        assert_eq!(
            errors_in(&env, "vars.UNKNOWN_VARIABLE"),
            [error(
                &UNDEFINED_CONFIG_VARIABLE,
                0,
                "undefined configuration variable \"unknown_variable\". defined configuration variables in actionlint.yaml are \"FOO_BAR\""
            )]
        );
    }

    #[test]
    fn config_variable_naming_convention() {
        assert_eq!(
            errors("vars.FOO-BAR"),
            [error(
                &INVALID_CONFIG_VARIABLE_NAME,
                0,
                "configuration variable name \"foo-bar\" can only contain alphabets, decimal numbers, and '_'. note: see the convention at https://docs.github.com/en/actions/learn-github-actions/variables#naming-conventions-for-configuration-variables"
            )]
        );
    }

    #[test]
    fn config_variable_name_cannot_start_with_github_prefix() {
        assert_eq!(
            errors("vars.GITHUB_FOOOOOOOO"),
            [error(
                &INVALID_CONFIG_VARIABLE_NAME,
                0,
                "configuration variable name \"github_foooooooo\" must not start with the GITHUB_ prefix (case insensitive). note: see the convention at https://docs.github.com/en/actions/learn-github-actions/variables#naming-conventions-for-configuration-variables"
            )]
        );
    }
    #[test]
    fn undefined_function() {
        assert_eq!(
            errors("foooo()"),
            [error(
                &UNDEFINED_FUNCTION,
                0,
                format!("undefined function \"foooo\". available functions are {FUNCTION_NAMES}")
            )]
        );
    }

    #[test]
    fn wrong_number_of_arguments_at_function_call() {
        assert_eq!(
            errors("contains('foo')"),
            [
                error(
                    &WRONG_ARGUMENT_COUNT,
                    0,
                    "number of arguments is wrong. function \"contains(string, string) -> bool\" takes 2 parameters but 1 arguments are given"
                ),
                error(
                    &WRONG_ARGUMENT_COUNT,
                    0,
                    "number of arguments is wrong. function \"contains(array<any>, any) -> bool\" takes 2 parameters but 1 arguments are given"
                ),
            ]
        );
    }

    #[test]
    fn wrong_number_of_arguments_at_function_call_for_variable_length_parameters() {
        assert_eq!(
            errors("hashFiles()"),
            [error(
                &WRONG_ARGUMENT_COUNT,
                0,
                "number of arguments is wrong. function \"hashFiles(string...) -> string\" takes at least 1 parameters but 0 arguments are given"
            )]
        );
    }

    #[test]
    fn wrong_type_at_parameter() {
        assert_eq!(
            errors("startsWith('foo', null)"),
            [error(
                &WRONG_ARGUMENT_TYPE,
                18,
                "2nd argument of function call is not assignable. \"null\" cannot be assigned to \"string\". called function type is \"startsWith(string, string) -> bool\""
            )]
        );
    }

    #[test]
    fn wrong_type_at_parameter_of_overloaded_function() {
        assert_eq!(
            errors("contains('foo', null)"),
            [
                error(
                    &WRONG_ARGUMENT_TYPE,
                    16,
                    "2nd argument of function call is not assignable. \"null\" cannot be assigned to \"string\". called function type is \"contains(string, string) -> bool\""
                ),
                error(
                    &WRONG_ARGUMENT_TYPE,
                    9,
                    "1st argument of function call is not assignable. \"string\" cannot be assigned to \"array<any>\". called function type is \"contains(array<any>, any) -> bool\""
                ),
            ]
        );
    }

    #[test]
    fn wrong_type_at_rest_parameter() {
        assert_eq!(
            errors("hashFiles(null)"),
            [error(
                &WRONG_ARGUMENT_TYPE,
                10,
                "1st argument of function call is not assignable. \"null\" cannot be assigned to \"string\". called function type is \"hashFiles(string...) -> string\""
            )]
        );
    }

    #[test]
    fn wrong_type_at_rest_parameter_part2() {
        assert_eq!(
            errors("hashFiles('foo', null)"),
            [error(
                &WRONG_ARGUMENT_TYPE,
                17,
                "2nd argument of function call is not assignable. \"null\" cannot be assigned to \"string\". called function type is \"hashFiles(string...) -> string\""
            )]
        );
    }

    #[test]
    fn less_arguments_for_format_builtin_function_call() {
        assert_eq!(
            errors("format('format {0} {1}', 'foo')"),
            [error(
                &MISSING_FORMAT_ARGUMENT,
                0,
                "format string \"format {0} {1}\" contains placeholder {1} but only 1 arguments are given to format"
            )]
        );
    }

    #[test]
    fn more_arguments_for_format_builtin_function_call() {
        assert_eq!(
            errors("format('format {0} {1} {2}', 'foo', 1, true, null)"),
            [error(
                &UNUSED_FORMAT_ARGUMENT,
                0,
                "format string \"format {0} {1} {2}\" does not contain placeholder {3}. remove argument which is unused in the format string"
            )]
        );
    }

    #[test]
    fn unused_placeholder_in_format_string_of_format() {
        assert_eq!(
            errors("format('format {0} {2}', 1, 2, 3)"),
            [error(
                &UNUSED_FORMAT_ARGUMENT,
                0,
                "format string \"format {0} {2}\" does not contain placeholder {1}. remove argument which is unused in the format string"
            )]
        );
    }

    #[test]
    fn missing_placeholder_and_less_argument_at_the_same_time_in_format_string_of_format() {
        assert_eq!(
            errors("format('format {0} {2}', 1, 2)"),
            [
                error(
                    &UNUSED_FORMAT_ARGUMENT,
                    0,
                    "format string \"format {0} {2}\" does not contain placeholder {1}. remove argument which is unused in the format string"
                ),
                error(
                    &MISSING_FORMAT_ARGUMENT,
                    0,
                    "format string \"format {0} {2}\" contains placeholder {2} but only 2 arguments are given to format"
                ),
            ]
        );
    }

    #[test]
    fn zero_format_arguments_for_format_call() {
        assert_eq!(
            errors("format('hi')"),
            [error(
                &WRONG_ARGUMENT_COUNT,
                0,
                "number of arguments is wrong. function \"format(string, any...) -> string\" takes at least 2 parameters but 1 arguments are given"
            )]
        );
    }

    #[test]
    fn function_name_of_format_call_check_is_case_insensitive() {
        assert_eq!(
            errors("Format('{0}', 1, 2)"),
            [error(
                &UNUSED_FORMAT_ARGUMENT,
                0,
                "format string \"{0}\" does not contain placeholder {1}. remove argument which is unused in the format string"
            )]
        );
    }

    #[test]
    fn format_specifier_is_escaped() {
        assert_eq!(
            errors("format('hello {{0}}', 'world')"),
            [error(
                &UNUSED_FORMAT_ARGUMENT,
                0,
                "format string \"hello {{0}}\" does not contain placeholder {0}. remove argument which is unused in the format string"
            )]
        );
    }

    /// The first `{{` is escaped, and `{{0}}` is still escaped.
    #[test]
    fn format_specifier_is_still_escaped() {
        assert_eq!(
            errors("format('hello {{{{0}}', 'world')"),
            [error(
                &UNUSED_FORMAT_ARGUMENT,
                0,
                "format string \"hello {{{{0}}\" does not contain placeholder {0}. remove argument which is unused in the format string"
            )]
        );
    }

    #[test]
    fn type_mismatch_in_matrix_value() {
        let env = env_with_matrix(strict([("foo", Ty::Null)]));
        assert_eq!(
            errors_in(&env, "startsWith('hello', matrix.foo)"),
            [error(
                &WRONG_ARGUMENT_TYPE,
                20,
                "2nd argument of function call is not assignable. \"null\" cannot be assigned to \"string\". called function type is \"startsWith(string, string) -> bool\""
            )]
        );
    }

    #[test]
    fn case_with_too_few_arguments() {
        let message = |given| {
            format!(
                "number of arguments is wrong. function \"case(bool, any, any...) -> any\" takes at least 3 parameters but {given} arguments are given"
            )
        };
        assert_eq!(
            errors("case() || case(true) || case(true, 'a')"),
            [
                error(&WRONG_ARGUMENT_COUNT, 0, message(0)),
                error(&WRONG_ARGUMENT_COUNT, 10, message(1)),
                error(&WRONG_ARGUMENT_COUNT, 24, message(2)),
            ]
        );
    }

    #[test]
    fn case_with_even_number_of_arguments() {
        assert_eq!(
            errors("case(true, 'a', false, 'b') || case(true, 'a', false, 'b', true, 'c')"),
            [
                error(
                    &CASE_ARGUMENT_COUNT,
                    0,
                    "case() requires an odd number of arguments (pred/value pairs + default) but got 4"
                ),
                error(
                    &CASE_ARGUMENT_COUNT,
                    31,
                    "case() requires an odd number of arguments (pred/value pairs + default) but got 6"
                ),
            ]
        );
    }
}

/// `TestExprCompareOperandsCheck`. `matrix` holds a value of each type, and `null` is the
/// literal. Each case of `==` also checks `!=`, and each case of `<` also checks `<=`, `>`,
/// and `>=`, which the reference treats the same.
mod compare_operands_check {
    use super::*;

    fn operand(name: &str) -> String {
        if name == "null" {
            name.to_owned()
        } else {
            format!("matrix.{name}")
        }
    }

    fn type_name(name: &str) -> &str {
        match name {
            "array" => "array<number>",
            "array_2d" => "array<array<number>>",
            name => name,
        }
    }

    fn check_case(lhs: &str, op: &str, rhs: &str, ok: bool) {
        let env = env_with_matrix(strict([
            ("number", Ty::Number),
            ("string", Ty::String),
            ("bool", Ty::Bool),
            ("object", Ty::empty_loose()),
            ("array", Ty::array(Ty::Number)),
            ("array_2d", Ty::array(Ty::array(Ty::Number))),
            ("any", Ty::Any),
        ]));
        let ops: &[&str] = if op == "==" {
            &["==", "!="]
        } else {
            &["<", "<=", ">", ">="]
        };
        for op in ops {
            let input = format!("{} {op} {}", operand(lhs), operand(rhs));
            let (ty, errors) = env.check(&parse(&input));
            assert_eq!(ty, Ty::Bool, "{input}");
            if ok {
                assert!(errors.is_empty(), "{input}: {errors:?}");
            } else {
                let errors: Vec<Error> = errors
                    .iter()
                    .map(|err| (err.rule.id, err.start, err.message.clone()))
                    .collect();
                assert_eq!(
                    errors,
                    [error(
                        &INVALID_COMPARISON,
                        0,
                        format!(
                            "\"{}\" value cannot be compared to \"{}\" value with \"{op}\" operator",
                            type_name(lhs),
                            type_name(rhs)
                        )
                    )],
                    "{input}"
                );
            }
        }
    }

    macro_rules! cases {
        ($($name:ident: $lhs:literal $op:literal $rhs:literal => $ok:literal,)*) => {
            $(
                #[test]
                fn $name() {
                    check_case($lhs, $op, $rhs, $ok);
                }
            )*
        };
    }

    cases! {
        eq_number_any: "number" "==" "any" => true,
        eq_number_number: "number" "==" "number" => true,
        eq_number_string: "number" "==" "string" => true,
        eq_number_bool: "number" "==" "bool" => true,
        eq_number_null: "number" "==" "null" => true,
        eq_number_object: "number" "==" "object" => false,
        eq_number_array: "number" "==" "array" => false,
        eq_string_any: "string" "==" "any" => true,
        eq_string_number: "string" "==" "number" => true,
        eq_string_string: "string" "==" "string" => true,
        eq_string_bool: "string" "==" "bool" => true,
        eq_string_null: "string" "==" "null" => true,
        eq_string_object: "string" "==" "object" => false,
        eq_string_array: "string" "==" "array" => false,
        eq_bool_any: "bool" "==" "any" => true,
        eq_bool_number: "bool" "==" "number" => true,
        eq_bool_string: "bool" "==" "string" => true,
        eq_bool_bool: "bool" "==" "bool" => true,
        eq_bool_null: "bool" "==" "null" => true,
        eq_bool_object: "bool" "==" "object" => false,
        eq_bool_array: "bool" "==" "array" => false,
        eq_null_any: "null" "==" "any" => true,
        eq_null_number: "null" "==" "number" => true,
        eq_null_string: "null" "==" "string" => true,
        eq_null_bool: "null" "==" "bool" => true,
        eq_null_null: "null" "==" "null" => true,
        eq_null_object: "null" "==" "object" => true,
        eq_null_array: "null" "==" "array" => true,
        eq_object_any: "object" "==" "any" => true,
        eq_object_number: "object" "==" "number" => false,
        eq_object_string: "object" "==" "string" => false,
        eq_object_bool: "object" "==" "bool" => false,
        eq_object_null: "object" "==" "null" => true,
        eq_object_object: "object" "==" "object" => true,
        eq_object_array: "object" "==" "array" => false,
        eq_array_any: "array" "==" "any" => true,
        eq_array_number: "array" "==" "number" => false,
        eq_array_string: "array" "==" "string" => false,
        eq_array_bool: "array" "==" "bool" => false,
        eq_array_null: "array" "==" "null" => true,
        eq_array_object: "array" "==" "object" => false,
        eq_array_array: "array" "==" "array" => true,
        eq_array_array_2d: "array" "==" "array_2d" => false,
        eq_array_2d_any: "array_2d" "==" "any" => true,
        eq_array_2d_number: "array_2d" "==" "number" => false,
        eq_array_2d_string: "array_2d" "==" "string" => false,
        eq_array_2d_bool: "array_2d" "==" "bool" => false,
        eq_array_2d_null: "array_2d" "==" "null" => true,
        eq_array_2d_object: "array_2d" "==" "object" => false,
        eq_array_2d_array: "array_2d" "==" "array" => false,
        eq_array_2d_array_2d: "array_2d" "==" "array_2d" => true,
        eq_any_any: "any" "==" "any" => true,
        eq_any_number: "any" "==" "number" => true,
        eq_any_string: "any" "==" "string" => true,
        eq_any_bool: "any" "==" "bool" => true,
        eq_any_null: "any" "==" "null" => true,
        eq_any_object: "any" "==" "object" => true,
        eq_any_array: "any" "==" "array" => true,
        less_number_any: "number" "<" "any" => true,
        less_number_number: "number" "<" "number" => true,
        less_number_string: "number" "<" "string" => true,
        less_number_bool: "number" "<" "bool" => false,
        less_number_null: "number" "<" "null" => false,
        less_number_object: "number" "<" "object" => false,
        less_number_array: "number" "<" "array" => false,
        less_string_any: "string" "<" "any" => true,
        less_string_number: "string" "<" "number" => true,
        less_string_string: "string" "<" "string" => true,
        less_string_bool: "string" "<" "bool" => false,
        less_string_null: "string" "<" "null" => false,
        less_string_object: "string" "<" "object" => false,
        less_string_array: "string" "<" "array" => false,
        less_bool_any: "bool" "<" "any" => false,
        less_bool_number: "bool" "<" "number" => false,
        less_bool_string: "bool" "<" "string" => false,
        less_bool_bool: "bool" "<" "bool" => false,
        less_bool_null: "bool" "<" "null" => false,
        less_bool_object: "bool" "<" "object" => false,
        less_bool_array: "bool" "<" "array" => false,
        less_null_any: "null" "<" "any" => false,
        less_null_number: "null" "<" "number" => false,
        less_null_string: "null" "<" "string" => false,
        less_null_bool: "null" "<" "bool" => false,
        less_null_null: "null" "<" "null" => false,
        less_null_object: "null" "<" "object" => false,
        less_null_array: "null" "<" "array" => false,
        less_any_any: "any" "<" "any" => true,
        less_any_number: "any" "<" "number" => true,
        less_any_string: "any" "<" "string" => true,
        less_any_bool: "any" "<" "bool" => false,
        less_any_null: "any" "<" "null" => false,
        less_any_object: "any" "<" "object" => false,
        less_any_array: "any" "<" "array" => false,
        less_object_any: "object" "<" "any" => false,
        less_object_number: "object" "<" "number" => false,
        less_object_string: "object" "<" "string" => false,
        less_object_bool: "object" "<" "bool" => false,
        less_object_null: "object" "<" "null" => false,
        less_object_object: "object" "<" "object" => false,
        less_object_array: "object" "<" "array" => false,
        less_array_any: "array" "<" "any" => false,
        less_array_number: "array" "<" "number" => false,
        less_array_string: "array" "<" "string" => false,
        less_array_bool: "array" "<" "bool" => false,
        less_array_null: "array" "<" "null" => false,
        less_array_object: "array" "<" "object" => false,
        less_array_array: "array" "<" "array" => false,
        less_array_array_2d: "array" "<" "array_2d" => false,
    }
}

/// `TestExprSemanticsCheckerUpdateInputsMultipleTimes`: inputs from `workflow_call`, then
/// inputs from `workflow_dispatch`.
mod update_inputs_multiple_times {
    use super::*;

    fn inputs_after(first: ObjectTy, second: ObjectTy) -> Ty {
        let mut env = TypeEnv::new();
        env.add_inputs(first);
        env.add_dispatch_inputs(second);
        ty_in(&env, "inputs")
    }

    #[test]
    fn different_inputs_are_merged() {
        assert_eq!(
            inputs_after(strict([("foo", Ty::String)]), strict([("bar", Ty::Number)])),
            Ty::strict([("foo", Ty::String), ("bar", Ty::Number)])
        );
    }

    #[test]
    fn same_input_with_the_same_type_is_kept() {
        assert_eq!(
            inputs_after(strict([("foo", Ty::String)]), strict([("foo", Ty::String)])),
            Ty::strict([("foo", Ty::String)])
        );
    }

    #[test]
    fn same_input_with_different_types_is_any() {
        assert_eq!(
            inputs_after(strict([("foo", Ty::Bool)]), strict([("foo", Ty::Number)])),
            Ty::strict([("foo", Ty::Any)])
        );
    }
}

/// `TestExprSematincsCheckerUpdateDispatchInputsVarType`.
mod update_dispatch_inputs {
    use super::*;

    #[test]
    fn event_inputs_are_strict_strings() {
        let mut env = TypeEnv::new();
        env.add_dispatch_inputs(strict([("foo", Ty::Null)]));
        assert_eq!(
            ty_in(&env, "github.event.inputs"),
            Ty::strict([("foo", Ty::String)])
        );
        assert_eq!(ty_in(&env, "github.event.inputs.foo"), Ty::String);
        assert_eq!(ty_in(&env, "github['EVENT']['Inputs'].foo"), Ty::String);
        assert_eq!(ty_in(&env, "inputs.foo"), Ty::Null);
        assert_eq!(
            errors_in(&env, "github.event.inputs.bar"),
            [error(
                &UNDEFINED_PROPERTY,
                0,
                "property \"bar\" is not defined in object type {foo: string}"
            )]
        );
    }

    #[test]
    fn other_environments_keep_the_untyped_event() {
        let mut env = TypeEnv::new();
        env.add_dispatch_inputs(strict([("foo", Ty::Null)]));
        assert_eq!(ty("github.event.inputs"), Ty::Any);
        assert_eq!(ty("github.event.inputs.bar"), Ty::Any);
        assert_eq!(ty("github"), global("github"));
    }
}

/// The reference's `UpdateMatrix` and `UpdateSteps` tests check that the checker copies the
/// shared map of contexts before it changes it. A `TypeEnv` owns its overrides, so these tests
/// check that changing one environment leaves others unchanged.
mod environment_overrides {
    use super::*;

    #[test]
    fn override_in_a_clone_leaves_the_original_unchanged() {
        let job = env_with_matrix(strict([("os", Ty::String)]));
        let mut step = job.clone();
        step.set_steps(steps_with("build"));
        step.set_matrix(strict([("arch", Ty::String)]));

        assert_eq!(ty_in(&step, "matrix.arch"), Ty::String);
        assert_eq!(ty_in(&step, "steps.build.outcome"), Ty::String);
        assert_eq!(ty_in(&job, "matrix.os"), Ty::String);
        assert_eq!(
            errors_in(&job, "steps.build"),
            [error(
                &UNDEFINED_PROPERTY,
                0,
                "property \"build\" is not defined in object type {}"
            )]
        );
    }

    #[test]
    fn jobs_is_a_variable_only_when_set() {
        let mut env = TypeEnv::new();
        assert_eq!(
            errors_in(&env, "jobs"),
            [error(
                &UNDEFINED_VARIABLE,
                0,
                format!("undefined variable \"jobs\". available variables are {GLOBAL_NAMES}")
            )]
        );
        env.set_jobs(ObjectTy::new([] as [(&str, Ty); 0], Rest::Strict));
        assert_eq!(
            errors_in(&env, "foo"),
            [error(
                &UNDEFINED_VARIABLE,
                0,
                "undefined variable \"foo\". available variables are \"env\", \"github\", \"inputs\", \"job\", \"jobs\", \"matrix\", \"needs\", \"runner\", \"secrets\", \"steps\", \"strategy\", \"vars\""
            )]
        );
    }

    #[test]
    fn declared_secrets_are_strict_and_keep_the_built_in_ones() {
        let mut env = TypeEnv::new();
        env.set_secrets(strict([("token", Ty::String)]));
        assert_eq!(
            ty_in(&env, "secrets"),
            Ty::strict([
                ("actions_runner_debug", Ty::String),
                ("actions_step_debug", Ty::String),
                ("github_token", Ty::String),
                ("token", Ty::String),
            ])
        );
    }

    #[test]
    fn first_inputs_replace_the_empty_default() {
        let mut env = TypeEnv::new();
        env.add_inputs(strict([("a", Ty::Number)]));
        assert_eq!(ty_in(&env, "inputs"), Ty::strict([("a", Ty::Number)]));
    }
}

/// Where workflint's checker differs from the reference's on purpose.
/// Calls of built-in functions that the reference's tests do not cover. The reference's checker
/// gives the same messages and offsets.
mod function_calls {
    use super::*;

    #[test]
    fn undefined_function_keeps_its_spelling_and_its_arguments_are_not_checked() {
        assert_eq!(
            errors("Foo_Bar(bar)"),
            [error(
                &UNDEFINED_FUNCTION,
                0,
                format!("undefined function \"Foo_Bar\". available functions are {FUNCTION_NAMES}")
            )]
        );
    }

    #[test]
    fn arguments_of_a_defined_function_are_checked() {
        let (ty, errors) = TypeEnv::new().check(&parse("contains(foo, 'a')"));
        assert_eq!(ty, Ty::Bool);
        let errors: Vec<Error> = errors
            .iter()
            .map(|err| (err.rule.id, err.start, err.message.clone()))
            .collect();
        assert_eq!(
            errors,
            [error(
                &UNDEFINED_VARIABLE,
                9,
                format!("undefined variable \"foo\". available variables are {GLOBAL_NAMES}")
            )]
        );
    }

    #[test]
    fn failed_call_has_type_any() {
        let (ty, _) = TypeEnv::new().check(&parse("startsWith('foo', null)"));
        assert_eq!(ty, Ty::Any);
    }

    #[test]
    fn result_type_is_the_return_type_of_the_overload() {
        assert_eq!(
            errors("startsWith('a', 'b').foo"),
            [error(
                &INVALID_PROPERTY_ACCESS,
                0,
                "receiver of object dereference \"foo\" must be type of object but got \"bool\""
            )]
        );
        assert_eq!(ty("join(github.event.labels.*.name)"), Ty::String);
        assert_eq!(ty("fromJSON(github.event.body)"), Ty::Any);
    }

    #[test]
    fn argument_count_of_a_function_without_variadic_parameters() {
        assert_eq!(
            errors("toJSON() || toJSON(1, 2) || always(1)"),
            [
                error(
                    &WRONG_ARGUMENT_COUNT,
                    0,
                    "number of arguments is wrong. function \"toJSON(any) -> string\" takes 1 parameters but 0 arguments are given"
                ),
                error(
                    &WRONG_ARGUMENT_COUNT,
                    12,
                    "number of arguments is wrong. function \"toJSON(any) -> string\" takes 1 parameters but 2 arguments are given"
                ),
                error(
                    &WRONG_ARGUMENT_COUNT,
                    28,
                    "number of arguments is wrong. function \"always() -> bool\" takes 0 parameters but 1 arguments are given"
                ),
            ]
        );
    }

    #[test]
    fn each_overload_reports_its_own_failure() {
        assert_eq!(
            errors("join('a')"),
            [
                error(
                    &WRONG_ARGUMENT_COUNT,
                    0,
                    "number of arguments is wrong. function \"join(array<string>, string) -> string\" takes 2 parameters but 1 arguments are given"
                ),
                error(
                    &WRONG_ARGUMENT_TYPE,
                    5,
                    "1st argument of function call is not assignable. \"string\" cannot be assigned to \"array<string>\". called function type is \"join(array<string>) -> string\""
                ),
            ]
        );
        assert_eq!(
            errors("join(null, ',')"),
            [
                error(
                    &WRONG_ARGUMENT_TYPE,
                    5,
                    "1st argument of function call is not assignable. \"null\" cannot be assigned to \"array<string>\". called function type is \"join(array<string>, string) -> string\""
                ),
                error(
                    &WRONG_ARGUMENT_COUNT,
                    0,
                    "number of arguments is wrong. function \"join(array<string>) -> string\" takes 1 parameters but 2 arguments are given"
                ),
            ]
        );
    }

    #[test]
    fn only_the_first_wrong_argument_of_an_overload_is_reported() {
        assert_eq!(
            errors("contains(null, null)"),
            [
                error(
                    &WRONG_ARGUMENT_TYPE,
                    9,
                    "1st argument of function call is not assignable. \"null\" cannot be assigned to \"string\". called function type is \"contains(string, string) -> bool\""
                ),
                error(
                    &WRONG_ARGUMENT_TYPE,
                    9,
                    "1st argument of function call is not assignable. \"null\" cannot be assigned to \"array<any>\". called function type is \"contains(array<any>, any) -> bool\""
                ),
            ]
        );
    }

    #[test]
    fn wrong_argument_type_names_the_argument_with_an_ordinal() {
        let message = |ordinal: &str| {
            format!(
                "{ordinal} argument of function call is not assignable. \"null\" cannot be assigned to \"string\". called function type is \"hashFiles(string...) -> string\""
            )
        };
        assert_eq!(
            errors("hashFiles('a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', null)"),
            [error(&WRONG_ARGUMENT_TYPE, 60, message("11th"))]
        );
        assert_eq!(
            errors("hashFiles('a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', null)"),
            [error(&WRONG_ARGUMENT_TYPE, 65, message("12th"))]
        );
        assert_eq!(
            errors(
                "hashFiles('a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', null)"
            ),
            [error(&WRONG_ARGUMENT_TYPE, 110, message("21st"))]
        );
    }

    #[test]
    fn wrong_argument_type_prints_the_argument_type() {
        assert_eq!(
            errors("startsWith(env, 'a') || fromJSON(1 == 2) || format(true, 1)"),
            [
                error(
                    &WRONG_ARGUMENT_TYPE,
                    11,
                    "1st argument of function call is not assignable. \"{string => string}\" cannot be assigned to \"string\". called function type is \"startsWith(string, string) -> bool\""
                ),
                error(
                    &WRONG_ARGUMENT_TYPE,
                    33,
                    "1st argument of function call is not assignable. \"bool\" cannot be assigned to \"string\". called function type is \"fromJSON(string) -> any\""
                ),
                error(
                    &WRONG_ARGUMENT_TYPE,
                    51,
                    "1st argument of function call is not assignable. \"bool\" cannot be assigned to \"string\". called function type is \"format(string, any...) -> string\""
                ),
            ]
        );
    }

    /// The reference iterates a Go map here, so its order varies between runs. Diagnostics are
    /// sorted before they are printed, so the order does not reach the output.
    #[test]
    fn placeholders_beyond_the_arguments_are_reported_in_ascending_order() {
        assert_eq!(
            errors("format('{10} {9} {0}', 1)"),
            [
                error(
                    &MISSING_FORMAT_ARGUMENT,
                    0,
                    "format string \"{10} {9} {0}\" contains placeholder {9} but only 1 arguments are given to format"
                ),
                error(
                    &MISSING_FORMAT_ARGUMENT,
                    0,
                    "format string \"{10} {9} {0}\" contains placeholder {10} but only 1 arguments are given to format"
                ),
            ]
        );
    }

    /// Go's `strconv.Atoi` returns the largest `int` for a number beyond it.
    #[test]
    fn placeholder_beyond_the_largest_integer_is_the_largest_integer() {
        assert_eq!(
            errors("format('{99999999999999999999}', 1)"),
            [
                error(
                    &UNUSED_FORMAT_ARGUMENT,
                    0,
                    "format string \"{99999999999999999999}\" does not contain placeholder {0}. remove argument which is unused in the format string"
                ),
                error(
                    &MISSING_FORMAT_ARGUMENT,
                    0,
                    "format string \"{99999999999999999999}\" contains placeholder {9223372036854775807} but only 1 arguments are given to format"
                ),
            ]
        );
    }

    #[test]
    fn placeholder_with_a_leading_zero_is_its_number() {
        assert_eq!(
            errors("format('{01}', 1, 2)"),
            [error(
                &UNUSED_FORMAT_ARGUMENT,
                0,
                "format string \"{01}\" does not contain placeholder {0}. remove argument which is unused in the format string"
            )]
        );
    }

    #[test]
    fn case_with_a_default_in_place_of_the_last_value_has_an_even_count() {
        assert_eq!(
            errors("case(true, 'a', false, 'b', 'c', 'd')"),
            [error(
                &CASE_ARGUMENT_COUNT,
                0,
                "case() requires an odd number of arguments (pred/value pairs + default) but got 6"
            )]
        );
    }

    /// `docs/differences.md#bool-assignability`. GitHub converts the predicate to a boolean, so
    /// it accepts every type, while a `bool` parameter of a reusable workflow does not.
    #[test]
    fn case_predicate_accepts_every_type() {
        let env = env_with_matrix(strict([
            ("object", Ty::empty_strict()),
            ("array", Ty::array(Ty::Number)),
        ]));
        for input in [
            "case(env, 1, 2)",
            "case(matrix.object, 1, 2)",
            "case(matrix.array, 1, 2)",
        ] {
            assert_eq!(ty_in(&env, input), Ty::Any, "{input}");
        }
    }
}

mod differences {
    use super::*;

    /// `docs/differences.md#object-filter-mutation`. The reference marks the shared type of
    /// `matrix.os` as filtered while it checks the first expression.
    #[test]
    fn object_filter_does_not_change_the_type_for_the_next_expression() {
        let env = env_with_matrix(strict([("os", Ty::array(Ty::String))]));
        assert_eq!(ty_in(&env, "matrix.os.*"), Ty::filtered_array(Ty::String));
        assert_eq!(ty_in(&env, "matrix.os"), Ty::array(Ty::String));
        assert_eq!(
            errors_in(&env, "matrix.os.a"),
            [error(
                &INVALID_PROPERTY_ACCESS,
                0,
                "receiver of object dereference \"a\" must be type of object but got \"array<string>\""
            )]
        );
    }

    /// `docs/differences.md#literal-index-case`.
    #[test]
    fn literal_index_keys_are_case_insensitive() {
        let mut env = env_with_matrix(strict([("os", Ty::String)]));
        env.set_steps(steps_with("mystep"));
        assert_eq!(ty_in(&env, "steps['MyStep'].outcome"), Ty::String);
        assert_eq!(ty_in(&env, "matrix['OS']"), Ty::String);
        assert_eq!(ty_in(&env, "github['EVENT']"), Ty::empty_loose());
        assert_eq!(ty_in(&env, "github['Ref_Name']"), Ty::String);
    }

    /// `docs/differences.md#literal-index-case`.
    #[test]
    fn undefined_literal_index_key_keeps_its_spelling_in_the_message() {
        assert_eq!(
            errors("runner['OSS']"),
            [error(
                &UNDEFINED_PROPERTY,
                0,
                format!("property \"OSS\" is not defined in object type {RUNNER}")
            )]
        );
    }

    /// `docs/differences.md#bool-assignability`. `!` converts its operand to a boolean, so it
    /// accepts every type. The reference's check of the operand can never fail either.
    #[test]
    fn not_accepts_every_operand_type() {
        let env = env_with_matrix(strict([
            ("object", Ty::empty_strict()),
            ("array", Ty::array(Ty::Number)),
            ("any", Ty::Any),
        ]));
        for input in [
            "!null",
            "!1",
            "!'s'",
            "!true",
            "!matrix.object",
            "!matrix.array",
            "!matrix.any",
        ] {
            assert_eq!(ty_in(&env, input), Ty::Bool, "{input}");
        }
    }

    /// `docs/differences.md#comparison-precedence`. The reference parses
    /// `'a' < ('b' == true)` and reports that a string cannot be compared to a bool.
    #[test]
    fn comparison_precedence_follows_github() {
        assert_eq!(ty("'a' < 'b' == true"), Ty::Bool);
        assert_eq!(
            errors("1 == 2 < true"),
            [error(
                &INVALID_COMPARISON,
                5,
                "\"number\" value cannot be compared to \"bool\" value with \"<\" operator"
            )]
        );
    }
}

/// The byte ranges of errors: from the token the reference reports the error at to the end of
/// the node.
mod error_ranges {
    use super::*;

    fn ranges(env: &TypeEnv, input: &str) -> Vec<(u32, u32)> {
        let (_, errors) = env.check(&parse(input));
        errors.iter().map(|err| (err.start, err.end)).collect()
    }

    #[test]
    fn error_covers_the_node() {
        assert_eq!(ranges(&TypeEnv::new(), "github.foo"), [(0, 10)]);
        assert_eq!(ranges(&TypeEnv::new(), "env[0] == 1"), [(4, 5)]);
    }

    #[test]
    fn parenthesized_operand_starts_after_the_parenthesis() {
        let env = env_with_matrix(strict([("object", Ty::empty_loose())]));
        assert_eq!(ranges(&env, "(matrix.object) == 1"), [(1, 20)]);
    }

    #[test]
    fn argument_error_covers_the_argument() {
        assert_eq!(
            ranges(&TypeEnv::new(), "startsWith('foo', (null))"),
            [(19, 23)]
        );
    }

    #[test]
    fn call_error_covers_the_call() {
        assert_eq!(ranges(&TypeEnv::new(), "!toJSON()"), [(1, 9)]);
        assert_eq!(ranges(&TypeEnv::new(), "!foo(1)"), [(1, 7)]);
    }

    #[test]
    fn errors_are_in_the_reference_order() {
        assert_eq!(
            errors("foo[bar] == baz.*"),
            [
                error(
                    &UNDEFINED_VARIABLE,
                    4,
                    format!("undefined variable \"bar\". available variables are {GLOBAL_NAMES}")
                ),
                error(
                    &UNDEFINED_VARIABLE,
                    0,
                    format!("undefined variable \"foo\". available variables are {GLOBAL_NAMES}")
                ),
                error(
                    &UNDEFINED_VARIABLE,
                    12,
                    format!("undefined variable \"baz\". available variables are {GLOBAL_NAMES}")
                ),
            ]
        );
    }
}

/// `TestExprIsConstant`.
mod is_constant {
    use super::*;

    fn is_constant(input: &str) -> bool {
        parse(input).is_constant()
    }

    macro_rules! cases {
        ($($name:ident: $input:literal => $expected:literal,)*) => {
            $(
                #[test]
                fn $name() {
                    assert_eq!(is_constant($input), $expected, "{}", $input);
                }
            )*
        };
    }

    cases! {
        bool: "true" => true,
        int: "42" => true,
        float: "3.14" => true,
        null: "null" => true,
        string: "'hello'" => true,
        variable: "foo" => false,
        property: "foo.bar" => false,
        object_filter: "foo.*" => false,
        index: "foo[0]" => false,
        not_bool: "!true" => true,
        not_null: "!null" => true,
        not_variable: "!foo" => false,
        compare_literals: "1 == 2" => true,
        compare_strings: "'foo' != 'bar'" => true,
        compare_variable_left: "foo == 2" => false,
        compare_variable_right: "1 == foo" => false,
        logical_literals: "true && 42 || 13" => true,
        logical_variable_first: "foo && 42 || 13" => false,
        logical_variable_second: "true && foo || 13" => false,
        logical_variable_third: "true && 42 || foo" => false,
        nested_compare_in_and: "true && ('foo' == 'bar') && null" => true,
        nested_compare_in_or: "false || (3.14 != null) || !null" => true,
        constant_function: "startsWith('foo', 'f')" => true,
        constant_function_with_variable: "startsWith(foo, 'f')" => false,
        format_with_literals: "format('{0}{1}', 1.0, true)" => true,
        format_with_constant_calls: "format('{0}{1}', startsWith('foo', 'f'), 1.0 == !null)" => true,
        to_json_literal: "toJSON(true)" => true,
        to_json_variable: "toJSON(foo)" => false,
        success: "success()" => false,
        failure: "failure()" => false,
        hash_files: "hashFiles('foo', 'bar')" => false,
        unknown_function_with_from_json: "conains(fromJSON('[\"foo\"]'), 'foo')" => false,
        unknown_function: "unknownFunc(1, 2, 3)" => false,
        from_json_literal: "fromJSON('[1]')" => false,
        function_name_in_upper_case: "STARTSWITH('foo', 'f')" => true,
    }
}
