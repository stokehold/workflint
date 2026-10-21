//! Runs the check of the `gotemplate_parse` fuzz target on each input in
//! `tests/fuzz-regressions/`.
//!
//! Each file is an input that once made the fuzz target fail, or one that covers an edge case
//! of the parser: nesting at and past the depth limit, an action or comment cut off by the end
//! of the input, trim markers next to the end, and multibyte characters where the lexer backs
//! up. `CONTRIBUTING.md` describes how to add a crash input found by `cargo fuzz`. As in the
//! fuzz target, invalid UTF-8 becomes U+FFFD.

mod invariants;

use datatest_stable::Utf8Path;

// datatest-stable passes the file contents as an owned `Vec<u8>`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn fuzz_regression(_path: &Utf8Path, data: Vec<u8>) -> datatest_stable::Result<()> {
    invariants::check_parse(&String::from_utf8_lossy(&data));
    Ok(())
}

datatest_stable::harness! {
    { test = fuzz_regression, root = "tests/fuzz-regressions", pattern = r".*" },
}
