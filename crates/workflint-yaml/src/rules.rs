//! The rules whose diagnostics the YAML layer reports.

use workflint_diagnostics::{Category, RuleMeta, Severity};

/// The input is not valid YAML. No tree is built.
pub static YAML_SYNTAX_ERROR: RuleMeta = syntax_check(
    "yaml-syntax-error",
    "The input is not valid YAML",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/yaml-syntax-error.md",
);

/// An alias refers to an anchor that no earlier node defines. No tree is
/// built.
pub static UNKNOWN_ANCHOR: RuleMeta = syntax_check(
    "unknown-anchor",
    "An alias refers to an anchor that no earlier node defines",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/unknown-anchor.md",
);

/// An alias refers to a node that encloses it.
pub static RECURSIVE_ALIAS: RuleMeta = syntax_check(
    "recursive-alias",
    "An alias refers to a node that encloses it",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/recursive-alias.md",
);

/// No alias refers to an anchor.
pub static UNUSED_ANCHOR: RuleMeta = syntax_check(
    "unused-anchor",
    "No alias refers to an anchor",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/unused-anchor.md",
);

const fn syntax_check(
    id: &'static str,
    description: &'static str,
    docs_url: &'static str,
) -> RuleMeta {
    RuleMeta {
        id,
        description,
        kind: "syntax-check",
        category: Category::Correctness,
        default_severity: Severity::Error,
        docs_url,
    }
}

/// Every rule in this module, in the order of their IDs.
pub static ALL: [&RuleMeta; 4] = [
    &RECURSIVE_ALIAS,
    &UNKNOWN_ANCHOR,
    &UNUSED_ANCHOR,
    &YAML_SYNTAX_ERROR,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_rule_has_its_kind_and_links_the_page_named_after_its_id() {
        for rule in ALL {
            assert_eq!(rule.kind, "syntax-check", "{}", rule.id);
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
