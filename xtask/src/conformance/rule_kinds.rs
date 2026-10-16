//! Maps workflint rule IDs to actionlint kinds, so that workflint's output
//! compares with the reference's `.out` files (ADR 0003, ADR 0008).
//!
//! Until the rule registry exists (M4.1), the map is the table in
//! `tests/conformance/rule-kinds.toml`, copied from the "Rule ID (kind)"
//! column of `docs/parity-checklist.md`. The conformance target checks that
//! the two agree.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;

use super::output::Diagnostic;

static TABLE: LazyLock<BTreeMap<String, String>> = LazyLock::new(|| {
    parse_table(include_str!("../../../tests/conformance/rule-kinds.toml"))
        .expect("tests/conformance/rule-kinds.toml is valid")
});

/// The actionlint kind that `rule_id` maps to, or `None` for an unknown ID.
///
/// This is the only place that knows the mapping. M4.1 replaces the table
/// with a lookup in the rule registry.
pub fn actionlint_kind(rule_id: &str) -> Option<&'static str> {
    TABLE.get(rule_id).map(String::as_str)
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
                    "rule ID {:?} has no actionlint kind in tests/conformance/rule-kinds.toml: {diagnostic}",
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
