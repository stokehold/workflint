//! Validates the input as a branch or tag pattern and as a path pattern,
//! with invalid UTF-8 replaced by U+FFFD. See `check_glob`.

#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../crates/workflint-syntax/tests/invariants/glob.rs"]
mod invariants;

fuzz_target!(|data: &[u8]| {
    invariants::check_glob(&String::from_utf8_lossy(data));
});
