//! The rules whose diagnostics the expression lexer, parser, type checker, and untrusted-input
//! check report.

use workflint_diagnostics::{Category, RuleMeta, Severity};

/// An expression in `${{ }}` cannot be lexed or parsed.
pub static INVALID_EXPRESSION_SYNTAX: RuleMeta = RuleMeta {
    id: "invalid-expression-syntax",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-expression-syntax.md",
};

/// A number literal in an expression has no value as a double, such as `1e400`.
pub static INVALID_NUMBER_LITERAL: RuleMeta = RuleMeta {
    id: "invalid-number-literal",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-number-literal.md",
};

/// An expression reads a variable that is not a context, such as `fooooo`.
pub static UNDEFINED_VARIABLE: RuleMeta = RuleMeta {
    id: "undefined-variable",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/undefined-variable.md",
};

/// An expression reads a property that its strict object type does not have, such as
/// `github.foo` or `steps.missing`.
pub static UNDEFINED_PROPERTY: RuleMeta = RuleMeta {
    id: "undefined-property",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/undefined-property.md",
};

/// An expression reads a property with a dot from a value that is not an object, such as
/// `true.foo`.
pub static INVALID_PROPERTY_ACCESS: RuleMeta = RuleMeta {
    id: "invalid-property-access",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-property-access.md",
};

/// An index has the wrong type for its operand, or the operand is neither an object nor an
/// array, such as `env[0]` or `true[0]`.
pub static INVALID_INDEX_ACCESS: RuleMeta = RuleMeta {
    id: "invalid-index-access",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-index-access.md",
};

/// An object filter `.*` applies to a value without object elements, such as `env.*`.
pub static INVALID_OBJECT_FILTER: RuleMeta = RuleMeta {
    id: "invalid-object-filter",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-object-filter.md",
};

/// A comparison has operands that GitHub cannot compare, such as an object and a number.
pub static INVALID_COMPARISON: RuleMeta = RuleMeta {
    id: "invalid-comparison",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-comparison.md",
};

/// The name in `vars.NAME` breaks GitHub's naming rules for configuration variables.
pub static INVALID_CONFIG_VARIABLE_NAME: RuleMeta = RuleMeta {
    id: "invalid-config-variable-name",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-config-variable-name.md",
};

/// `vars.NAME` reads a configuration variable that the configuration does not list.
pub static UNDEFINED_CONFIG_VARIABLE: RuleMeta = RuleMeta {
    id: "undefined-config-variable",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/undefined-config-variable.md",
};

/// An expression calls a function that is not built in, such as `foo()`.
pub static UNDEFINED_FUNCTION: RuleMeta = RuleMeta {
    id: "undefined-function",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/undefined-function.md",
};

/// A call of a built-in function has the wrong number of arguments for an overload, such as
/// `contains('foo')`.
pub static WRONG_ARGUMENT_COUNT: RuleMeta = RuleMeta {
    id: "wrong-argument-count",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/wrong-argument-count.md",
};

/// An argument of a built-in function has a type that its parameter does not accept, such as
/// `startsWith('foo', null)`.
pub static WRONG_ARGUMENT_TYPE: RuleMeta = RuleMeta {
    id: "wrong-argument-type",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/wrong-argument-type.md",
};

/// The literal format string of `format()` has no placeholder for an argument, such as
/// `format('{0}', 1, 2)`.
pub static UNUSED_FORMAT_ARGUMENT: RuleMeta = RuleMeta {
    id: "unused-format-argument",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/unused-format-argument.md",
};

/// The literal format string of `format()` has a placeholder beyond its arguments, such as
/// `format('{0} {1}', 1)`.
pub static MISSING_FORMAT_ARGUMENT: RuleMeta = RuleMeta {
    id: "missing-format-argument",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/missing-format-argument.md",
};

/// `case()` has an even number of arguments, so it has no default value, such as
/// `case(true, 'a', false, 'b')`.
pub static CASE_ARGUMENT_COUNT: RuleMeta = RuleMeta {
    id: "case-argument-count",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/case-argument-count.md",
};

/// An expression reads a context that its workflow key does not allow, such as `env` in
/// `jobs.<job_id>.if`.
pub static CONTEXT_NOT_ALLOWED: RuleMeta = RuleMeta {
    id: "context-not-allowed",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/context-not-allowed.md",
};

/// An expression calls a special function that its workflow key does not allow, such as
/// `success()` in `jobs.<job_id>.env`.
pub static FUNCTION_NOT_ALLOWED: RuleMeta = RuleMeta {
    id: "function-not-allowed",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/function-not-allowed.md",
};

/// The string literal passed to `fromJSON()` is not valid JSON, such as `fromJSON('[1,')`.
pub static INVALID_JSON_LITERAL: RuleMeta = RuleMeta {
    id: "invalid-json-literal",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-json-literal.md",
};

/// A script reads a value that someone outside the repository can set, such as
/// `github.event.issue.title`, so the value can inject code.
pub static UNTRUSTED_INPUT: RuleMeta = RuleMeta {
    id: "untrusted-input",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/untrusted-input.md",
};

/// A script reads an object filter, such as `github.event.*.body`, that extracts several
/// values that someone outside the repository can set.
pub static UNTRUSTED_OBJECT_FILTER: RuleMeta = RuleMeta {
    id: "untrusted-object-filter",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/untrusted-object-filter.md",
};

/// Every rule in this module, in the order of their IDs.
pub static ALL: [&RuleMeta; 21] = [
    &CASE_ARGUMENT_COUNT,
    &CONTEXT_NOT_ALLOWED,
    &FUNCTION_NOT_ALLOWED,
    &INVALID_COMPARISON,
    &INVALID_CONFIG_VARIABLE_NAME,
    &INVALID_EXPRESSION_SYNTAX,
    &INVALID_INDEX_ACCESS,
    &INVALID_JSON_LITERAL,
    &INVALID_NUMBER_LITERAL,
    &INVALID_OBJECT_FILTER,
    &INVALID_PROPERTY_ACCESS,
    &MISSING_FORMAT_ARGUMENT,
    &UNDEFINED_CONFIG_VARIABLE,
    &UNDEFINED_FUNCTION,
    &UNDEFINED_PROPERTY,
    &UNDEFINED_VARIABLE,
    &UNTRUSTED_INPUT,
    &UNTRUSTED_OBJECT_FILTER,
    &UNUSED_FORMAT_ARGUMENT,
    &WRONG_ARGUMENT_COUNT,
    &WRONG_ARGUMENT_TYPE,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_rule_has_its_kind_and_links_the_page_named_after_its_id() {
        for rule in ALL {
            assert_eq!(rule.kind, "expression", "{}", rule.id);
            assert_eq!(
                rule.docs_url,
                format!(
                    "https://github.com/stokehold/workflint/blob/main/docs/rules/{}.md",
                    rule.id
                )
            );
        }
    }

    #[test]
    fn the_list_is_sorted_by_id() {
        let ids: Vec<_> = ALL.iter().map(|rule| rule.id).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted);
    }
}
