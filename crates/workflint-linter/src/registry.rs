//! The registry of every rule: the rules that [`workflint_rules::registry`] lists and the rules
//! that the driver reports (ADR 0034).

use std::sync::LazyLock;

use workflint_diagnostics::RuleMeta;

/// Every rule, sorted by rule ID.
static RULES: LazyLock<Vec<&'static RuleMeta>> = LazyLock::new(|| {
    let mut rules: Vec<&'static RuleMeta> = workflint_rules::registry::rules()
        .iter()
        .chain(&crate::rules::ALL)
        .copied()
        .collect();
    rules.sort_by_key(|rule| rule.id);
    rules
});

/// Returns the metadata of every rule, sorted by rule ID: the rules of
/// [`workflint_rules::registry::rules`] and those of [`crate::rules`].
#[must_use]
pub fn rules() -> &'static [&'static RuleMeta] {
    &RULES
}

/// Returns the metadata of the rule with the ID `id`, or `None` if no rule has it.
#[must_use]
pub fn find(id: &str) -> Option<&'static RuleMeta> {
    let rules = rules();
    rules
        .binary_search_by_key(&id, |rule| rule.id)
        .ok()
        .and_then(|index| rules.get(index).copied())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{INTERNAL_ERROR, INTERNAL_ERROR_KIND};

    #[test]
    fn every_rule_of_the_rules_crate_and_of_the_driver_is_listed_once() {
        let lists = [workflint_rules::registry::rules(), &crate::rules::ALL[..]];
        for rule in lists.into_iter().flatten() {
            assert!(
                rules().iter().any(|listed| std::ptr::eq(*listed, *rule)),
                "{} is not listed",
                rule.id
            );
        }
        let count: usize = lists.iter().map(|list| list.len()).sum();
        assert_eq!(rules().len(), count);
        let ids: Vec<_> = rules().iter().map(|rule| rule.id).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn find_looks_up_the_rules_of_both() {
        assert!(find("internal-error").is_some_and(|rule| rule.id == INTERNAL_ERROR.id));
        assert_eq!(
            find("yaml-syntax-error").map(|rule| rule.kind),
            Some("syntax-check")
        );
        assert!(find("no-such-rule").is_none());
    }

    #[test]
    fn every_rule_maps_to_an_actionlint_kind_or_to_internal_error() {
        for rule in rules() {
            assert!(
                workflint_rules::kind::ALL
                    .iter()
                    .chain([&&INTERNAL_ERROR_KIND])
                    .any(|kind| kind.name == rule.kind),
                "{}: {}",
                rule.id,
                rule.kind
            );
        }
    }
}
