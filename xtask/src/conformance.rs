//! The conformance runner (ADR 0004). It lints the fixtures in
//! `tests/conformance/` with the `workflint` binary and compares the output
//! with each fixture's expected errors, by the rules of `checkErrors` in the
//! reference's `linter_test.go`. `tests/conformance/README.md` documents the
//! command line the runner uses and the data files it reads.
//!
//! `cargo xtask oracle` reuses the command line, the output parsing, the
//! matching, the kind mapping and the allow-list to compare workflint with the
//! reference binary.

pub mod allow_list;
pub mod binary;
pub mod checklist;
pub mod expected_failures;
pub mod fixture;
pub mod matcher;
pub mod output;
pub mod rule_kinds;
pub mod runner;
pub mod verdict;

use std::path::{Path, PathBuf};

/// The repository root, found from the xtask manifest at compile time.
///
/// # Panics
///
/// Panics if the xtask manifest directory has no parent.
#[must_use]
pub fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives in a subdirectory of the repository")
        .to_path_buf()
}

/// `tests/conformance/` in the repository.
#[must_use]
pub fn conformance_dir() -> PathBuf {
    repository_root().join("tests").join("conformance")
}
