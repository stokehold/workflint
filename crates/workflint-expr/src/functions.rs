//! The built-in functions of `workflint-data`, with their signatures converted to [`Ty`] once.

use std::cmp::Ordering;
use std::fmt;
use std::sync::LazyLock;

use workflint_data::BUILTIN_FUNCTIONS;

use crate::types::Ty;

/// A built-in function.
#[derive(Debug)]
pub(crate) struct Function {
    /// The name in lower case, which calls are looked up by.
    pub(crate) key: String,
    /// The overloads, in the order the checker tries them.
    pub(crate) overloads: Vec<Signature>,
}

/// One overload of a built-in function. It prints as the reference's `FuncSignature.String()`
/// does, such as `hashFiles(string...) -> string`.
#[derive(Debug)]
pub(crate) struct Signature {
    /// The name as GitHub's documentation writes it, such as `startsWith`.
    name: &'static str,
    params: Vec<Ty>,
    variadic: bool,
    pub(crate) ret: Ty,
}

/// Why an overload does not accept the arguments of a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mismatch {
    /// The call has too few or too many arguments.
    Count,
    /// The argument at this index has a type that its parameter does not accept. Only the
    /// first such argument is reported.
    Type(usize),
}

/// The built-in functions in the order of `BUILTIN_FUNCTIONS`, which is sorted by key.
static FUNCTIONS: LazyLock<Vec<Function>> = LazyLock::new(|| {
    BUILTIN_FUNCTIONS
        .iter()
        .map(|function| Function {
            key: function.name.to_ascii_lowercase(),
            overloads: function
                .overloads
                .iter()
                .map(|sig| Signature {
                    name: function.name,
                    params: sig.params.iter().map(Ty::from).collect(),
                    variadic: sig.variadic,
                    ret: Ty::from(&sig.ret),
                })
                .collect(),
        })
        .collect()
});

/// Returns the built-in function `name`, which must be lower case.
pub(crate) fn builtin(name: &str) -> Option<&'static Function> {
    let index = FUNCTIONS
        .binary_search_by(|function| function.key.as_str().cmp(name))
        .ok()?;
    FUNCTIONS.get(index)
}

/// Returns the keys of the built-in functions, for the message about an undefined function.
pub(crate) fn keys() -> impl Iterator<Item = &'static str> {
    FUNCTIONS.iter().map(|function| function.key.as_str())
}

impl Signature {
    /// The number of parameters. A variadic signature takes at least this many arguments.
    pub(crate) fn param_count(&self) -> usize {
        self.params.len()
    }

    /// Returns whether the last parameter repeats.
    pub(crate) fn is_variadic(&self) -> bool {
        self.variadic
    }

    /// Returns the parameter that takes the argument at `index`: the last one for the
    /// repeated arguments of a variadic signature.
    pub(crate) fn param(&self, index: usize) -> Option<&Ty> {
        match self.params.get(index) {
            Some(param) => Some(param),
            None if self.variadic => self.params.last(),
            None => None,
        }
    }

    /// Returns why the overload does not accept arguments of the types `args`, or `None` if
    /// it does. This is the reference's `checkFuncSignature` without the messages.
    pub(crate) fn mismatch(&self, args: &[Ty]) -> Option<Mismatch> {
        let count_fits = match self.params.len().cmp(&args.len()) {
            Ordering::Equal => true,
            Ordering::Less => self.variadic,
            Ordering::Greater => false,
        };
        if !count_fits {
            return Some(Mismatch::Count);
        }
        args.iter()
            .enumerate()
            .position(|(index, arg)| !self.param(index).is_some_and(|param| accepts(param, arg)))
            .map(Mismatch::Type)
    }
}

/// Returns whether a parameter of type `param` accepts an argument of type `arg`. A `bool`
/// parameter is a condition that GitHub converts to a boolean, as the predicates of `case()`,
/// so it accepts every type (`docs/differences.md#bool-assignability`).
fn accepts(param: &Ty, arg: &Ty) -> bool {
    match param {
        Ty::Bool => arg.converts_to_bool(),
        _ => param.accepts(arg),
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}(", self.name)?;
        for (i, param) in self.params.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{param}")?;
        }
        if self.variadic {
            f.write_str("...")?;
        }
        write!(f, ") -> {}", self.ret)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn printed(key: &str) -> Vec<String> {
        builtin(key)
            .map(|function| function.overloads.iter().map(ToString::to_string).collect())
            .unwrap_or_default()
    }

    #[test]
    fn signatures_print_as_the_reference_prints_them() {
        assert_eq!(
            printed("contains"),
            [
                "contains(string, string) -> bool",
                "contains(array<any>, any) -> bool",
            ]
        );
        assert_eq!(printed("hashfiles"), ["hashFiles(string...) -> string"]);
        assert_eq!(printed("case"), ["case(bool, any, any...) -> any"]);
        assert_eq!(printed("always"), ["always() -> bool"]);
    }

    #[test]
    fn builtin_finds_a_function_by_its_lower_case_name_only() {
        assert!(builtin("startswith").is_some());
        assert!(builtin("startsWith").is_none());
        assert!(builtin("foo").is_none());
    }

    #[test]
    fn keys_are_sorted() {
        let keys: Vec<&str> = keys().collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted);
    }
}
