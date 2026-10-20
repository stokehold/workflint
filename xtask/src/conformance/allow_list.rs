//! `tests/conformance/allowed-differences.toml`: the expected lines that
//! workflint replaces, removes, or adds on purpose, in fixtures, in corpus
//! files, and in the `Output` blocks of upstream `docs/checks.md`. Each entry
//! names an anchor in `docs/differences.md` (ADR 0004).

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;

use super::corpus;
use super::matcher::{line_matches, pattern};
use super::output::parse_line;

/// The parsed file.
#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AllowList {
    /// The `[[difference]]` tables, in file order.
    #[serde(default, rename = "difference")]
    pub entries: Vec<Entry>,
    /// The `[[example]]` tables, in file order.
    #[serde(default, rename = "example")]
    pub examples: Vec<Example>,
}

/// One `[[example]]` table: a line of the `Output` block of a section of
/// upstream `docs/checks.md` that workflint prints differently. It is for
/// lines that the section's fixtures match with a `/pattern/`, which needs no
/// `[[difference]]` entry, while the `Output` block has the whole line.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Example {
    /// An anchor in `docs/differences.md`, without the `#`.
    pub anchor: String,
    /// The anchor of the section in `docs/checks.md`, without the `#`.
    pub section: String,
    /// The error line of the `Output` block, with the kind in brackets.
    pub upstream: String,
    /// The line that workflint prints instead, with the kind in brackets.
    pub workflint: String,
}

/// One `[[difference]]` table.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// An anchor in `docs/differences.md`, without the `#`.
    pub anchor: String,
    /// The fixture ID, as in `expected-failures.toml`, `realworld/<file
    /// name>` for a corpus file, or `realworld/*` for every corpus file.
    pub fixture: String,
    /// The line of the `.out` file that workflint does not report. For a
    /// corpus file, a line that the reference prints. For every corpus file,
    /// a `/pattern/`.
    pub upstream: Option<String>,
    /// The line that workflint reports instead, or in addition. For every
    /// corpus file, the text that replaces each match of `upstream`.
    pub workflint: Option<String>,
}

