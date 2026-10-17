//! `tests/conformance/expected-failures.toml`: the fixtures that do not pass
//! yet, each with the milestone expected to make it pass.

use std::collections::{BTreeMap, BTreeSet};

use super::checklist::{Item, milestone_number};

/// The parsed file, from fixture ID to milestone.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ExpectedFailures {
    /// The milestone of each fixture, by fixture ID.
    pub fixtures: BTreeMap<String, String>,
}

impl ExpectedFailures {
    /// Parses lines of the form `"err/name" = "M2"`.
    ///
    /// # Errors
    ///
    /// Returns the TOML error, or a message for a value that is not a
    /// milestone such as `"M2"`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let table: toml::Table = text.parse().map_err(|error| format!("{error}"))?;
        let mut fixtures = BTreeMap::new();
        for (fixture, milestone) in table {
            match milestone {
                toml::Value::String(milestone) if milestone_number(&milestone).is_some() => {
                    fixtures.insert(fixture, milestone);
                }
                other => {
                    return Err(format!(
                        "fixture {fixture:?} has {other:?}, not a milestone such as \"M2\""
                    ));
                }
            }
        }
        Ok(Self { fixtures })
    }

    /// Checks that every listed fixture exists.
    ///
    /// # Errors
    ///
    /// Returns one line per unknown fixture ID.
    pub fn validate(&self, fixtures: &BTreeSet<String>) -> Result<(), String> {
        let unknown: Vec<String> = self
            .fixtures
            .keys()
            .filter(|fixture| !fixtures.contains(*fixture))
            .map(|fixture| {
                format!("expected-failures.toml lists {fixture:?}, which does not exist")
            })
            .collect();
        if unknown.is_empty() {
            Ok(())
        } else {
            Err(unknown.join("\n"))
        }
    }

    /// Checks the milestone of every entry against the checklist `items`
    /// and the `closed` milestones, by the rule in
    /// `tests/conformance/README.md`:
    ///
    /// - An entry names the milestone that [`rule_milestone`] gives.
    /// - An entry does not name a closed milestone.
    /// - A note that says an item waits on a fixture gives the fixture's
    ///   milestone, and names only listed fixtures.
    ///
    /// # Errors
    ///
    /// Returns one line per problem.
    pub fn check_milestones(&self, items: &[Item], closed: &BTreeSet<u32>) -> Result<(), String> {
        let mut problems = Vec::new();
        for (fixture, milestone) in &self.fixtures {
            let Some(number) = milestone_number(milestone) else {
                continue;
            };
            if closed.contains(&number) {
                problems.push(format!(
                    "expected-failures.toml lists {fixture:?} until {milestone}, but {milestone} is closed (tests/conformance/milestones.toml). Fix the fixture, or give it the milestone of the work that makes it pass."
                ));
            }
            let (expected, lines) = rule_milestone(fixture, items);
            if expected != number {
                problems.push(format!(
                    "expected-failures.toml lists {fixture:?} until {milestone}, but the parity checklist gives M{expected}: {}",
                    describe(&lines)
                ));
            }
        }
        for item in items {
            for (fixture, milestone) in &item.waits_on {
                match self.fixtures.get(fixture) {
                    None => problems.push(format!(
                        "docs/parity-checklist.md:{}: the notes say the row waits on {fixture:?}, which passes. Remove it from the notes.",
                        item.line
                    )),
                    Some(listed) if milestone_number(listed) != Some(*milestone) => {
                        problems.push(format!(
                            "docs/parity-checklist.md:{}: the notes say {fixture:?} passes in M{milestone}, but expected-failures.toml says {listed}",
                            item.line
                        ));
                    }
                    Some(_) => {}
                }
            }
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems.join("\n"))
        }
    }
}

/// The milestone that the rule in `tests/conformance/README.md` gives a
/// listed `fixture`, with the lines of the items it comes from: the latest
/// milestone of the items that cite the fixture, or 5 if none does. An item
/// whose notes say it waits on the fixture does not count, because its own
/// check is done.
#[must_use]
pub fn rule_milestone(fixture: &str, items: &[Item]) -> (u32, Vec<usize>) {
    let citing: Vec<(u32, usize)> = items
        .iter()
        .filter(|item| item.fixtures.iter().any(|cited| cited == fixture))
        .filter(|item| !item.waits_on.iter().any(|(waited, _)| waited == fixture))
        .filter_map(|item| Some((item.milestone?, item.line)))
        .collect();
    match citing.iter().map(|(milestone, _)| *milestone).max() {
        None => (5, Vec::new()),
        Some(latest) => (
            latest,
            citing
                .iter()
                .filter(|(milestone, _)| *milestone == latest)
                .map(|(_, line)| *line)
                .collect(),
        ),
    }
}

fn describe(lines: &[usize]) -> String {
    if lines.is_empty() {
        return "no row cites the fixture, so it is M5".to_owned();
    }
    let lines: Vec<String> = lines.iter().map(ToString::to_string).collect();
    let noun = if lines.len() == 1 { "line" } else { "lines" };
    format!(
        "that is the milestone of the row on {noun} {} of docs/parity-checklist.md",
        lines.join(", ")
    )
}

