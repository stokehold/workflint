//! The rules whose diagnostics the expression lexer and parser report.

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
