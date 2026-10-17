//! The registry: the rule metadata of every rule, from this crate and from the crates that
//! report diagnostics while parsing or checking (ADR 0008, ADR 0024, ADR 0028).

use std::sync::LazyLock;

use workflint_diagnostics::RuleMeta;

/// Every rule, sorted by rule ID.
static RULES: LazyLock<Vec<&'static RuleMeta>> = LazyLock::new(|| {
    let mut rules: Vec<&'static RuleMeta> = workflint_yaml::rules::ALL
        .iter()
        .chain(&workflint_syntax::rules::ALL)
        .chain(&workflint_expr::rules::ALL)
        .chain(&crate::rules::ALL)
        .copied()
        .collect();
    rules.sort_by_key(|rule| rule.id);
    rules
});

/// Returns the metadata of every rule, sorted by rule ID: the rules of this crate and the rules
/// that `workflint-yaml`, `workflint-syntax`, and `workflint-expr` report.
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
    use crate::kind;

    #[test]
    fn no_two_rules_share_an_id() {
        let ids: Vec<_> = rules().iter().map(|rule| rule.id).collect();
        let duplicates: Vec<_> = ids
            .windows(2)
            .filter(|pair| pair[0] == pair[1])
            .map(|pair| pair[0])
            .collect();
        assert!(duplicates.is_empty(), "{duplicates:?}");
    }

    #[test]
    fn the_rules_of_the_parsing_and_checking_crates_are_listed() {
        let lists = [
            &workflint_yaml::rules::ALL[..],
            &workflint_syntax::rules::ALL[..],
            &workflint_expr::rules::ALL[..],
        ];
        for rule in lists.into_iter().flatten() {
            assert!(
                rules().iter().any(|listed| std::ptr::eq(*listed, *rule)),
                "{} is not listed",
                rule.id
            );
        }
        let count: usize = lists.iter().map(|list| list.len()).sum();
        assert_eq!(rules().len(), count + crate::rules::ALL.len());
    }

    #[test]
    fn the_listing_is_sorted_by_rule_id() {
        let ids: Vec<_> = rules().iter().map(|rule| rule.id).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn find_looks_up_a_rule_by_id() {
        assert_eq!(
            find("yaml-syntax-error").map(|rule| rule.kind),
            Some("syntax-check")
        );
        assert_eq!(
            find("undefined-variable").map(|rule| rule.kind),
            Some("expression")
        );
        assert!(find("no-such-rule").is_none());
        for rule in rules() {
            assert!(find(rule.id).is_some_and(|found| std::ptr::eq(found, *rule)));
        }
    }

    #[test]
    fn every_rule_has_a_one_line_description_without_a_final_period() {
        for rule in rules() {
            let description = rule.description;
            assert!(!description.is_empty(), "{}", rule.id);
            assert!(!description.contains('\n'), "{}", rule.id);
            assert!(!description.ends_with('.'), "{}", rule.id);
        }
    }

    #[test]
    fn every_rule_maps_to_a_known_kind() {
        for rule in rules() {
            assert!(
                kind::ALL.iter().any(|kind| kind.name == rule.kind),
                "{}: {}",
                rule.id,
                rule.kind
            );
        }
    }
}
