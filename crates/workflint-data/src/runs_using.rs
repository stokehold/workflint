//! The values that `runs.using` accepts in action metadata.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: the `switch` on `r.Using` in
//! `checkLocalActionRuns` in `rule_action.go`, lines 495-504. Until M7 replaces it with a
//! generated file (ADR 0011), keep it equal to that code. The tests
//! `runs_using_values_match_the_reference_switch` and
//! `runs_using_values_match_the_reference_message` compare them.
//!
//! <https://docs.github.com/en/actions/creating-actions/metadata-syntax-for-github-actions#runs>

/// The values, sorted as the reference's message lists them.
pub static RUNS_USING_VALUES: &[&str] = &["composite", "docker", "node20", "node24"];

/// Returns whether `using` is one of [`RUNS_USING_VALUES`]. The comparison is case-sensitive.
#[must_use]
pub fn is_runs_using_value(using: &str) -> bool {
    RUNS_USING_VALUES.contains(&using)
}
