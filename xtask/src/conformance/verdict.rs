//! Decides whether a fixture's test passes, given what the binary printed, the
//! expected-failure list and the allow-list.

use super::allow_list::{Entry, apply};
use super::matcher::check_errors;
use super::output::Diagnostic;

/// Why a fixture is expected to fail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Listing {
    /// `expected-failures.toml` lists the fixture until this milestone.
    Until(String),
    /// The reference binary fails the fixture, for this reason. See
    /// [`super::runner::REFERENCE_FAILURES`].
    Reference(String),
}

/// How a fixture's test ends when it does not fail.
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The output matches.
    Passed,
    /// The output does not match, and a list of expected failures says so.
    ExpectedFailure {
        /// Why the fixture is expected to fail.
        listing: Listing,
        /// Why the test would fail without the entry.
        reason: String,
    },
}

/// Judges one fixture.
///
/// `actual` is the binary's output with kinds in the brackets, or the reason
/// it could not be read. `listed` says why the fixture is expected to fail,
/// if it is.
///
/// # Errors
///
/// Returns the reason the test fails:
///
/// - The output does not match and the fixture is not listed.
/// - The output matches but the fixture is listed. The message says to
///   remove the entry.
/// - An allow-list entry does not apply to the `.out` file.
/// - The fixture passes and also passes without one of its allow-list
///   entries, so that entry is unused.
pub fn judge(
    fixture: &str,
    upstream: &[String],
    actual: Result<&[Diagnostic], &str>,
    entries: &[&Entry],
    listed: Option<&Listing>,
) -> Result<Verdict, String> {
    let expected = apply(upstream, entries)?;
    let result = actual
        .map_err(str::to_owned)
        .and_then(|actual| check_errors(&expected, actual));
    match (result, listed) {
        (Ok(()), None) => {
            check_entries_are_used(upstream, actual.unwrap_or_default(), entries)?;
            Ok(Verdict::Passed)
        }
        (Ok(()), Some(Listing::Until(milestone))) => Err(format!(
            "{fixture} passes, but tests/conformance/expected-failures.toml lists it as an expected failure until {milestone}. Remove its entry from that file."
        )),
        (Ok(()), Some(Listing::Reference(_))) => Err(format!(
            "{fixture} passes with the reference, but REFERENCE_FAILURES in xtask/src/conformance/runner.rs lists it as a known failure. Remove its entry from that list."
        )),
        (Err(reason), Some(listing)) => Ok(Verdict::ExpectedFailure {
            listing: listing.clone(),
            reason,
        }),
        (Err(reason), None) => Err(reason),
    }
}

