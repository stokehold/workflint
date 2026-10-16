//! `tests/conformance/expected-failures.toml`: the fixtures that do not pass
//! yet, each with the milestone expected to make it pass.

use std::collections::{BTreeMap, BTreeSet};

/// The parsed file, from fixture ID to milestone.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ExpectedFailures {
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
                toml::Value::String(milestone) if is_milestone(&milestone) => {
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

    /// The milestone of a listed fixture.
    pub fn milestone(&self, fixture: &str) -> Option<&str> {
        self.fixtures.get(fixture).map(String::as_str)
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
}

fn is_milestone(text: &str) -> bool {
    text.strip_prefix('M')
        .is_some_and(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_map_fixtures_to_milestones() {
        let list = ExpectedFailures::parse("\"err/a\" = \"M2\"\n\"ok/b.yaml\" = \"M5\"\n").unwrap();
        assert_eq!(list.milestone("err/a"), Some("M2"));
        assert_eq!(list.milestone("ok/b.yaml"), Some("M5"));
        assert_eq!(list.milestone("err/c"), None);
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
}
