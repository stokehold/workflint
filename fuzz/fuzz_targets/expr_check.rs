//! Parses the input as the text after `${{`, with invalid UTF-8 replaced by U+FFFD, and
//! type-checks the tree against the global contexts. See `check_types`.

#![no_main]

use libfuzzer_sys::fuzz_target;

#[allow(dead_code)]
#[path = "../../crates/workflint-expr/tests/invariants/mod.rs"]
mod invariants;

fuzz_target!(|data: &[u8]| {
    invariants::check_types(&String::from_utf8_lossy(data));
});
