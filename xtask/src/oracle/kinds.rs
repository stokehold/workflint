//! `--kind`: limits a run to the diagnostics of some actionlint kinds.
//!
//! A line's kind is the text in its brackets. For workflint's lines that is
//! the kind its rule ID maps to. A `/pattern/` line has the kinds that its
//! pattern ends with, as `\[kind\]` or `\[(kind|kind)\]`. A line that shows
//! no kind, such as a line without a position, is always kept, so that it
//! still shows up in the report.

use std::collections::BTreeSet;

use anyhow::{Result, bail};
use xtask::conformance::matcher::pattern;
use xtask::conformance::output::parse_line;
use xtask::conformance::rule_kinds;

/// The actionlint kinds that a run compares. No kind means every kind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Kinds {
    selected: BTreeSet<String>,
}

impl Kinds {
    /// The kinds named `names`, each one of the kinds in
    /// `workflint_rules::kind`.
    ///
    /// # Errors
    ///
    /// Fails on the first name that is not a kind, and lists the kinds.
    pub(crate) fn new(names: &[String]) -> Result<Self> {
        let known = rule_kinds::kinds();
        if let Some(unknown) = names.iter().find(|name| !known.contains(name.as_str())) {
            let known: Vec<&str> = known.into_iter().collect();
            bail!(
                "unknown kind {unknown:?}; the kinds are {}",
                known.join(", ")
            );
        }
        Ok(Self {
            selected: names.iter().cloned().collect(),
        })
    }

    /// Whether the run compares every kind.
    pub(crate) fn is_all(&self) -> bool {
        self.selected.is_empty()
    }

    /// The selected kinds, sorted.
    pub(crate) fn names(&self) -> Vec<&str> {
        self.selected.iter().map(String::as_str).collect()
    }

    /// Whether a diagnostic of `kind` is compared.
    pub(crate) fn keeps_kind(&self, kind: &str) -> bool {
        self.is_all() || self.selected.contains(kind)
    }

    /// Whether `line`, a line of output, of a `.out` file, or of the
    /// allow-list, is compared: when one of its kinds is selected, or when it
    /// shows no kind.
    pub(crate) fn keeps_line(&self, line: &str) -> bool {
        self.is_all()
            || line_kinds(line).is_none_or(|kinds| kinds.iter().any(|kind| self.keeps_kind(kind)))
    }
}

/// The kinds of the external tools. The oracle runs both binaries with the
/// tools off, so neither prints a line of these kinds.
const TOOL_KINDS: [&str; 2] = ["shellcheck", "pyflakes"];

/// Whether every kind that `line` shows is the kind of an external tool.
/// Such a line of the allow-list or a `.out` file cannot apply to a run.
pub(crate) fn only_external_tools(line: &str) -> bool {
    line_kinds(line).is_some_and(|kinds| kinds.iter().all(|kind| TOOL_KINDS.contains(kind)))
}

/// The kinds that `line` shows, or `None` if it shows none.
fn line_kinds(line: &str) -> Option<Vec<&str>> {
    let Some(source) = pattern(line) else {
        parse_line(line).ok()?;
        let (_, code) = line.strip_suffix(']')?.rsplit_once('[')?;
        return Some(vec![code]);
    };
    let (_, inner) = source.strip_suffix(r"\]")?.rsplit_once(r"\[")?;
    let inner = inner
        .strip_prefix('(')
        .and_then(|inner| inner.strip_suffix(')'))
        .unwrap_or(inner);
    let kinds: Vec<&str> = inner.split('|').collect();
    kinds
        .iter()
        .all(|kind| !kind.is_empty() && kind.bytes().all(|b| b.is_ascii_lowercase() || b == b'-'))
        .then_some(kinds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(names: &[&str]) -> Kinds {
        let names: Vec<String> = names.iter().map(|name| (*name).to_owned()).collect();
        Kinds::new(&names).unwrap()
    }

    #[test]
    fn an_unknown_kind_name_is_an_error_that_lists_the_kinds() {
        let error = Kinds::new(&["syntax".to_owned()]).unwrap_err().to_string();
        assert!(error.contains("unknown kind \"syntax\""), "{error}");
        assert!(error.contains("syntax-check"), "{error}");
        assert!(error.contains("workflow-call"), "{error}");
    }

    #[test]
    fn the_kinds_are_sorted_and_named_once() {
        let kinds = kinds(&["syntax-check", "expression", "syntax-check"]);
        assert_eq!(kinds.names(), ["expression", "syntax-check"]);
        assert!(!kinds.is_all());
        assert!(Kinds::new(&[]).unwrap().is_all());
    }

    #[test]
    fn without_kinds_every_line_is_kept() {
        let all = Kinds::default();
        assert!(all.keeps_line("test.yaml:1:1: a [expression]"));
        assert!(all.keeps_line("Hello, world!"));
        assert!(all.keeps_kind("shellcheck"));
    }

    #[test]
    fn a_line_is_kept_when_its_kind_is_selected() {
        let kinds = kinds(&["syntax-check"]);
        assert!(kinds.keeps_line("test.yaml:1:1: a [syntax-check]"));
        assert!(!kinds.keeps_line("test.yaml:1:1: a [expression]"));
        assert!(kinds.keeps_kind("syntax-check"));
        assert!(!kinds.keeps_kind("expression"));
    }

    #[test]
    fn a_line_is_of_external_tools_only_when_each_of_its_kinds_is_a_tool() {
        assert!(only_external_tools("test.yaml:6:9: a [shellcheck]"));
        assert!(only_external_tools(r"/test\.yaml:9:9: .+ \[pyflakes\]/"));
        assert!(only_external_tools(
            r"/test\.yaml:9:9: .+ \[(shellcheck|pyflakes)\]/"
        ));
        assert!(!only_external_tools(
            r"/test\.yaml:9:9: .+ \[(shellcheck|expression)\]/"
        ));
        assert!(!only_external_tools("test.yaml:6:9: a [expression]"));
        assert!(!only_external_tools("Hello, world!"));
    }

    #[test]
    fn a_pattern_line_has_the_kind_at_the_end_of_its_pattern() {
        let kinds = kinds(&["action"]);
        assert!(kinds.keeps_line(r"/test\.yaml:1:1: .+ \[action\]/"));
        assert!(!kinds.keeps_line(r"/test\.yaml:1:1: .+ \[expression\]/"));
    }

    #[test]
    fn a_pattern_line_with_alternative_kinds_is_kept_for_each_of_them() {
        let line = r"/workflows/test\.yaml:5:11: .+ \[(expression|workflow-call)\]/";
        assert!(kinds(&["workflow-call"]).keeps_line(line));
        assert!(kinds(&["expression"]).keeps_line(line));
        assert!(!kinds(&["syntax-check"]).keeps_line(line));
    }

    #[test]
    fn a_line_that_shows_no_kind_is_kept() {
        let kinds = kinds(&["syntax-check"]);
        assert!(kinds.keeps_line("Hello, world!"));
        assert!(kinds.keeps_line(r#"/test\.yaml:5:14: label "macos-12" is unknown\. .+/"#));
        assert!(kinds.keeps_line("/{string => string}/"));
    }
}
