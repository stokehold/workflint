//! Lints arbitrary input as a workflow of a project with the driver of
//! `workflint-linter`, the full lint of one file, and checks the result. See
//! `check_lint`. The driver turns a panic into an `internal-error`
//! diagnostic, but libFuzzer's panic hook aborts first, so a panic is a
//! crash. An input that starts with the byte `C0` is first expanded into a
//! deeply nested one, as `nesting.rs` describes.

#![no_main]

use std::path::PathBuf;
use std::sync::LazyLock;

use libfuzzer_sys::fuzz_target;

#[path = "../../crates/workflint-linter/tests/invariants/mod.rs"]
mod invariants;

#[path = "../../crates/workflint-yaml/tests/invariants/nesting.rs"]
mod nesting;

/// The project in the temporary directory, one per fuzzing process.
static PROJECT: LazyLock<PathBuf> = LazyLock::new(|| {
    let root = std::env::temp_dir().join(format!("workflint-fuzz-lint-{}", std::process::id()));
    invariants::write_project(&root);
    root
});

fuzz_target!(|data: &[u8]| {
    invariants::check_lint(&PROJECT, &nesting::expand(data));
});
