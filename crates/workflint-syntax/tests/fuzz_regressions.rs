//! Runs the checks of the `syntax`, `metadata`, and `glob` fuzz targets on
//! each input in the target's directory under `tests/fuzz-regressions/`.
//!
//! Each file is an input that once made a fuzz target fail, or one that
//! covers an edge case of the checks. `CONTRIBUTING.md` describes how to add
//! a crash input found by `cargo fuzz`. As in the fuzz targets, invalid UTF-8
//! becomes U+FFFD, and a `syntax` or `metadata` input that starts with the
//! byte `C0` is expanded first, as `workflint-yaml`'s
//! `tests/invariants/nesting.rs` describes.

mod invariants;

#[path = "invariants/glob.rs"]
mod glob;

#[path = "../../workflint-yaml/tests/invariants/nesting.rs"]
mod nesting;

use datatest_stable::Utf8Path;

// datatest-stable passes the file contents as an owned `Vec<u8>`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn syntax(_path: &Utf8Path, data: Vec<u8>) -> datatest_stable::Result<()> {
    invariants::check_parse(&String::from_utf8_lossy(&nesting::expand(&data)));
    Ok(())
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn metadata(_path: &Utf8Path, data: Vec<u8>) -> datatest_stable::Result<()> {
    invariants::check_metadata(&String::from_utf8_lossy(&nesting::expand(&data)));
    Ok(())
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn glob(_path: &Utf8Path, data: Vec<u8>) -> datatest_stable::Result<()> {
    glob::check_glob(&String::from_utf8_lossy(&data));
    Ok(())
}

datatest_stable::harness! {
    { test = syntax, root = "tests/fuzz-regressions/syntax", pattern = r".*" },
    { test = metadata, root = "tests/fuzz-regressions/metadata", pattern = r".*" },
    { test = glob, root = "tests/fuzz-regressions/glob", pattern = r".*" },
}
