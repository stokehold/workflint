//! The rules whose diagnostics the workflow parser reports (ADR 0024).
//!
//! Each section parser reports through these values, so a rule has one
//! `RuleMeta` however many sections report it.

use workflint_diagnostics::{Category, RuleMeta, Severity};

/// The file has no YAML document, or only comments: `workflow is empty`.
pub static EMPTY_WORKFLOW: RuleMeta = syntax_check(
    "empty-workflow",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/empty-workflow.md",
);

/// A mapping has a `<<` key, quoted or not. GitHub Actions does not apply
/// YAML merge keys, so the entry is skipped.
pub static MERGE_KEY_UNSUPPORTED: RuleMeta = syntax_check(
    "merge-key-unsupported",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/merge-key-unsupported.md",
);

/// A required section or key is missing, such as `on` or `jobs` at the
/// workflow root.
pub static MISSING_REQUIRED_KEY: RuleMeta = syntax_check(
    "missing-required-key",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/missing-required-key.md",
);

/// A mapping has a key that its section does not define.
pub static UNEXPECTED_KEY: RuleMeta = syntax_check(
    "unexpected-key",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/unexpected-key.md",
);

/// A mapping repeats a key. The first entry is kept, and the duplicate's
/// value is not parsed.
pub static DUPLICATE_KEY: RuleMeta = syntax_check(
    "duplicate-key",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/duplicate-key.md",
);

/// A mapping or sequence that must have entries is empty or null.
pub static EMPTY_SECTION: RuleMeta = syntax_check(
    "empty-section",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/empty-section.md",
);

/// A string that must not be empty is empty.
pub static EMPTY_STRING: RuleMeta = syntax_check(
    "empty-string",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/empty-string.md",
);

/// A node has the wrong kind, such as a sequence where a mapping is
/// expected.
pub static UNEXPECTED_NODE_KIND: RuleMeta = syntax_check(
    "unexpected-node-kind",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/unexpected-node-kind.md",
);

/// A boolean, integer, or float value has the wrong tag or does not parse.
pub static INVALID_LITERAL_VALUE: RuleMeta = syntax_check(
    "invalid-literal-value",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-literal-value.md",
);

/// A value that must be a literal or a single `${{ }}` expression is other
/// text.
pub static EXPECTED_SINGLE_EXPRESSION: RuleMeta = syntax_check(
    "expected-single-expression",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/expected-single-expression.md",
);

/// The workflow has more YAML nodes, with its aliases expanded, than GitHub
/// reads. GitHub rejects it with `Maximum YAML nodes exceeded`.
pub static TOO_MANY_YAML_NODES: RuleMeta = syntax_check(
    "too-many-yaml-nodes",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/too-many-yaml-nodes.md",
);

/// Every rule in this module, in the order of their IDs.
pub static ALL: [&RuleMeta; 11] = [
    &DUPLICATE_KEY,
    &EMPTY_SECTION,
    &EMPTY_STRING,
    &EMPTY_WORKFLOW,
    &EXPECTED_SINGLE_EXPRESSION,
    &INVALID_LITERAL_VALUE,
    &MERGE_KEY_UNSUPPORTED,
    &MISSING_REQUIRED_KEY,
    &TOO_MANY_YAML_NODES,
    &UNEXPECTED_KEY,
    &UNEXPECTED_NODE_KIND,
];

/// A parse-stage rule of kind `syntax-check`. `docs_url` is the page that
/// M4.x adds (ADR 0024).
const fn syntax_check(id: &'static str, docs_url: &'static str) -> RuleMeta {
    RuleMeta {
        id,
        kind: "syntax-check",
        category: Category::Correctness,
        default_severity: Severity::Error,
        docs_url,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_rule_links_the_page_named_after_its_id() {
        for rule in ALL {
            assert_eq!(
                rule.docs_url,
                format!(
                    "https://github.com/stokehold/workflint/blob/main/docs/rules/{}.md",
                    rule.id
                )
            );
            assert_eq!(rule.kind, "syntax-check");
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
