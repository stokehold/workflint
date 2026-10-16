//! Builds the span tree from arbitrary input and checks its invariants. See
//! `check_tree`.

#![no_main]

use libfuzzer_sys::fuzz_target;

#[allow(dead_code)]
#[path = "../../crates/workflint-yaml/tests/invariants/mod.rs"]
mod invariants;

fuzz_target!(|data: &[u8]| {
    invariants::check_tree(&invariants::text(data));
});
