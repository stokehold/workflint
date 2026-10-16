//! Drains the event parser on arbitrary input. See `check_events`. An input
//! that starts with the byte `C0` is first expanded into a deeply nested one,
//! as `nesting.rs` describes.

#![no_main]

use libfuzzer_sys::fuzz_target;

#[allow(dead_code)]
#[path = "../../crates/workflint-yaml/tests/invariants/mod.rs"]
mod invariants;

#[path = "../../crates/workflint-yaml/tests/invariants/nesting.rs"]
mod nesting;

fuzz_target!(|data: &[u8]| {
    invariants::check_events(&invariants::text(&nesting::expand(data)));
});
