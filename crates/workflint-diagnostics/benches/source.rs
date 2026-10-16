//! Micro-benchmarks for the line index of a source file (ADR 0019).
//!
//! The input comes from `tests/conformance/actionlint/testdata/bench/`, which
//! is outside this crate, so the published package does not contain it.

#![allow(clippy::expect_used)]

use divan::Bencher;
use workflint_diagnostics::{FileId, SourceFile};

fn main() {
    divan::main();
}

const LARGE: &str = include_str!("../../../tests/conformance/actionlint/testdata/bench/large.yaml");

fn large() -> SourceFile {
    SourceFile::new(FileId::new(0), "large.yaml", LARGE)
        .expect("the benchmark input is far below the size limit")
}

/// Builds the line index of `large.yaml`. Copying the text into the source
/// file happens before the measurement.
#[divan::bench]
fn line_index(bencher: Bencher<'_, '_>) {
    bencher
        .with_inputs(|| LARGE.to_owned())
        .bench_local_values(|text| SourceFile::new(FileId::new(0), "large.yaml", text));
}

/// Converts the offset of every character in `large.yaml` to a line and
/// column.
#[divan::bench]
fn line_col(bencher: Bencher<'_, '_>) {
    let file = large();
    let offsets: Vec<u32> = LARGE
        .char_indices()
        .filter_map(|(offset, _)| u32::try_from(offset).ok())
        .collect();
    bencher.bench_local(|| {
        divan::black_box(&offsets)
            .iter()
            .map(|&offset| file.line_col(offset).column)
            .sum::<usize>()
    });
}
