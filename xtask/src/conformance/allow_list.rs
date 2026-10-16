//! `tests/conformance/allowed-differences.toml`: the expected lines that
//! workflint replaces, removes, or adds on purpose. Each entry names an anchor
//! in `docs/differences.md` (ADR 0004).

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;

use super::matcher::pattern;
use super::output::parse_line;

/// The parsed file.
#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AllowList {
    #[serde(default, rename = "difference")]
    pub entries: Vec<Entry>,
}

/// One `[[difference]]` table.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// An anchor in `docs/differences.md`, without the `#`.
    pub anchor: String,
    /// The fixture ID, as in `expected-failures.toml`.
    pub fixture: String,
    /// The line of the `.out` file that workflint does not report.
    pub upstream: Option<String>,
    /// The line that workflint reports instead, or in addition.
    pub workflint: Option<String>,
}

impl AllowList {
    /// Parses the TOML text.
    ///
    /// # Errors
    ///
    /// Returns the TOML error, which names unknown keys.
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|error| format!("{error}"))
    }

    /// The entries for one fixture, in file order.
    #[must_use]
    pub fn entries_for(&self, fixture: &str) -> Vec<&Entry> {
        self.entries
            .iter()
            .filter(|entry| entry.fixture == fixture)
            .collect()
    }

    /// Checks every entry against the anchors of `docs/differences.md` and
    /// the known fixture IDs.
    ///
    /// # Errors
    ///
    /// Returns one line per problem.
    pub fn validate(
        &self,
        anchors: &BTreeSet<String>,
        fixtures: &BTreeSet<String>,
    ) -> Result<(), String> {
        let mut problems = Vec::new();
        for entry in &self.entries {
            if !anchors.contains(&entry.anchor) {
                problems.push(format!(
                    "anchor {:?} of the entry for {} does not exist in docs/differences.md",
                    entry.anchor, entry.fixture
                ));
            }
            if !fixtures.contains(&entry.fixture) {
                problems.push(format!(
                    "fixture {:?} of the entry for anchor {} does not exist",
                    entry.fixture, entry.anchor
                ));
            }
            if entry.upstream.is_none() && entry.workflint.is_none() {
                problems.push(format!(
                    "the entry for {} and anchor {} has neither `upstream` nor `workflint`",
                    entry.fixture, entry.anchor
                ));
            }
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems.join("\n"))
        }
    }
}

impl Entry {
    /// Names the entry in a message.
    #[must_use]
    pub fn describe(&self) -> String {
        format!(
            "the allowed-differences.toml entry for {} with anchor {}",
            self.fixture, self.anchor
        )
    }
}

