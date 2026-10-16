//! Micro-benchmarks for parsing the reference's benchmark workflows to events
//! and to the span tree (ADR 0019).
//!
//! The inputs come from `tests/conformance/actionlint/testdata/bench/`, which
//! is outside this crate, so the published package does not contain them.

#![allow(clippy::expect_used)]

use std::fmt;

use divan::Bencher;
use workflint_diagnostics::{FileId, SourceFile};
use workflint_yaml::event::EventParser;
use workflint_yaml::parse;

fn main() {
    divan::main();
}

/// One benchmark input. `Display` gives the name divan shows for it.
struct Input {
    name: &'static str,
    text: &'static str,
}

impl fmt::Display for Input {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

const INPUTS: &[Input] = &[
    Input {
        name: "minimal.yaml",
        text: include_str!("../../../tests/conformance/actionlint/testdata/bench/minimal.yaml"),
    },
    Input {
        name: "small.yaml",
        text: include_str!("../../../tests/conformance/actionlint/testdata/bench/small.yaml"),
    },
    Input {
        name: "large.yaml",
        text: include_str!("../../../tests/conformance/actionlint/testdata/bench/large.yaml"),
    },
    Input {
        name: "many_scripts.yaml",
        text: include_str!(
            "../../../tests/conformance/actionlint/testdata/bench/many_scripts.yaml"
        ),
    },
];

/// Runs the event parser over the whole input.
#[divan::bench(args = INPUTS)]
fn events(input: &Input) -> usize {
    EventParser::new(divan::black_box(input.text)).count()
}

/// Builds the span tree. The source file and its line index are built before
/// the measurement.
#[divan::bench(args = INPUTS)]
fn tree(bencher: Bencher<'_, '_>, input: &Input) {
    let file = SourceFile::new(FileId::new(0), input.name, input.text)
        .expect("benchmark inputs are far below the size limit");
    bencher.bench_local(|| parse(divan::black_box(&file)));
}
