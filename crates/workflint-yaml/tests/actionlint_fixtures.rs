//! The YAML-level diagnostics of the reference's fixtures.
//!
//! The inputs are the reference's fixtures vendored in
//! `tests/conformance/actionlint/testdata/`. Each test lists the lines of the
//! fixture's `.out` file that the YAML layer reports, as
//! `line:column: message [rule-id]`. Lines that the workflow parser or rules
//! report come in later milestones. Where `docs/differences.md` changes a
//! position or message, the test says so.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fmt::Write as _;

use workflint_diagnostics::{FileId, SourceFile, sort_and_dedup};
use workflint_yaml::{Document, NodeKind, parse};

fn source(text: &str) -> SourceFile {
    SourceFile::new(FileId::new(0), "test.yaml", text).unwrap()
}

/// Parses `text` and returns its sorted diagnostics, one per line, and the
/// document.
fn lint(text: &str) -> (String, Option<Document>) {
    let file = source(text);
    let mut parsed = parse(&file);
    sort_and_dedup(&file, &mut parsed.diagnostics);
    let mut out = String::new();
    for diagnostic in &parsed.diagnostics {
        let pos = file.line_col(diagnostic.span.start);
        let _ = writeln!(
            out,
            "{}:{}: {} [{}]",
            pos.line, pos.column, diagnostic.message, diagnostic.rule.id
        );
    }
    (out, parsed.document)
}

// `.out`: `2:7: could not parse as YAML: block sequence entries are not
// allowed in this context`. The position is the problem's
// (docs/differences.md#yaml-error-positions).
#[test]
fn err_yaml_syntax_error() {
    let (diagnostics, document) = lint(include_str!(
        "../../../tests/conformance/actionlint/testdata/err/yaml_syntax_error.yaml"
    ));
    assert_eq!(
        diagnostics,
        "2:8: could not parse as YAML: block sequence entries are not allowed in this context [yaml-syntax-error]\n"
    );
    assert!(document.is_none());
}

// `.out`: `6:16: could not parse as YAML: mapping values are not allowed in
// this context` (docs/differences.md#yaml-error-positions).
#[test]
fn examples_broken_yaml() {
    let (diagnostics, document) = lint(include_str!(
        "../../../tests/conformance/actionlint/testdata/examples/broken_yaml.yaml"
    ));
    assert_eq!(
        diagnostics,
        "6:17: could not parse as YAML: mapping values are not allowed in this context [yaml-syntax-error]\n"
    );
    assert!(document.is_none());
}

// `.out`: `0:0: could not parse as YAML: yaml: unknown anchor 'default_env'
// referenced` (docs/differences.md#unknown-anchor-position). Only the first
// unknown alias is reported.
#[test]
fn err_undefined_anchor() {
    let (diagnostics, document) = lint(include_str!(
        "../../../tests/conformance/actionlint/testdata/err/undefined_anchor.yaml"
    ));
    assert_eq!(
        diagnostics,
        "8:14: could not parse as YAML: unknown anchor 'default_env' referenced [unknown-anchor]\n"
    );
    assert!(document.is_none());
}

// `.out`: `0:0: could not parse as YAML: yaml: unknown anchor 'credentials'
// referenced` (docs/differences.md#unknown-anchor-position).
#[test]
fn examples_dangling_alias() {
    let (diagnostics, document) = lint(include_str!(
        "../../../tests/conformance/actionlint/testdata/examples/dangling_alias.yaml"
    ));
    assert_eq!(
        diagnostics,
        "9:14: could not parse as YAML: unknown anchor 'credentials' referenced [unknown-anchor]\n"
    );
    assert!(document.is_none());
}

#[test]
fn err_unused_anchors() {
    let (diagnostics, document) = lint(include_str!(
        "../../../tests/conformance/actionlint/testdata/err/unused_anchors.yaml"
    ));
    assert_eq!(
        diagnostics,
        concat!(
            "2:5: anchor \"anchor\" is defined but not used [unused-anchor]\n",
            "6:9: anchor \"anchor\" is defined but not used [unused-anchor]\n",
            "12:12: anchor \"anchor\" is defined but not used [unused-anchor]\n",
            "24:14: anchor \"default_env\" is defined but not used [unused-anchor]\n",
            "27:9: anchor \"step\" is defined but not used [unused-anchor]\n",
        )
    );
    assert!(document.is_some());
}