/// What an entry applies to, from its `fixture`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target<'a> {
    /// The fixture with this ID.
    Fixture(&'a str),
    /// The corpus file with this name.
    CorpusFile(&'a str),
    /// Every corpus file.
    EveryCorpusFile,
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

    /// The entries for the corpus files `names`, in file order, then the
    /// entries for every corpus file, in file order.
    #[must_use]
    pub fn corpus_entries(&self, names: &[String]) -> Vec<&Entry> {
        let (every, files): (Vec<&Entry>, Vec<&Entry>) = self
            .entries
            .iter()
            .filter(|entry| match entry.target() {
                Target::CorpusFile(name) => names.iter().any(|selected| selected == name),
                Target::EveryCorpusFile => true,
                Target::Fixture(_) => false,
            })
            .partition(|entry| entry.target() == Target::EveryCorpusFile);
        files.into_iter().chain(every).collect()
    }

    /// The `[[example]]` tables for the `docs/checks.md` section `section`,
    /// in file order.
    #[must_use]
    pub fn examples_for(&self, section: &str) -> Vec<&Example> {
        self.examples
            .iter()
            .filter(|example| example.section == section)
            .collect()
    }

    /// Checks every entry against the anchors of `docs/differences.md`, the
    /// known fixture IDs, and the names of the corpus files, and every
    /// example against the anchors.
    ///
    /// An entry for a corpus file must name a file in the corpus, and each
    /// of its lines must start with that file's name and a position, as an
    /// exact line or a `/pattern/`. An entry for every corpus file needs an
    /// `upstream` that is a valid `/pattern/`, and a `workflint`.
    ///
    /// # Errors
    ///
    /// Returns one line per problem.
    pub fn validate(
        &self,
        anchors: &BTreeSet<String>,
        fixtures: &BTreeSet<String>,
        corpus: &BTreeSet<String>,
    ) -> Result<(), String> {
        let mut problems = Vec::new();
        for entry in &self.entries {
            if !anchors.contains(&entry.anchor) {
                problems.push(format!(
                    "anchor {:?} of the entry for {} does not exist in docs/differences.md",
                    entry.anchor, entry.fixture
                ));
            }
            match entry.target() {
                Target::Fixture(id) => {
                    if !fixtures.contains(id) {
                        problems.push(format!(
                            "fixture {id:?} of the entry for anchor {} does not exist",
                            entry.anchor
                        ));
                    }
                }
                Target::CorpusFile(name) => {
                    if !corpus.contains(name) {
                        problems.push(format!(
                            "corpus file {name:?} of the entry for anchor {} is not in {}",
                            entry.anchor,
                            corpus::ARCHIVE
                        ));
                    }
                    for line in entry.upstream.iter().chain(&entry.workflint) {
                        if line_path(line).as_deref() != Some(name) {
                            problems.push(format!(
                                "{}: {line:?} is not a line of {name}: it must start with \
                                 the file name and a position",
                                entry.describe()
                            ));
                        }
                    }
                }
                Target::EveryCorpusFile => {
                    let upstream = entry.upstream.as_deref();
                    match upstream.map(|line| (pattern(line), line_matches(line, ""))) {
                        Some((Some(_), Ok(_))) => {}
                        Some((Some(_), Err(problem))) => {
                            problems.push(format!("{}: `upstream` {problem}", entry.describe()));
                        }
                        _ => problems.push(format!(
                            "{}: `upstream` must be a /pattern/",
                            entry.describe()
                        )),
                    }
                    if entry.workflint.is_none() {
                        problems.push(format!(
                            "{}: `workflint` must give the text that replaces each match",
                            entry.describe()
                        ));
                    }
                }
            }
            if entry.upstream.is_none() && entry.workflint.is_none() {
                problems.push(format!(
                    "the entry for {} and anchor {} has neither `upstream` nor `workflint`",
                    entry.fixture, entry.anchor
                ));
            }
        }
        for example in &self.examples {
            if !anchors.contains(&example.anchor) {
                problems.push(format!(
                    "anchor {:?} of the example for checks.md#{} does not exist in \
                     docs/differences.md",
                    example.anchor, example.section
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
    /// What the entry applies to.
    #[must_use]
    pub fn target(&self) -> Target<'_> {
        if self.fixture == corpus::EVERY_FILE {
            Target::EveryCorpusFile
        } else if let Some(name) = self.fixture.strip_prefix(corpus::PREFIX) {
            Target::CorpusFile(name)
        } else {
            Target::Fixture(&self.fixture)
        }
    }

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
/// any line at that position. An entry with both replaces the line. A
/// pattern line that replaces a line at its own position takes that line's
/// place, because its order among the lines at that position is unknown.
///
/// # Errors
///
/// Returns a message if an `upstream` line is not in `expected`.
pub fn apply(expected: &[String], entries: &[&Entry]) -> Result<Vec<String>, String> {
    let mut lines = expected.to_vec();
    for entry in entries {
        let mut removed = None;
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
            removed = Some((index, upstream.as_str()));
        }
        if let Some(workflint) = &entry.workflint {
            place(&mut lines, removed, workflint);
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

/// Inserts `line`, the `workflint` line of an entry, as [`apply`] does.
/// `removed` is the index that the entry's `upstream` line was removed from,
/// with that line, if the entry has one.
pub fn place(lines: &mut Vec<String>, removed: Option<(usize, &str)>, line: &str) {
    match removed {
        Some((index, upstream)) if pattern(line).is_some() && same_position(upstream, line) => {
            lines.insert(index, line.to_owned());
        }
        _ => insert(lines, line),
    }
}

/// Whether two expected lines have a position and it is the same.
fn same_position(a: &str, b: &str) -> bool {
    match (sort_key(a), sort_key(b)) {
        (Some(a), Some(b)) => (a.0, a.1, a.2) == (b.0, b.1, b.2),
        _ => false,
    }
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
    fn the_file_parses_example_tables() {
        let list = AllowList::parse(
            r#"
[[example]]
anchor = "platform-data-updates"
section = "check-runner-labels"
upstream = "test.yaml:1:1: a [runner-label]"
workflint = "test.yaml:1:1: b [runner-label]"
"#,
        )
        .unwrap();
        assert_eq!(list.entries, []);
        assert_eq!(list.examples_for("check-runner-labels").len(), 1);
        assert_eq!(list.examples_for("permissions"), Vec::<&Example>::new());
    }

    #[test]
    fn an_example_with_an_unknown_anchor_fails_validation() {
        let list = AllowList::parse(
            "[[example]]\nanchor = \"no-such-anchor\"\nsection = \"s\"\n\
             upstream = \"a\"\nworkflint = \"b\"\n",
        )
        .unwrap();
        let error = list
            .validate(&BTreeSet::new(), &BTreeSet::new(), &BTreeSet::new())
            .unwrap_err();
        assert!(error.contains("anchor \"no-such-anchor\""), "{error}");
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
            ..AllowList::default()
        };
        let anchors = BTreeSet::from(["rule-id-in-brackets".to_owned()]);
        let fixtures = BTreeSet::from(["err/x".to_owned()]);
        let error = list
            .validate(&anchors, &fixtures, &BTreeSet::new())
            .unwrap_err();
        assert!(error.contains("anchor \"sort-order\""), "{error}");
    }

    #[test]
    fn an_entry_with_an_unknown_fixture_fails_validation() {
        let list = AllowList {
            entries: vec![entry(Some("a"), None)],
            ..AllowList::default()
        };
        let anchors = BTreeSet::from(["sort-order".to_owned()]);
        let error = list
            .validate(&anchors, &BTreeSet::new(), &BTreeSet::new())
            .unwrap_err();
        assert!(error.contains("fixture \"err/x\""), "{error}");
    }

    #[test]
    fn an_entry_without_lines_fails_validation() {
        let list = AllowList {
            entries: vec![entry(None, None)],
            ..AllowList::default()
        };
        let anchors = BTreeSet::from(["sort-order".to_owned()]);
        let fixtures = BTreeSet::from(["err/x".to_owned()]);
        assert!(
            list.validate(&anchors, &fixtures, &BTreeSet::new())
                .is_err()
        );
    }

    #[test]
    fn a_valid_entry_passes_validation() {
        let list = AllowList {
            entries: vec![entry(Some("a"), None)],
            ..AllowList::default()
        };
        let anchors = BTreeSet::from(["sort-order".to_owned()]);
        let fixtures = BTreeSet::from(["err/x".to_owned()]);
        assert_eq!(list.validate(&anchors, &fixtures, &BTreeSet::new()), Ok(()));
    }

    fn corpus_entry(fixture: &str, upstream: Option<&str>, workflint: Option<&str>) -> Entry {
        Entry {
            fixture: fixture.into(),
            ..entry(upstream, workflint)
        }
    }

    /// Validates `entries` with the anchor `sort-order`, the fixture
    /// `err/x`, and the corpus file `a_ci.yml`.
    fn validate_corpus(entries: Vec<Entry>) -> Result<(), String> {
        let list = AllowList {
            entries,
            ..AllowList::default()
        };
        list.validate(
            &BTreeSet::from(["sort-order".to_owned()]),
            &BTreeSet::from(["err/x".to_owned()]),
            &BTreeSet::from(["a_ci.yml".to_owned()]),
        )
    }

    #[test]
    fn an_entry_for_a_file_in_the_corpus_archive_passes_validation() {
        let replaced = corpus_entry(
            "realworld/a_ci.yml",
            Some("a_ci.yml:1:2: a [x]"),
            Some(r"/a_ci\.yml:1:3: a/"),
        );
        assert_eq!(validate_corpus(vec![replaced]), Ok(()));
    }

    #[test]
    fn an_entry_for_a_file_that_is_not_in_the_corpus_archive_fails_validation() {
        let missing = corpus_entry("realworld/b_ci.yml", Some("b_ci.yml:1:2: a [x]"), None);
        let error = validate_corpus(vec![missing]).unwrap_err();
        assert!(
            error.contains("corpus file \"b_ci.yml\" of the entry for anchor sort-order is not in"),
            "{error}"
        );
    }

    #[test]
    fn a_line_of_a_corpus_entry_must_be_on_its_file() {
        for line in [
            "b_ci.yml:1:2: a [x]",
            r"/b_ci\.yml:1:2: a/",
            "/a_ci.yml/",
            "no position",
        ] {
            for (upstream, workflint) in [(Some(line), None), (None, Some(line))] {
                let elsewhere = corpus_entry("realworld/a_ci.yml", upstream, workflint);
                let error = validate_corpus(vec![elsewhere]).unwrap_err();
                assert!(
                    error.contains("is not a line of a_ci.yml"),
                    "{line}: {error}"
                );
            }
        }
    }

    #[test]
    fn an_entry_for_every_corpus_file_needs_an_upstream_pattern_and_a_replacement() {
        let valid = corpus_entry("realworld/*", Some(r"/old \[x\]/"), Some("new [x]"));
        assert_eq!(validate_corpus(vec![valid]), Ok(()));
        for (upstream, workflint) in [
            (Some("a_ci.yml:1:2: old [x]"), Some("new [x]")),
            (Some(r"/old \[x\]/"), None),
            (None, Some("new [x]")),
            (Some("/(/"), Some("new [x]")),
        ] {
            let invalid = corpus_entry("realworld/*", upstream, workflint);
            let error = validate_corpus(vec![invalid]).unwrap_err();
            assert!(error.contains("realworld/*"), "{upstream:?}: {error}");
        }
    }

    #[test]
    fn the_corpus_entries_of_selected_files_come_before_the_entries_for_every_file() {
        let list = AllowList {
            entries: vec![
                corpus_entry("realworld/*", Some("/a/"), Some("b")),
                corpus_entry("realworld/b.yml", Some("b.yml:1:1: b [x]"), None),
                corpus_entry("realworld/a.yml", Some("a.yml:1:1: a [x]"), None),
                corpus_entry("realworld/c.yml", Some("c.yml:1:1: c [x]"), None),
                entry(Some("a.yml:1:1: a [x]"), None),
            ],
            ..AllowList::default()
        };
        let names = ["a.yml".to_owned(), "b.yml".to_owned()];
        let fixtures: Vec<&str> = list
            .corpus_entries(&names)
            .iter()
            .map(|entry| entry.fixture.as_str())
            .collect();
        assert_eq!(
            fixtures,
            ["realworld/b.yml", "realworld/a.yml", "realworld/*"]
        );
    }

    #[test]
    fn an_entry_names_its_target() {
        assert_eq!(entry(None, None).target(), Target::Fixture("err/x"));
        assert_eq!(
            corpus_entry("realworld/a.yml", None, None).target(),
            Target::CorpusFile("a.yml")
        );
        assert_eq!(
            corpus_entry("realworld/*", None, None).target(),
            Target::EveryCorpusFile
        );
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
    fn a_pattern_line_that_replaces_a_line_at_its_position_takes_its_place() {
        let expected = lines(&[
            "/test.yaml:1:1: a/",
            "/test.yaml:1:1: b/",
            "/test.yaml:1:1: c/",
        ]);
        let replaced = entry(Some("/test.yaml:1:1: a/"), Some("/test.yaml:1:1: A/"));
        assert_eq!(
            apply(&expected, &[&replaced]).unwrap(),
            lines(&[
                "/test.yaml:1:1: A/",
                "/test.yaml:1:1: b/",
                "/test.yaml:1:1: c/"
            ])
        );
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
