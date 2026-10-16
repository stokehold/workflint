//! Decides whether a fixture's test passes, given what workflint printed, the
//! expected-failure list and the allow-list.

use super::allow_list::{Entry, apply};
use super::matcher::check_errors;
use super::output::Diagnostic;

/// How a fixture's test ends when it does not fail.
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The output matches.
    Passed,
    /// The output does not match, and the expected-failure list says so.
    ExpectedFailure { milestone: String, reason: String },
}

/// Judges one fixture.
///
/// `actual` is workflint's output with kinds in the brackets, or the reason
/// it could not be read. `listed` is the fixture's milestone in
/// `expected-failures.toml`.
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
    listed: Option<&str>,
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
        (Ok(()), Some(milestone)) => Err(format!(
            "{fixture} passes, but tests/conformance/expected-failures.toml lists it as an expected failure until {milestone}. Remove its entry from that file."
        )),
        (Err(reason), Some(milestone)) => Ok(Verdict::ExpectedFailure {
            milestone: milestone.to_owned(),
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
            Some("M2"),
        );
        assert_eq!(
            verdict,
            Ok(Verdict::ExpectedFailure {
                milestone: "M2".into(),
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
            Some("M2"),
        )
        .unwrap_err();
        assert!(error.contains("err/x passes"), "{error}");
        assert!(error.contains("until M2"), "{error}");
        assert!(error.contains("Remove its entry"), "{error}");
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
            Some("M3"),
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
            Some("M3"),
        );
        assert!(verdict.unwrap_err().contains("matches no line"));
    }
}