/// The anchors (`<a id="...">`) in the text of `docs/differences.md`.
pub fn anchors(markdown: &str) -> BTreeSet<String> {
    static ANCHOR: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"<a id="([^"]+)">"#).expect("the pattern is valid"));
    ANCHOR
        .captures_iter(markdown)
        .map(|captures| captures[1].to_owned())
        .collect()
}

/// Applies `entries` to the expected lines of a `.out` file.
///
/// An entry with `upstream` removes that line, which must be in `expected`.
/// An entry with `workflint` inserts that line before the first expected line
/// that sorts after it by path, line, column and message. A pattern line
/// sorts by the position at its start, if it has one, and counts as equal to
/// any line at that position. An entry with both replaces the line.
///
/// # Errors
///
/// Returns a message if an `upstream` line is not in `expected`.
pub fn apply(expected: &[String], entries: &[&Entry]) -> Result<Vec<String>, String> {
    let mut lines = expected.to_vec();
    for entry in entries {
        if let Some(upstream) = &entry.upstream {
            let index = lines
                .iter()
                .position(|line| line == upstream)
                .ok_or_else(|| {
                    format!(
                        "{}: `upstream` matches no line of the .out file: {upstream:?}",
                        entry.describe()
                    )
                })?;
            lines.remove(index);
        }
        if let Some(workflint) = &entry.workflint {
            insert(&mut lines, workflint);
        }
    }
    Ok(lines)
}

/// Inserts `line` before the first of `lines` that sorts after it, as
/// [`apply`] inserts a `workflint` line.
pub fn insert(lines: &mut Vec<String>, line: &str) {
    let key = sort_key(line);
    let index = lines
        .iter()
        .position(|other| compare(&sort_key(other), &key) == Ordering::Greater)
        .unwrap_or(lines.len());
    lines.insert(index, line.to_owned());
}

/// The file path of an expected line: the path of an exact line, or the path
/// at the start of a pattern line. `None` for a line with no position.
#[must_use]
pub fn line_path(line: &str) -> Option<String> {
    sort_key(line).map(|(path, ..)| path)
}

/// What an expected line sorts by: path, line and column, and the message
/// for an exact line. `None` for a line with no position.
type SortKey = Option<(String, u32, u32, Option<String>)>;

fn sort_key(line: &str) -> SortKey {
    match pattern(line) {
        None => parse_line(line)
            .ok()
            .map(|d| (d.path, d.line, d.column, Some(d.message))),
        Some(source) => {
            static POSITION: LazyLock<Regex> = LazyLock::new(|| {
                Regex::new(r"^\^?((?:[^:\\]|\\.)+?):(\d+):(\d+):").expect("the pattern is valid")
            });
            let captures = POSITION.captures(source)?;
            let path = captures[1].replace('\\', "");
            Some((
                path,
                captures[2].parse().ok()?,
                captures[3].parse().ok()?,
                None,
            ))
        }
    }
}

/// Lines without a position never sort after another line.
fn compare(line: &SortKey, inserted: &SortKey) -> Ordering {
    let (Some(line), Some(inserted)) = (line, inserted) else {
        return Ordering::Less;
    };
    let position = (&line.0, line.1, line.2).cmp(&(&inserted.0, inserted.1, inserted.2));
    match (position, &line.3, &inserted.3) {
        (Ordering::Equal, Some(a), Some(b)) => a.cmp(b),
        (position, _, _) => position,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(upstream: Option<&str>, workflint: Option<&str>) -> Entry {
        Entry {
            anchor: "sort-order".into(),
            fixture: "err/x".into(),
            upstream: upstream.map(str::to_owned),
            workflint: workflint.map(str::to_owned),
        }
    }

    fn lines(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|line| (*line).to_owned()).collect()
    }

    #[test]
    fn the_file_parses_difference_tables() {
        let list = AllowList::parse(
            r#"
[[difference]]
anchor = "yaml-error-positions"
fixture = "err/yaml_syntax_error"
upstream = "test.yaml:1:1: a [syntax-check]"
workflint = "test.yaml:2:1: a [syntax-check]"

[[difference]]
anchor = "missing-tools"
fixture = "projects/x"
workflint = "workflows/a.yaml:1:1: b [action]"
"#,
        )
        .unwrap();
        assert_eq!(list.entries.len(), 2);
        assert_eq!(list.entries[1].upstream, None);
        assert_eq!(list.entries_for("projects/x").len(), 1);
    }

    #[test]
    fn an_empty_file_has_no_entries() {
        assert_eq!(AllowList::parse("").unwrap(), AllowList::default());
    }

    #[test]
    fn an_unknown_key_is_an_error() {
        let error = AllowList::parse(
            "[[difference]]\nanchor = \"a\"\nfixture = \"err/x\"\nreplace = \"y\"\n",
        )
        .unwrap_err();
        assert!(error.contains("replace"), "{error}");
    }

    #[test]
    fn an_entry_with_an_unknown_anchor_fails_validation() {
        let list = AllowList {
            entries: vec![entry(Some("a"), None)],
        };
        let anchors = BTreeSet::from(["rule-id-in-brackets".to_owned()]);
        let fixtures = BTreeSet::from(["err/x".to_owned()]);
        let error = list.validate(&anchors, &fixtures).unwrap_err();
        assert!(error.contains("anchor \"sort-order\""), "{error}");
    }

    #[test]
    fn an_entry_with_an_unknown_fixture_fails_validation() {
        let list = AllowList {
            entries: vec![entry(Some("a"), None)],
        };
        let anchors = BTreeSet::from(["sort-order".to_owned()]);
        let error = list.validate(&anchors, &BTreeSet::new()).unwrap_err();
        assert!(error.contains("fixture \"err/x\""), "{error}");
    }

    #[test]
    fn an_entry_without_lines_fails_validation() {
        let list = AllowList {
            entries: vec![entry(None, None)],
        };
        let anchors = BTreeSet::from(["sort-order".to_owned()]);
        let fixtures = BTreeSet::from(["err/x".to_owned()]);
        assert!(list.validate(&anchors, &fixtures).is_err());
    }

    #[test]
    fn a_valid_entry_passes_validation() {
        let list = AllowList {
            entries: vec![entry(Some("a"), None)],
        };
        let anchors = BTreeSet::from(["sort-order".to_owned()]);
        let fixtures = BTreeSet::from(["err/x".to_owned()]);
        assert_eq!(list.validate(&anchors, &fixtures), Ok(()));
    }

    #[test]
    fn anchors_are_read_from_a_tags() {
        let markdown = "<a id=\"rule-id-in-brackets\"></a>\n### x\n<a id=\"sort-order\"></a>\n";
        assert_eq!(
            anchors(markdown),
            BTreeSet::from(["rule-id-in-brackets".to_owned(), "sort-order".to_owned()])
        );
    }

    #[test]
    fn an_upstream_line_is_removed() {
        let expected = lines(&["test.yaml:1:1: a [x]", "test.yaml:2:1: b [x]"]);
        let removed = entry(Some("test.yaml:1:1: a [x]"), None);
        assert_eq!(
            apply(&expected, &[&removed]).unwrap(),
            lines(&["test.yaml:2:1: b [x]"])
        );
    }

    #[test]
    fn an_upstream_line_that_is_not_in_the_out_file_is_an_error() {
        let expected = lines(&["test.yaml:1:1: a [x]"]);
        let stale = entry(Some("test.yaml:9:9: gone [x]"), None);
        let error = apply(&expected, &[&stale]).unwrap_err();
        assert!(error.contains("matches no line"), "{error}");
    }

    #[test]
    fn a_workflint_line_is_inserted_in_sorted_order() {
        let expected = lines(&[
            "test.yaml:1:1: a [x]",
            r"/test\.yaml:3:1: c/",
            "test.yaml:5:1: e [x]",
        ]);
        let added = entry(None, Some("test.yaml:4:1: d [x]"));
        assert_eq!(
            apply(&expected, &[&added]).unwrap(),
            lines(&[
                "test.yaml:1:1: a [x]",
                r"/test\.yaml:3:1: c/",
                "test.yaml:4:1: d [x]",
                "test.yaml:5:1: e [x]",
            ])
        );
    }

    #[test]
    fn a_workflint_line_at_a_shared_position_sorts_by_message() {
        let expected = lines(&["test.yaml:1:1: a [x]", "test.yaml:1:1: c [x]"]);
        let added = entry(None, Some("test.yaml:1:1: b [x]"));
        assert_eq!(
            apply(&expected, &[&added]).unwrap(),
            lines(&[
                "test.yaml:1:1: a [x]",
                "test.yaml:1:1: b [x]",
                "test.yaml:1:1: c [x]"
            ])
        );
    }

    #[test]
    fn a_workflint_line_goes_after_a_pattern_line_at_its_position() {
        let expected = lines(&["/test.yaml:1:1: z/", "test.yaml:2:1: a [x]"]);
        let added = entry(None, Some("test.yaml:1:1: a [x]"));
        assert_eq!(
            apply(&expected, &[&added]).unwrap(),
            lines(&[
                "/test.yaml:1:1: z/",
                "test.yaml:1:1: a [x]",
                "test.yaml:2:1: a [x]"
            ])
        );
    }

    #[test]
    fn the_path_of_a_line_comes_from_its_position() {
        assert_eq!(
            line_path("workflows/a.yaml:1:2: m [x]").as_deref(),
            Some("workflows/a.yaml")
        );
        assert_eq!(
            line_path(r"/workflows/b\.yaml:3:4: m/").as_deref(),
            Some("workflows/b.yaml")
        );
        assert_eq!(line_path("/is unknown/"), None);
        assert_eq!(line_path("Hello, world!"), None);
    }

    #[test]
    fn a_replaced_line_moves_to_its_new_position() {
        let expected = lines(&["test.yaml:1:1: a [x]", "test.yaml:2:1: b [x]"]);
        let replaced = entry(Some("test.yaml:1:1: a [x]"), Some("test.yaml:3:1: a [x]"));
        assert_eq!(
            apply(&expected, &[&replaced]).unwrap(),
            lines(&["test.yaml:2:1: b [x]", "test.yaml:3:1: a [x]"])
        );
    }
}
