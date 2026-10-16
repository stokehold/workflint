//! The signatures of the built-in functions.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `BuiltinFuncSignatures` in
//! `expr_sema.go`, lines 132-255. Until M7 replaces it with a generated file (ADR 0011), keep
//! it equal to that table. The test `functions_match_the_reference_table` compares them.

use core::cmp::Ordering;

use crate::types::TypeDesc as T;

/// A built-in function, such as `contains` or `format`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Function {
    /// The name as GitHub's documentation writes it, such as `startsWith`. Expressions compare
    /// function names without regard to case.
    pub name: &'static str,
    /// The overloads, in the order the checker tries them.
    pub overloads: &'static [Signature],
    /// Whether a call returns the same value in every run when all its arguments do, as for
    /// `startsWith('foo', 'f')`. `success()` and `hashFiles()` depend on the run.
    pub constant: bool,
}

/// The parameter and return types of one overload of a built-in function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Signature {
    /// The parameter types. A `bool` parameter is a condition: GitHub converts the argument
    /// to a boolean, so the checker accepts every type for it
    /// (`docs/differences.md#bool-assignability`).
    pub params: &'static [T],
    /// Whether the last parameter repeats. It must appear at least once, so `hashFiles()`
    /// takes at least one argument. A variadic signature has at least one parameter.
    pub variadic: bool,
    /// The return type.
    pub ret: T,
}

const fn sig(params: &'static [T], ret: T) -> Signature {
    Signature {
        params,
        variadic: false,
        ret,
    }
}

const fn variadic(params: &'static [T], ret: T) -> Signature {
    Signature {
        params,
        variadic: true,
        ret,
    }
}

const fn function(name: &'static str, overloads: &'static [Signature], constant: bool) -> Function {
    Function {
        name,
        overloads,
        constant,
    }
}

/// The built-in functions, sorted by name in lower case.
pub static BUILTIN_FUNCTIONS: &[Function] = &[
    function("always", &[sig(&[], T::Bool)], false),
    function("cancelled", &[sig(&[], T::Bool)], false),
    // case(pred1, value1, pred2, value2, ..., default)
    function(
        "case",
        &[variadic(&[T::Bool, T::Any, T::Any], T::Any)],
        true,
    ),
    function(
        "contains",
        &[
            sig(&[T::String, T::String], T::Bool),
            sig(&[T::Array(&T::Any), T::Any], T::Bool),
        ],
        true,
    ),
    function("endsWith", &[sig(&[T::String, T::String], T::Bool)], true),
    function("failure", &[sig(&[], T::Bool)], false),
    function("format", &[variadic(&[T::String, T::Any], T::String)], true),
    // `workflint-expr` types the result of a literal argument (M3.9).
    function("fromJSON", &[sig(&[T::String], T::Any)], false),
    function("hashFiles", &[variadic(&[T::String], T::String)], false),
    function(
        "join",
        &[
            sig(&[T::Array(&T::String), T::String], T::String),
            // Without the separator, GitHub joins the elements with `,`.
            sig(&[T::Array(&T::String)], T::String),
        ],
        true,
    ),
    function("startsWith", &[sig(&[T::String, T::String], T::Bool)], true),
    function("success", &[sig(&[], T::Bool)], false),
    function("toJSON", &[sig(&[T::Any], T::String)], true),
];

/// Returns the built-in function `name`, which must be lower case, such as `startswith`.
#[must_use]
pub fn builtin_function(name: &str) -> Option<&'static Function> {
    let index = BUILTIN_FUNCTIONS
        .binary_search_by(|function| cmp_lowercase(function.name, name))
        .ok()?;
    BUILTIN_FUNCTIONS.get(index)
}

/// Compares `name` in lower case with `lower`, without allocating.
fn cmp_lowercase(name: &str, lower: &str) -> Ordering {
    name.bytes()
        .map(|b| b.to_ascii_lowercase())
        .cmp(lower.bytes())
}
