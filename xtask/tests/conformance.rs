//! The conformance suite (ADR 0004): one test per fixture in
//! `tests/conformance/`, run through the `workflint` binary. See
//! `tests/conformance/README.md`.

#![allow(clippy::unnecessary_wraps)]

use std::fmt;
use std::path::Path;

use xtask::conformance::fixture::{Fixture, Origin};
use xtask::conformance::runner::{self, Outcome};
use xtask::conformance::verdict::{Listing, Verdict};

/// A failure message that the harness prints as written. datatest-stable
/// prints errors with `{:?}`, which would quote a `String`.
struct Failure(String);

impl fmt::Debug for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Failure {}

fn check(result: Result<(), String>) -> datatest_stable::Result<()> {
    result.map_err(|message| Failure(message).into())
}

fn lint(origin: Origin, path: &Path) -> datatest_stable::Result<()> {
    let fixture = Fixture::new(origin, path);
    match runner::run_fixture(&fixture) {
        Ok(Outcome::Judged(Verdict::Passed)) => Ok(()),
        Ok(Outcome::Judged(Verdict::ExpectedFailure { listing, reason })) => {
            let first = reason.lines().next().unwrap_or_default();
            match listing {
                Listing::Until(milestone) => {
                    println!(
                        "{}: expected failure until {milestone}: {first}",
                        fixture.id
                    );
                }
                Listing::Reference(why) => {
                    println!(
                        "{}: known failure of the reference ({why}): {first}",
                        fixture.id
                    );
                }
            }
            Ok(())
        }
        Ok(Outcome::Skipped(message)) => {
            println!("{message}");
            Ok(())
        }
        Err(message) => check(Err(format!("{}: {message}", fixture.id))),
    }
}

fn actionlint(path: &Path) -> datatest_stable::Result<()> {
    lint(Origin::Actionlint, path)
}

fn workflint(path: &Path) -> datatest_stable::Result<()> {
    lint(Origin::Workflint, path)
}

fn allowed_differences(_: &Path) -> datatest_stable::Result<()> {
    check(runner::check_allow_list())
}

fn expected_failures(_: &Path) -> datatest_stable::Result<()> {
    check(runner::check_expected_failures())
}

fn rule_kinds(_: &Path) -> datatest_stable::Result<()> {
    check(runner::check_rule_kinds())
}

/// Prints how many fixtures the runner skips and why. `.config/nextest.toml`
/// shows the output of this test when it passes.
fn skipped_fixtures(_: &Path) -> datatest_stable::Result<()> {
    check(runner::skip_summary().map(|summary| println!("{summary}")))
}

datatest_stable::harness! {
    { test = actionlint, root = "../tests/conformance/actionlint/testdata", pattern = r"^((err|examples|ok)/[^/]+\.ya?ml|projects/[^/]+\.out)$" },
    { test = workflint, root = "../tests/conformance/workflint", pattern = r"^((err|examples|ok)/[^/]+\.ya?ml|projects/[^/]+\.out)$" },
    { test = allowed_differences, root = "../tests/conformance", pattern = r"^allowed-differences\.toml$" },
    { test = expected_failures, root = "../tests/conformance", pattern = r"^expected-failures\.toml$" },
    { test = rule_kinds, root = "../tests/conformance", pattern = r"^rule-kinds\.toml$" },
    // The README says when the runner skips a fixture.
    { test = skipped_fixtures, root = "../tests/conformance", pattern = r"^README\.md$" },
}
