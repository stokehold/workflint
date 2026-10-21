//! Parses the input as a Go template, with invalid UTF-8 replaced by U+FFFD, and checks the
//! result. See `check_parse`.

#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../crates/workflint-gotemplate/tests/invariants/mod.rs"]
mod invariants;

fuzz_target!(|data: &[u8]| {
    invariants::check_parse(&String::from_utf8_lossy(data));
});