#[test]
fn err_recursive_anchors() {
    let (diagnostics, document) = lint(include_str!(
        "../../../tests/conformance/actionlint/testdata/err/recursive_anchors.yaml"
    ));
    assert_eq!(
        diagnostics,
        concat!(
            "9:14: recursive alias \"recursive1\" is found. anchor was declared at line:7, column:9 [recursive-alias]\n",
            "11:14: recursive alias \"recursive2\" is found. anchor was declared at line:4, column:9 [recursive-alias]\n",
            "15:9: recursive alias \"recursive2\" is found. anchor was declared at line:4, column:9 [recursive-alias]\n",
        )
    );
    // A recursive alias stays in the tree unresolved, and the alias to the
    // completed `&recursive1` on line 13 is resolved.
    let document = document.unwrap();
    let file = source(include_str!(
        "../../../tests/conformance/actionlint/testdata/err/recursive_anchors.yaml"
    ));
    let aliases: Vec<_> = document
        .nodes()
        .filter_map(|(_, node)| match &node.kind {
            NodeKind::Alias(alias) => {
                Some((file.line_col(node.span.start).line, alias.target.is_some()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(aliases, [(9, false), (11, false), (13, true), (15, false)]);
}

#[test]
fn err_issue_610_recursive_raw_yaml_value() {
    let (diagnostics, _) = lint(include_str!(
        "../../../tests/conformance/actionlint/testdata/err/issue-610_recursive_raw_yaml_value.yaml"
    ));
    assert_eq!(
        diagnostics,
        "10:21: recursive alias \"recursive_include\" is found. anchor was declared at line:8, column:18 [recursive-alias]\n"
    );
}

#[test]
fn err_errors_in_anchor() {
    let (diagnostics, _) = lint(include_str!(
        "../../../tests/conformance/actionlint/testdata/err/errors_in_anchor.yaml"
    ));
    assert_eq!(
        diagnostics,
        concat!(
            "20:15: anchor \"valid_inputs\" is defined but not used [unused-anchor]\n",
            "28:15: anchor \"valid_inputs\" is defined but not used [unused-anchor]\n",
        )
    );
}

#[test]
fn examples_yaml_anchor_usage() {
    let (diagnostics, _) = lint(include_str!(
        "../../../tests/conformance/actionlint/testdata/examples/yaml_anchor_usage.yaml"
    ));
    assert_eq!(
        diagnostics,
        concat!(
            "18:14: anchor \"credentials\" is defined but not used [unused-anchor]\n",
            "22:14: recursive alias \"recursive\" is found. anchor was declared at line:19, column:9 [recursive-alias]\n",
        )
    );
}

#[test]
fn ok_anchors() {
    let (diagnostics, document) = lint(include_str!(
        "../../../tests/conformance/actionlint/testdata/ok/anchors.yaml"
    ));
    assert_eq!(diagnostics, "");
    assert!(document.is_some());
}

/// Returns the items of the sequence at `steps` in the job `test`.
fn steps(document: &Document) -> Vec<workflint_yaml::NodeId> {
    let value_of = |id, key: &str| {
        let NodeKind::Mapping(mapping) = &document.node(id)?.kind else {
            return None;
        };
        document
            .entries(mapping)
            .iter()
            .find_map(|entry| match &document.node(entry.key)?.kind {
                NodeKind::Scalar(scalar) if &*scalar.value == key => Some(entry.value),
                _ => None,
            })
    };
    let steps = document
        .root()
        .and_then(|root| value_of(root, "jobs"))
        .and_then(|jobs| value_of(jobs, "test"))
        .and_then(|test| value_of(test, "steps"))
        .unwrap();
    let NodeKind::Sequence(sequence) = &document.node(steps).unwrap().kind else {
        panic!("steps is not a sequence");
    };
    document.items(sequence).to_vec()
}

// The reference reports the error inside `&step` once per alias and then
// removes the duplicates. Here every alias points at the anchored node, so
// the same subtree, with its error, is reachable from all 12 items.
#[test]
fn err_dedup_errors_shares_the_anchored_subtree() {
    let text = include_str!("../../../tests/conformance/actionlint/testdata/err/dedup_errors.yaml");
    let (diagnostics, document) = lint(text);
    assert_eq!(diagnostics, "");
    let document = document.unwrap();

    let items = steps(&document);
    assert_eq!(items.len(), 12);
    assert!(items.iter().all(|&item| item == items[0]));

    // One node per scalar, collection, and alias in the source: the 11
    // aliases add 11 alias nodes and no copies of the step.
    let file = source(text);
    let aliases = document
        .nodes()
        .filter(|(_, node)| matches!(node.kind, NodeKind::Alias(_)))
        .count();
    assert_eq!(aliases, 11);
    assert_eq!(document.nodes().count(), 29);
    let NodeKind::Mapping(step) = &document.node(items[0]).unwrap().kind else {
        panic!("the step is not a mapping");
    };
    let with = document.entries(step)[1];
    assert_eq!(
        file.line_col(document.node(with.key).unwrap().span.start)
            .line,
        9
    );
}

// `with: &valid_inputs` on line 20 is inside `&invalid_step`. Both aliases on
// lines 23 and 25 reach the same node, so an error reported at it would be
// reported once per use at the anchor's position and then deduplicated.
#[test]
fn err_errors_in_anchor_reaches_the_subtree_through_every_alias() {
    let text =
        include_str!("../../../tests/conformance/actionlint/testdata/err/errors_in_anchor.yaml");
    let (_, document) = lint(text);
    let document = document.unwrap();
    let items = steps(&document);
    assert_eq!(items.len(), 4);
    assert_eq!(items[1], items[0]);
    assert_eq!(items[2], items[0]);
    assert_ne!(items[3], items[0]);
}
