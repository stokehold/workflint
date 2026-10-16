//! Checks the built-in function signatures against the reference's `BuiltinFuncSignatures` in
//! the vendored `expr_sema.go`, and against the rules of `TestExprBuiltinFunctionSignatures` in
//! `expr_sema_test.go`.

#![allow(clippy::panic)]

mod go_source;

use go_source::{FuncSignature, Ty, builtin_func_signatures};
use workflint_data::{BUILTIN_FUNCTIONS, Function, builtin_function};

/// Each overload of each function, printed as the reference's `FuncSignature.String()` prints
/// it, after the lower-case name that the table is keyed by. A constant function ends with
/// `(constant)`.
fn printed(key: &str, sig: &FuncSignature) -> String {
    let constant = if sig.is_const_func { " (constant)" } else { "" };
    format!("{key}: {sig}{constant}")
}

fn workflint_signatures(function: &Function) -> Vec<FuncSignature> {
    function
        .overloads
        .iter()
        .map(|sig| FuncSignature {
            name: function.name.to_owned(),
            ret: Some(Ty::from(&sig.ret)),
            params: sig.params.iter().map(Ty::from).collect(),
            variable_length_params: sig.variadic,
            is_const_func: function.constant,
        })
        .collect()
}

#[test]
fn functions_match_the_reference_table() {
    let reference: Vec<String> = builtin_func_signatures()
        .iter()
        .flat_map(|(key, sigs)| sigs.iter().map(|sig| printed(key, sig)))
        .collect();
    let workflint: Vec<String> = BUILTIN_FUNCTIONS
        .iter()
        .flat_map(|function| {
            let key = function.name.to_ascii_lowercase();
            workflint_signatures(function)
                .iter()
                .map(|sig| printed(&key, sig))
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(workflint, reference);
}

#[test]
fn the_reference_table_prints_as_the_reference_does() {
    let reference = builtin_func_signatures();
    let printed: Vec<String> = reference["contains"]
        .iter()
        .chain(&reference["hashfiles"])
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        printed,
        [
            "contains(string, string) -> bool",
            "contains(array<any>, any) -> bool",
            "hashFiles(string...) -> string",
        ]
    );
}

#[test]
fn functions_are_sorted_by_lower_case_name() {
    let names: Vec<String> = BUILTIN_FUNCTIONS
        .iter()
        .map(|function| function.name.to_ascii_lowercase())
        .collect();
    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(names, sorted);
}

// `TestExprBuiltinFunctionSignatures`: every function has an overload, and a variadic overload
// has a parameter to repeat.
#[test]
fn every_function_has_an_overload_and_every_variadic_overload_a_parameter() {
    for function in BUILTIN_FUNCTIONS {
        assert!(!function.overloads.is_empty(), "{}", function.name);
        for sig in function.overloads {
            assert!(!sig.variadic || !sig.params.is_empty(), "{}", function.name);
        }
    }
}

#[test]
fn builtin_function_finds_every_function_by_lower_case_name() {
    for function in BUILTIN_FUNCTIONS {
        let key = function.name.to_ascii_lowercase();
        assert_eq!(builtin_function(&key), Some(function), "{key}");
    }
}

#[test]
fn builtin_function_does_not_find_a_name_with_upper_case_letters() {
    assert_eq!(builtin_function("startsWith"), None);
    assert_eq!(builtin_function("startswith2"), None);
}
