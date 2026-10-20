//! Micro-benchmarks for the rule pass over one workflow (ADR 0019).
//!
//! The inputs are the reference's benchmark workflows in
//! `tests/conformance/actionlint/testdata/bench/` and the largest file of the
//! corpus, `mozilla_DeepSpeech_.github_workflows_build-and-test.yml`, which
//! the benchmark reads from the corpus archive `dataset.zip`. Both sit outside
//! this crate, so the published package does not contain them.

#![allow(clippy::expect_used)]

use std::fmt;
use std::io::{Cursor, Read};

use divan::Bencher;
use workflint_diagnostics::{FileId, SourceFile};
use zip::ZipArchive;

fn main() {
    divan::main();
}

const ARCHIVE: &[u8] =
    include_bytes!("../../../tests/conformance/actionlint/testdata/realworld/dataset.zip");

/// The largest file of the corpus, at 116,299 bytes.
const LARGEST_CORPUS_FILE: &str = "mozilla_DeepSpeech_.github_workflows_build-and-test.yml";

/// One benchmark input. `Display` gives the name divan shows for it.
enum Input {
    Bench(&'static str, &'static str),
    Corpus(&'static str),
}

impl Input {
    fn name(&self) -> &'static str {
        match self {
            Self::Bench(name, _) | Self::Corpus(name) => name,
        }
    }

    fn text(&self) -> String {
        match self {
            Self::Bench(_, text) => (*text).to_owned(),
            Self::Corpus(name) => {
                let mut archive =
                    ZipArchive::new(Cursor::new(ARCHIVE)).expect("the corpus archive is a zip");
                let mut entry = archive
                    .by_name(name)
                    .expect("the corpus archive has the file");
                let mut text = String::new();
                entry
                    .read_to_string(&mut text)
                    .expect("the corpus file is UTF-8");
                text
            }
        }
    }

    fn file(&self) -> SourceFile {
        SourceFile::new(FileId::new(0), self.name(), self.text())
            .expect("benchmark inputs are far below the size limit")
    }
}

impl fmt::Display for Input {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

const INPUTS: &[Input] = &[
    Input::Bench(
        "minimal.yaml",
        include_str!("../../../tests/conformance/actionlint/testdata/bench/minimal.yaml"),
    ),
    Input::Bench(
        "small.yaml",
        include_str!("../../../tests/conformance/actionlint/testdata/bench/small.yaml"),
    ),
    Input::Bench(
        "large.yaml",
        include_str!("../../../tests/conformance/actionlint/testdata/bench/large.yaml"),
    ),
    Input::Bench(
        "many_scripts.yaml",
        include_str!("../../../tests/conformance/actionlint/testdata/bench/many_scripts.yaml"),
    ),
    Input::Corpus(LARGEST_CORPUS_FILE),
];

/// Runs every rule on a workflow. The span tree and the workflow are built
/// before the measurement.
#[divan::bench(args = INPUTS)]
fn rules(bencher: Bencher<'_, '_>, input: &Input) {
    let file = input.file();
    let document = workflint_yaml::parse(&file)
        .document
        .expect("benchmark inputs are valid YAML");
    let workflow = workflint_syntax::parse(&file, &document).workflow;
    bencher.bench_local(|| workflint_rules::run(&file, divan::black_box(&workflow)));
}

/// Builds the span tree, parses it into a workflow, and runs every rule, as
/// the binary does for each file.
#[divan::bench(args = INPUTS)]
fn lint(bencher: Bencher<'_, '_>, input: &Input) {
    let file = input.file();
    bencher.bench_local(|| {
        let tree = workflint_yaml::parse(divan::black_box(&file));
        tree.document.map(|document| {
            let syntax = workflint_syntax::parse(&file, &document);
            syntax.diagnostics.len() + workflint_rules::run(&file, &syntax.workflow).len()
        })
    });
}
