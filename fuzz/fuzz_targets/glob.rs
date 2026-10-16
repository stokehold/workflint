//! Validates the input as a branch or tag pattern and as a path pattern,
//! with invalid UTF-8 replaced by U+FFFD.
//!
//! The checks: validation does not panic, and every problem has a message.
//! Each range covers one character of the pattern, and the ranges never move
//! backwards. An empty pattern has exactly one problem.

#![no_main]

use libfuzzer_sys::fuzz_target;
use workflint_syntax::glob::{GlobProblem, validate_path_glob, validate_ref_glob};

fuzz_target!(|data: &[u8]| {
    let pattern = String::from_utf8_lossy(data);
    check(&pattern, &validate_ref_glob(&pattern));
    check(&pattern, &validate_path_glob(&pattern));
});

fn check(pattern: &str, problems: &[GlobProblem]) {
    if pattern.is_empty() {
        assert_eq!(problems.len(), 1);
    }
    let mut previous = 0;
    for problem in problems {
        assert!(!problem.message.is_empty());
        let Some(range) = &problem.range else {
            continue;
        };
        let Some(text) = pattern.get(range.clone()) else {
            panic!("{range:?} is not a character range of {pattern:?}");
        };
        assert_eq!(text.chars().count(), 1, "{range:?} in {pattern:?}");
        assert!(range.start >= previous, "{range:?} in {pattern:?}");
        previous = range.start;
    }
}
