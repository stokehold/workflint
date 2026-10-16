//! The rules whose diagnostics the YAML layer reports.

use workflint_diagnostics::{Category, RuleMeta, Severity};

/// The input is not valid YAML. No tree is built.
pub static YAML_SYNTAX_ERROR: RuleMeta = syntax_check(
    "yaml-syntax-error",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/yaml-syntax-error.md",
);

/// An alias refers to an anchor that no earlier node defines. No tree is
/// built.
pub static UNKNOWN_ANCHOR: RuleMeta = syntax_check(
    "unknown-anchor",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/unknown-anchor.md",
);

/// An alias refers to a node that encloses it.
pub static RECURSIVE_ALIAS: RuleMeta = syntax_check(
    "recursive-alias",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/recursive-alias.md",
);

/// No alias refers to an anchor.
pub static UNUSED_ANCHOR: RuleMeta = syntax_check(
    "unused-anchor",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/unused-anchor.md",
);

const fn syntax_check(id: &'static str, docs_url: &'static str) -> RuleMeta {
    RuleMeta {
        id,
        kind: "syntax-check",
        category: Category::Correctness,
        default_severity: Severity::Error,
        docs_url,
    }
}
