//! Runs the checks of the `lint` fuzz target on each input in
//! `tests/fuzz-regressions/`.
//!
//! Each file is an input that once made the fuzz target fail, or one that
//! covers an edge case of the checks. `CONTRIBUTING.md` describes how to add
//! a crash input found by `cargo fuzz`. As in the fuzz target, an input that
//! starts with the byte `C0` is expanded first, as `workflint-yaml`'s
//! `tests/invariants/nesting.rs` describes.

#![allow(clippy::unwrap_used)]

mod invariants;

#[path = "../../workflint-yaml/tests/invariants/nesting.rs"]
mod nesting;

use std::fs;
use std::path::Path;

#[test]
fn fuzz_regression_inputs_lint_without_an_internal_error() {
    let project = tempfile::tempdir().unwrap();
    invariants::write_project(project.path());
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fuzz-regressions");
    let mut paths: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    paths.sort();
    assert_ne!(paths.len(), 0, "no inputs");
    for path in paths {
        let data = fs::read(&path).unwrap();
        eprintln!("{}", path.display());
        invariants::check_lint(project.path(), &nesting::expand(&data));
    }
}
