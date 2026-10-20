//! The action that runs a JavaScript snippet from its `script` input.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: the `strings.HasPrefix` checks in
//! `rule_expression.go`, line 295, where the `script` input is checked as a script for untrusted
//! input, and line 348, where the step's outputs accept any name because the script can call
//! `core.setOutput`. Until M7 replaces it with a generated file (ADR 0011), keep it equal to that
//! code. The test `github_script_matches_the_reference_prefix_checks` compares them.
//!
//! <https://github.com/actions/github-script>

/// The `owner/repo` of the action, without a ref.
pub const GITHUB_SCRIPT_ACTION: &str = "actions/github-script";

/// Returns whether `spec`, the `uses:` of a step, names [`GITHUB_SCRIPT_ACTION`] at any ref,
/// such as `actions/github-script@v7`. The comparison is case-sensitive, as in the reference.
#[must_use]
pub fn is_github_script(spec: &str) -> bool {
    spec.strip_prefix(GITHUB_SCRIPT_ACTION)
        .is_some_and(|rest| rest.starts_with('@'))
}
