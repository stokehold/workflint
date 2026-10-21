//! Maps workflint rule IDs to actionlint kinds, so that workflint's output
//! compares with the reference's `.out` files (ADR 0003, ADR 0008).
//!
//! A rule ID's kind comes from the rule registry in `workflint-rules`. The
//! rule IDs of the parity checklist that no crate defines yet keep their kind
//! in `tests/conformance/rule-kinds.toml`, and each task that adds a rule
//! moves its IDs out of the table. The conformance target checks that every
//! checklist rule ID is in exactly one of the two, with the checklist's kind.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use regex::Regex;
use workflint_diagnostics::RuleMeta;

use super::output::Diagnostic;

static TABLE: LazyLock<BTreeMap<String, String>> = LazyLock::new(|| {
    parse_table(include_str!("../../../tests/conformance/rule-kinds.toml"))
        .expect("tests/conformance/rule-kinds.toml is valid")
});

/// The actionlint kind that `rule_id` maps to, or `None` for an unknown ID.
///
/// This is the only place that knows the mapping: the registry, then the
/// table of the rule IDs that have no rule yet.
#[must_use]
pub fn actionlint_kind(rule_id: &str) -> Option<&'static str> {
    workflint_rules::registry::find(rule_id)
        .map(|rule| rule.kind)
        .or_else(|| TABLE.get(rule_id).map(String::as_str))
}

/// Every actionlint kind, sorted: the kinds that `workflint_rules::kind`
/// describes.
#[must_use]
pub fn kinds() -> BTreeSet<&'static str> {
    workflint_rules::kind::ALL
        .iter()
        .map(|kind| kind.name)
        .collect()
}

/// Replaces the rule ID in each diagnostic's brackets with its kind.
///
/// # Errors
///
/// Returns a message that names the first rule ID with no kind.
pub fn to_kinds(diagnostics: Vec<Diagnostic>) -> Result<Vec<Diagnostic>, String> {
    to_kinds_with(diagnostics, actionlint_kind)
}

fn to_kinds_with<'a>(
    diagnostics: Vec<Diagnostic>,
    kind: impl Fn(&str) -> Option<&'a str>,
) -> Result<Vec<Diagnostic>, String> {
    diagnostics
        .into_iter()
        .map(|mut diagnostic| {
            let found = kind(&diagnostic.code).ok_or_else(|| {
                format!(
                    "rule ID {:?} has no actionlint kind in the rule registry or tests/conformance/rule-kinds.toml: {diagnostic}",
                    diagnostic.code
                )
            })?;
            diagnostic.code = found.to_owned();
            Ok(diagnostic)
        })
        .collect()
}

/// Parses the rule-kinds table: one `rule-id = "kind"` line per rule.
///
/// # Errors
///
/// Returns the TOML error, or a message if a value is not a string.
pub fn parse_table(text: &str) -> Result<BTreeMap<String, String>, String> {
    let table: toml::Table = text.parse().map_err(|error| format!("{error}"))?;
    table
        .into_iter()
        .map(|(rule, kind)| match kind {
            toml::Value::String(kind) => Ok((rule, kind)),
            other => Err(format!(
                "rule {rule:?} maps to a {}, not a string",
                other.type_str()
            )),
        })
        .collect()
}

