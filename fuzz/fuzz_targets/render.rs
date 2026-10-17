//! Builds a `SourceFile` from arbitrary text and renders diagnostics with
//! arbitrary spans in the default and one-line formats. See `check_render`,
//! which also describes the input layout.

#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../crates/workflint-diagnostics/tests/invariants/mod.rs"]
mod invariants;

fuzz_target!(|data: &[u8]| {
    invariants::check_render(data);
});
