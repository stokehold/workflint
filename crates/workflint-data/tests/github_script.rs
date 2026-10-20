//! Checks the name of `actions/github-script` against the `strings.HasPrefix` checks in the
//! vendored `rule_expression.go`.

mod go_source;

use go_source::uses_prefixes;
use workflint_data::{GITHUB_SCRIPT_ACTION, is_github_script};

#[test]
fn github_script_matches_the_reference_prefix_checks() {
    let prefix = format!("{GITHUB_SCRIPT_ACTION}@");
    assert_eq!(uses_prefixes(), [prefix.clone(), prefix]);
}

#[test]
fn github_script_lookup_is_a_case_sensitive_prefix_match() {
    assert!(is_github_script("actions/github-script@v7"));
    assert!(is_github_script("actions/github-script@"));
    assert!(!is_github_script("actions/github-script"));
    assert!(!is_github_script("actions/github-script/sub@v7"));
    assert!(!is_github_script("Actions/GitHub-Script@v7"));
}