/// One line for each way that `table`, the rule IDs that have no rule yet,
/// and `rules`, the registry, disagree with `checklist`, the rule IDs and
/// kinds of the parity checklist.
///
/// Every checklist rule ID is in exactly one of `table` and `rules`, with the
/// checklist's kind, and every rule ID of `table` and `rules` is in the
/// checklist.
#[must_use]
pub fn check_rules(
    table: &BTreeMap<String, String>,
    checklist: &BTreeMap<String, String>,
    rules: &[&RuleMeta],
) -> Vec<String> {
    let registered: BTreeMap<&str, &str> = rules.iter().map(|rule| (rule.id, rule.kind)).collect();
    let mut problems = Vec::new();
    for (rule, kind) in checklist {
        match (registered.get(rule.as_str()), table.get(rule)) {
            (Some(_), Some(_)) => problems.push(format!(
                "{rule} is in the rule registry, so remove it from rule-kinds.toml"
            )),
            (Some(found), None) if found != kind => problems.push(format!(
                "{rule}: its RuleMeta says {found}, the parity checklist says {kind}"
            )),
            (None, Some(found)) if found != kind => problems.push(format!(
                "{rule}: rule-kinds.toml says {found}, the parity checklist says {kind}"
            )),
            (None, None) => problems.push(format!(
                "{rule} ({kind}) is in the parity checklist but neither in the rule registry nor in rule-kinds.toml"
            )),
            _ => {}
        }
    }
    for rule in table.keys().filter(|rule| !checklist.contains_key(*rule)) {
        problems.push(format!(
            "{rule} is in rule-kinds.toml but not in the parity checklist"
        ));
    }
    for (rule, kind) in registered {
        if !checklist.contains_key(rule) {
            problems.push(format!(
                "{rule} ({kind}) is in the rule registry but not in the parity checklist"
            ));
            if table.contains_key(rule) {
                problems.push(format!(
                    "{rule} is in the rule registry, so remove it from rule-kinds.toml"
                ));
            }
        }
    }
    problems
}

