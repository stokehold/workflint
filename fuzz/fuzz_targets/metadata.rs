//! Decodes arbitrary input as the metadata of a local action and of a
//! local reusable workflow, and checks the results. See `check_metadata`.
//! An input that starts with the byte `C0` is first expanded into a deeply
//! nested one, as `nesting.rs` describes.

#![no_main]

use libfuzzer_sys::fuzz_target;

#[allow(dead_code)]
#[path = "../../crates/workflint-syntax/tests/invariants/mod.rs"]
mod invariants;

#[path = "../../crates/workflint-yaml/tests/invariants/nesting.rs"]
mod nesting;

fuzz_target!(|data: &[u8]| {
    invariants::check_metadata(&String::from_utf8_lossy(&nesting::expand(data)));
});