/// Parses `tests/conformance/milestones.toml`: `closed = ["M1", ...]`.
///
/// # Errors
///
/// Returns the TOML error, or a message if `closed` is missing or holds
/// something other than milestones.
pub fn closed_milestones(text: &str) -> Result<BTreeSet<u32>, String> {
    let table: toml::Table = text.parse().map_err(|error| format!("{error}"))?;
    let Some(toml::Value::Array(closed)) = table.get("closed") else {
        return Err("`closed` is missing or not an array".to_owned());
    };
    closed
        .iter()
        .map(|value| {
            value
                .as_str()
                .and_then(milestone_number)
                .ok_or_else(|| format!("{value:?} in `closed` is not a milestone such as \"M2\""))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_map_fixtures_to_milestones() {
        let list = ExpectedFailures::parse("\"err/a\" = \"M2\"\n\"ok/b.yaml\" = \"M5\"\n").unwrap();
        assert_eq!(list.fixtures["err/a"], "M2");
        assert_eq!(list.fixtures["ok/b.yaml"], "M5");
        assert_eq!(list.fixtures.len(), 2);
    }

    #[test]
    fn a_value_that_is_not_a_milestone_is_an_error() {
        assert!(ExpectedFailures::parse("\"err/a\" = \"soon\"\n").is_err());
        assert!(ExpectedFailures::parse("\"err/a\" = 2\n").is_err());
        assert!(ExpectedFailures::parse("\"err/a\" = \"M\"\n").is_err());
    }

    #[test]
    fn a_listed_fixture_that_does_not_exist_fails_validation() {
        let list = ExpectedFailures::parse("\"err/gone\" = \"M2\"\n").unwrap();
        let error = list
            .validate(&BTreeSet::from(["err/a".to_owned()]))
            .unwrap_err();
        assert!(error.contains("\"err/gone\""), "{error}");
    }

    fn item(line: usize, milestone: u32, fixtures: &[&str]) -> Item {
        Item {
            line,
            milestone: Some(milestone),
            fixtures: fixtures.iter().map(|f| (*f).to_owned()).collect(),
            ..Item::default()
        }
    }

    fn list(entries: &[(&str, &str)]) -> ExpectedFailures {
        ExpectedFailures {
            fixtures: entries
                .iter()
                .map(|(fixture, milestone)| ((*fixture).to_owned(), (*milestone).to_owned()))
                .collect(),
        }
    }

    #[test]
    fn the_rule_takes_the_latest_row_that_cites_the_fixture() {
        let items = [
            item(10, 2, &["err/a"]),
            item(11, 4, &["err/a", "err/b"]),
            item(12, 3, &["err/a"]),
        ];
        assert_eq!(rule_milestone("err/a", &items), (4, vec![11]));
        assert_eq!(rule_milestone("err/c", &items), (5, vec![]));
    }

    #[test]
    fn a_row_that_waits_on_the_fixture_does_not_count() {
        let mut waiting = item(10, 5, &["err/a", "examples/main"]);
        waiting.waits_on = vec![("err/a".to_owned(), 4), ("examples/main".to_owned(), 5)];
        let items = [waiting, item(11, 4, &["err/a"])];
        assert_eq!(rule_milestone("err/a", &items), (4, vec![11]));
        let listed = list(&[("err/a", "M4"), ("examples/main", "M5")]);
        assert_eq!(listed.check_milestones(&items, &BTreeSet::new()), Ok(()));
    }

    #[test]
    fn an_entry_with_another_milestone_than_the_rule_fails() {
        let items = [item(10, 4, &["err/a"])];
        let error = list(&[("err/a", "M3")])
            .check_milestones(&items, &BTreeSet::new())
            .unwrap_err();
        assert!(
            error.contains("lists \"err/a\" until M3, but the parity checklist gives M4"),
            "{error}"
        );
        assert!(error.contains("line 10"), "{error}");
    }

    #[test]
    fn an_entry_with_a_closed_milestone_fails() {
        let items = [item(10, 3, &["err/a"])];
        let error = list(&[("err/a", "M3")])
            .check_milestones(&items, &BTreeSet::from([1, 2, 3]))
            .unwrap_err();
        assert!(error.contains("M3 is closed"), "{error}");
    }

    #[test]
    fn a_wait_note_with_another_milestone_or_a_passing_fixture_fails() {
        let mut waiting = item(10, 5, &["err/a", "err/b"]);
        waiting.waits_on = vec![("err/a".to_owned(), 3), ("err/b".to_owned(), 4)];
        let items = [waiting, item(11, 4, &["err/a"])];
        let error = list(&[("err/a", "M4")])
            .check_milestones(&items, &BTreeSet::new())
            .unwrap_err();
        assert!(
            error.contains(
                ":10: the notes say \"err/a\" passes in M3, but expected-failures.toml says M4"
            ),
            "{error}"
        );
        assert!(
            error.contains(":10: the notes say the row waits on \"err/b\", which passes"),
            "{error}"
        );
    }

    #[test]
    fn closed_milestones_are_a_list_of_milestones() {
        assert_eq!(
            closed_milestones("closed = [\"M1\", \"M3\"]\n"),
            Ok(BTreeSet::from([1, 3]))
        );
        assert!(closed_milestones("closed = [\"soon\"]\n").is_err());
        assert!(closed_milestones("open = []\n").is_err());
    }
}
