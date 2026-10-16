//! The rules whose diagnostics the expression lexer, parser, and type checker report.

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
