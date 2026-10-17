//! The check that the `glob` fuzz target in `fuzz/` runs on each input.
//!
//! The fuzz target and `fuzz_regressions.rs` include this file with
//! `#[path]`, so a crash input found by the fuzzer fails the same check as
//! a test. The check panics when an invariant does not hold.

use workflint_syntax::glob::{GlobProblem, validate_path_glob, validate_ref_glob};

/// Validates `pattern` as a branch or tag pattern and as a path pattern.
///
/// Validation does not panic, and every problem has a message. Each range
/// covers one character of the pattern, and the ranges never move
/// backwards. An empty pattern has exactly one problem.
pub(crate) fn check_glob(pattern: &str) {
    check_problems(pattern, &validate_ref_glob(pattern));
    check_problems(pattern, &validate_path_glob(pattern));
}

fn check_problems(pattern: &str, problems: &[GlobProblem]) {
    if pattern.is_empty() {
        assert_eq!(problems.len(), 1);
    }
    let mut previous = 0;
    for problem in problems {
        assert_ne!(problem.message, "");
        let Some(range) = &problem.range else {
            continue;
        };
        let text = pattern.get(range.clone());
        assert!(
            text.is_some(),
            "{range:?} is not a character range of {pattern:?}"
        );
        let text = text.unwrap_or_default();
        assert_eq!(text.chars().count(), 1, "{range:?} in {pattern:?}");
        assert!(range.start >= previous, "{range:?} in {pattern:?}");
        previous = range.start;
    }
}
