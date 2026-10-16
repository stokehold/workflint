//! Runs the checks of the fuzz targets on each input in
//! `tests/fuzz-regressions/`.
//!
//! Each file is an input that once made a fuzz target fail, or one that
//! covers an edge case of the checks. `CONTRIBUTING.md` describes how to add a
//! crash input found by `cargo fuzz`. As in the fuzz targets, an input that
//! starts with the byte `C0` is expanded first, as `invariants/nesting.rs`
//! describes. The `depth-*` inputs expand to the parser's depth limit.

mod invariants;

#[path = "invariants/nesting.rs"]
mod nesting;

use datatest_stable::Utf8Path;

// datatest-stable passes the file contents as an owned `Vec<u8>`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn fuzz_regression(_path: &Utf8Path, data: Vec<u8>) -> datatest_stable::Result<()> {
    let data = nesting::expand(&data);
    let text = invariants::text(&data);
    invariants::check_events(&text);
    invariants::check_tree(&text);
    Ok(())
}

datatest_stable::harness! {
    { test = fuzz_regression, root = "tests/fuzz-regressions", pattern = r".*" },
}