/// Collects the rule IDs and kinds from every "Rule ID (kind)" cell in the
/// parity checklist, where a cell reads like `` `rule-id` (kind) ``.
///
/// # Errors
///
/// Returns a message for each rule ID that the checklist maps to two kinds.
pub fn checklist_kinds(markdown: &str) -> Result<BTreeMap<String, String>, String> {
    static PAIR: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"`([a-z0-9-]+)` \(([a-z-]+)\)").expect("the pattern is valid")
    });
    let mut kinds = BTreeMap::new();
    let mut conflicts = Vec::new();
    for row in super::checklist::markdown_rows(markdown) {
        let Some(cell) = row.get("Rule ID (kind)") else {
            continue;
        };
        for captures in PAIR.captures_iter(cell) {
            let (rule, kind) = (&captures[1], &captures[2]);
            match kinds.insert(rule.to_owned(), kind.to_owned()) {
                Some(previous) if previous != kind => {
                    conflicts.push(format!("{rule} maps to both {previous} and {kind}"));
                }
                _ => {}
            }
        }
    }
    if conflicts.is_empty() {
        Ok(kinds)
    } else {
        Err(conflicts.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance::output::parse_line;

    #[test]
    fn a_known_rule_id_maps_to_its_kind() {
        assert_eq!(actionlint_kind("yaml-syntax-error"), Some("syntax-check"));
        assert_eq!(actionlint_kind("undefined-variable"), Some("expression"));
    }

    #[test]
    fn a_rule_id_without_a_rule_takes_its_kind_from_the_table() {
        // The table is empty once every rule of the checklist exists.
        for (rule, kind) in TABLE.iter() {
            assert!(workflint_rules::registry::find(rule).is_none());
            assert_eq!(actionlint_kind(rule), Some(kind.as_str()));
        }
    }

    #[test]
    fn the_kinds_are_the_kinds_of_the_registry() {
        let kinds = kinds();
        assert!(kinds.contains("syntax-check"));
        assert!(kinds.contains("workflow-call"));
        assert!(!kinds.contains("yaml-syntax-error"));
        assert_eq!(kinds.len(), 18);
    }

    #[test]
    fn an_unknown_rule_id_has_no_kind() {
        assert_eq!(actionlint_kind("no-such-rule"), None);
    }

    #[test]
    fn to_kinds_replaces_the_rule_id_in_brackets() {
        let diagnostic = parse_line("test.yaml:1:1: m [undefined-variable]").unwrap();
        let mapped = to_kinds(vec![diagnostic]).unwrap();
        assert_eq!(mapped[0].to_string(), "test.yaml:1:1: m [expression]");
    }

    #[test]
    fn to_kinds_fails_on_an_unknown_rule_id() {
        let diagnostic = parse_line("test.yaml:1:1: m [syntax-check]").unwrap();
        let error = to_kinds_with(vec![diagnostic], |_| None).unwrap_err();
        assert!(error.contains("\"syntax-check\""), "{error}");
    }

    #[test]
    fn the_table_parses_rule_kind_pairs() {
        let table = parse_table("a-rule = \"expression\"\nb-rule = \"action\"\n").unwrap();
        assert_eq!(table["a-rule"], "expression");
        assert_eq!(table.len(), 2);
    }

    fn meta(id: &'static str, kind: &'static str) -> RuleMeta {
        RuleMeta {
            id,
            description: "",
            kind,
            category: workflint_diagnostics::Category::Correctness,
            default_severity: workflint_diagnostics::Severity::Error,
            docs_url: "",
        }
    }

    #[test]
    fn a_checklist_rule_id_is_in_exactly_one_of_the_registry_and_the_table() {
        let checklist = parse_table(
            "a-rule = \"expression\"\nb-rule = \"action\"\nc-rule = \"events\"\nd-rule = \"glob\"\n",
        )
        .unwrap();
        let table = parse_table("b-rule = \"action\"\nc-rule = \"events\"\n").unwrap();
        let (a, c) = (meta("a-rule", "expression"), meta("c-rule", "events"));
        assert_eq!(
            check_rules(&table, &checklist, &[&a, &c]),
            [
                "c-rule is in the rule registry, so remove it from rule-kinds.toml",
                "d-rule (glob) is in the parity checklist but neither in the rule registry nor in rule-kinds.toml",
            ]
        );
    }

    #[test]
    fn a_kind_that_differs_from_the_checklist_is_reported() {
        let checklist = parse_table("a-rule = \"expression\"\nb-rule = \"action\"\n").unwrap();
        let table = parse_table("b-rule = \"events\"\n").unwrap();
        let a = meta("a-rule", "syntax-check");
        assert_eq!(
            check_rules(&table, &checklist, &[&a]),
            [
                "a-rule: its RuleMeta says syntax-check, the parity checklist says expression",
                "b-rule: rule-kinds.toml says events, the parity checklist says action",
            ]
        );
    }

    #[test]
    fn a_rule_id_missing_from_the_checklist_is_reported() {
        let checklist = BTreeMap::new();
        let table = parse_table("b-rule = \"action\"\n").unwrap();
        let a = meta("a-rule", "expression");
        assert_eq!(
            check_rules(&table, &checklist, &[&a]),
            [
                "b-rule is in rule-kinds.toml but not in the parity checklist",
                "a-rule (expression) is in the rule registry but not in the parity checklist",
            ]
        );
    }

    #[test]
    fn a_table_value_that_is_not_a_string_is_an_error() {
        assert!(parse_table("a-rule = 1\n").is_err());
    }

    #[test]
    fn checklist_cells_give_rule_kind_pairs() {
        let markdown = "\
| Item | Rule ID (kind) | Milestone |
|---|---|---|
| - [ ] x \\| y | `first-rule` (syntax-check), `second-rule` (expression) | M2 |
| - [ ] z | n/a | M3 |
";
        let kinds = checklist_kinds(markdown).unwrap();
        assert_eq!(kinds["first-rule"], "syntax-check");
        assert_eq!(kinds["second-rule"], "expression");
        assert_eq!(kinds.len(), 2);
    }

    #[test]
    fn a_rule_with_two_kinds_in_the_checklist_is_an_error() {
        let markdown = "\
| Rule ID (kind) |
|---|
| `a-rule` (syntax-check) |
| `a-rule` (expression) |
";
        assert!(checklist_kinds(markdown).is_err());
    }
}
