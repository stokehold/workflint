//! The inputs of `benches/parse.rs` parse without errors, so the benchmarks
//! measure the whole parse and not an early exit at a syntax error.

#![allow(clippy::unwrap_used)]

use workflint_diagnostics::{FileId, SourceFile};
use workflint_yaml::event::EventParser;
use workflint_yaml::parse;

const INPUTS: [(&str, &str); 4] = [
    (
        "minimal.yaml",
        include_str!("../../../tests/conformance/actionlint/testdata/bench/minimal.yaml"),
    ),
    (
        "small.yaml",
        include_str!("../../../tests/conformance/actionlint/testdata/bench/small.yaml"),
    ),
    (
        "large.yaml",
        include_str!("../../../tests/conformance/actionlint/testdata/bench/large.yaml"),
    ),
    (
        "many_scripts.yaml",
        include_str!("../../../tests/conformance/actionlint/testdata/bench/many_scripts.yaml"),
    ),
];

#[test]
fn bench_inputs_produce_events_without_errors() {
    for (name, text) in INPUTS {
        let errors: Vec<_> = EventParser::new(text).filter_map(Result::err).collect();
        assert!(errors.is_empty(), "{name}: {errors:?}");
    }
}

#[test]
fn bench_inputs_parse_to_a_tree_without_diagnostics() {
    for (name, text) in INPUTS {
        let file = SourceFile::new(FileId::new(0), name, text).unwrap();
        let parsed = parse(&file);
        assert!(
            parsed.diagnostics.is_empty(),
            "{name}: {:?}",
            parsed.diagnostics
        );
        assert!(parsed.document.is_some(), "{name}");
    }
}
