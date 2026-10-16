//! Parses arbitrary input with the YAML layer and the workflow parser and
//! checks the diagnostics. See `check_parse`. An input that starts with the
//! byte `C0` is first expanded into a deeply nested one, as `nesting.rs`
//! describes.

#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../crates/workflint-syntax/tests/invariants/mod.rs"]
mod invariants;

#[path = "../../crates/workflint-yaml/tests/invariants/nesting.rs"]
mod nesting;

fuzz_target!(|data: &[u8]| {
    invariants::check_parse(&String::from_utf8_lossy(&nesting::expand(data)));
});
