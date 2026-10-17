//! Type-checks `fromJSON('<input>')`, with invalid UTF-8 replaced by U+FFFD and each `'`
//! doubled, which runs the JSON scanner and the type builder of `workflint-expr` on the input.
//! See `check_json`.

#![no_main]

use libfuzzer_sys::fuzz_target;

#[allow(dead_code)]
#[path = "../../crates/workflint-expr/tests/invariants/mod.rs"]
mod invariants;

fuzz_target!(|data: &[u8]| {
    invariants::check_json(&String::from_utf8_lossy(data));
});