fn check_entries_are_used(
    upstream: &[String],
    actual: &[Diagnostic],
    entries: &[&Entry],
) -> Result<(), String> {
    for (index, entry) in entries.iter().enumerate() {
        let others: Vec<&Entry> = entries
            .iter()
            .enumerate()
            .filter(|(other, _)| *other != index)
            .map(|(_, entry)| *entry)
            .collect();
        let Ok(without) = apply(upstream, &others) else {
            continue;
        };
        if check_errors(&without, actual).is_ok() {
            return Err(format!(
                "the allowed-differences.toml entry for {} with anchor {} is unused: the fixture passes without it. Remove the entry.",
                entry.fixture, entry.anchor
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance::output::parse_line;

    fn diagnostics(lines: &[&str]) -> Vec<Diagnostic> {
        lines.iter().map(|line| parse_line(line).unwrap()).collect()
    }

    fn lines(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|line| (*line).to_owned()).collect()
    }

    fn until(milestone: &str) -> Listing {
        Listing::Until(milestone.to_owned())
    }

    fn entry(upstream: Option<&str>, workflint: Option<&str>) -> Entry {
        Entry {
            anchor: "yaml-error-positions".into(),
            fixture: "err/x".into(),
            upstream: upstream.map(str::to_owned),
            workflint: workflint.map(str::to_owned),
        }
    }

    #[test]
    fn a_matching_unlisted_fixture_passes() {
        let verdict = judge(
            "err/x",
            &lines(&["test.yaml:1:1: a [x]"]),
            Ok(&diagnostics(&["test.yaml:1:1: a [x]"])),
            &[],
            None,
        );
        assert_eq!(verdict, Ok(Verdict::Passed));
    }

    #[test]
    fn a_mismatching_unlisted_fixture_fails() {
        let verdict = judge(
            "err/x",
            &lines(&["test.yaml:1:1: a [x]"]),
            Ok(&diagnostics(&[])),
            &[],
            None,
        );
        assert!(verdict.unwrap_err().contains("1 errors are expected"));
    }

    #[test]
    fn a_mismatching_listed_fixture_is_an_expected_failure() {
        let verdict = judge(
            "err/x",
            &lines(&["test.yaml:1:1: a [x]"]),
            Err("output line is not ..."),
            &[],
            Some(&until("M2")),
        );
        assert_eq!(
            verdict,
            Ok(Verdict::ExpectedFailure {
                listing: until("M2"),
                reason: "output line is not ...".into(),
            })
        );
    }

    #[test]
    fn a_listed_fixture_that_passes_fails_and_says_to_remove_the_entry() {
        let error = judge(
            "err/x",
            &lines(&["test.yaml:1:1: a [x]"]),
            Ok(&diagnostics(&["test.yaml:1:1: a [x]"])),
            &[],
            Some(&until("M2")),
        )
        .unwrap_err();
        assert!(error.contains("err/x passes"), "{error}");
        assert!(error.contains("until M2"), "{error}");
        assert!(error.contains("Remove its entry"), "{error}");
    }

    #[test]
    fn a_known_failure_of_the_reference_that_passes_says_to_remove_it() {
        let error = judge(
            "projects/x",
            &[],
            Ok(&[]),
            &[],
            Some(&Listing::Reference("why".into())),
        )
        .unwrap_err();
        assert!(error.contains("passes with the reference"), "{error}");
        assert!(error.contains("REFERENCE_FAILURES"), "{error}");
    }

    #[test]
    fn an_allowed_difference_makes_a_fixture_pass() {
        let replaced = entry(Some("test.yaml:1:1: a [x]"), Some("test.yaml:1:2: a [x]"));
        let verdict = judge(
            "err/x",
            &lines(&["test.yaml:1:1: a [x]"]),
            Ok(&diagnostics(&["test.yaml:1:2: a [x]"])),
            &[&replaced],
            None,
        );
        assert_eq!(verdict, Ok(Verdict::Passed));
    }

    #[test]
    fn an_unused_entry_for_a_passing_fixture_fails() {
        // The fixture passes with the original line too, so the entry is not needed.
        let unused = entry(Some("test.yaml:1:1: a [x]"), Some("/a/"));
        let error = judge(
            "err/x",
            &lines(&["test.yaml:1:1: a [x]"]),
            Ok(&diagnostics(&["test.yaml:1:1: a [x]"])),
            &[&unused],
            None,
        )
        .unwrap_err();
        assert!(error.contains("is unused"), "{error}");
    }

    #[test]
    fn an_unused_entry_for_a_listed_fixture_is_not_reported() {
        let unused = entry(Some("test.yaml:1:1: a [x]"), Some("/a/"));
        let verdict = judge(
            "err/x",
            &lines(&["test.yaml:1:1: a [x]"]),
            Ok(&diagnostics(&[])),
            &[&unused],
            Some(&until("M3")),
        );
        assert!(matches!(verdict, Ok(Verdict::ExpectedFailure { .. })));
    }

    #[test]
    fn an_entry_that_does_not_apply_fails_even_for_a_listed_fixture() {
        let stale = entry(Some("test.yaml:5:5: gone [x]"), None);
        let verdict = judge(
            "err/x",
            &lines(&["test.yaml:1:1: a [x]"]),
            Ok(&diagnostics(&[])),
            &[&stale],
            Some(&until("M3")),
        );
        assert!(verdict.unwrap_err().contains("matches no line"));
    }
}
