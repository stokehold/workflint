//! Drains the event parser on arbitrary input. See `check_events`.

#![no_main]

use libfuzzer_sys::fuzz_target;

#[allow(dead_code)]
#[path = "../../crates/workflint-yaml/tests/invariants/mod.rs"]
mod invariants;

fuzz_target!(|data: &[u8]| {
    invariants::check_events(&invariants::text(data));
});
