//! Runs the check of the `render` fuzz target on each input in
//! `tests/fuzz-regressions/`.
//!
//! Each file is an input that once made the fuzz target fail, or one that
//! covers an edge case of the check. `CONTRIBUTING.md` describes how to add
//! a crash input found by `cargo fuzz`. `check_render` describes the input
//! layout.

mod invariants;

use datatest_stable::Utf8Path;

// datatest-stable passes the file contents as an owned `Vec<u8>`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn fuzz_regression(_path: &Utf8Path, data: Vec<u8>) -> datatest_stable::Result<()> {
    invariants::check_render(&data);
    Ok(())
}

datatest_stable::harness! {
    { test = fuzz_regression, root = "tests/fuzz-regressions", pattern = r".*" },
}
