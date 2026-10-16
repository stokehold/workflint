//! Runs the checks of the expression fuzz targets on each input in `tests/fuzz-regressions/`.
//!
//! Each file is an input that once made a fuzz target fail, or one that covers an edge case of
//! the checks: a byte order mark, a NUL character, nesting at and past the parser's depth limit,
//! and the end of the input before `}}`. `CONTRIBUTING.md` describes how to add a crash input
//! found by `cargo fuzz`. As in the fuzz targets, invalid UTF-8 becomes U+FFFD.

mod invariants;

use datatest_stable::Utf8Path;

// datatest-stable passes the file contents as an owned `Vec<u8>`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn fuzz_regression(_path: &Utf8Path, data: Vec<u8>) -> datatest_stable::Result<()> {
    let src = String::from_utf8_lossy(&data);
    invariants::check_lex(&src);
    invariants::check_parse(&src);
    Ok(())
}

datatest_stable::harness! {
    { test = fuzz_regression, root = "tests/fuzz-regressions", pattern = r".*" },
}
