//! Tests for the `json` and `jsonl` renderers. Each output is also checked against the schema in
//! `schemas/json-output.schema.json`.

#![allow(clippy::unwrap_used)]

use insta::assert_snapshot;
use workflint_diagnostics::{
    Category, Diagnostic, FileId, JsonRecord, RuleMeta, Severity, SourceFile, Span, json_records,
    render_json, render_jsonl,
};

static UNDEFINED_PROPERTY: RuleMeta = RuleMeta {
    id: "undefined-property",
    description: "",
    kind: "expression",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://example.com",
};

static UNUSED_ANCHOR: RuleMeta = RuleMeta {
    id: "unused-anchor",
    description: "",
    kind: "syntax-check",
    category: Category::Correctness,
    default_severity: Severity::Warning,
    docs_url: "https://example.com",
};

/// A workflow with non-ASCII text before both spans, so that columns and byte offsets differ.
const WORKFLOW: &str = "\
name: ビルド
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    env:
      Ä: &ä 1
    steps:
      - run: echo ${{ gihub.sha }} \"ü\"
";

fn file() -> SourceFile {
    SourceFile::new(FileId::new(0), "ワーク/ci.yaml", WORKFLOW).unwrap()
}

/// A diagnostic of `rule` whose span covers the first occurrence of `token`.
fn at(file: &SourceFile, rule: &'static RuleMeta, token: &str, message: &str) -> Diagnostic {
    let start = u32::try_from(file.text().find(token).unwrap()).unwrap();
    let end = start + u32::try_from(token.len()).unwrap();
    Diagnostic::new(rule, Span::new(file.id(), start, end), message)
}

/// Two diagnostics: an unused anchor without help, and an undefined variable with help.
fn records() -> Vec<JsonRecord> {
    let file = file();
    let diagnostics = [
        at(
            &file,
            &UNUSED_ANCHOR,
            "&ä",
            "anchor \"ä\" is defined but not used",
        ),
        at(
            &file,
            &UNDEFINED_PROPERTY,
            "gihub",
            "undefined variable \"gihub\". available variables are \"env\", \"github\"",
        )
        .with_help(Some("did you mean \"github\"?".to_owned())),
    ];
    json_records(&file, &diagnostics)
}

fn json(records: &[JsonRecord]) -> String {
    let mut out = Vec::new();
    render_json(&mut out, records).unwrap();
    String::from_utf8(out).unwrap()
}

fn jsonl(records: &[JsonRecord]) -> String {
    let mut out = Vec::new();
    render_jsonl(&mut out, records).unwrap();
    String::from_utf8(out).unwrap()
}

/// The schema file, compiled at the definition at `pointer`: the empty pointer for the whole
/// document, or `/$defs/line` for a line of `jsonl`.
fn schema(pointer: &str) -> (boon::Schemas, boon::SchemaIndex) {
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../../../schemas/json-output.schema.json")).unwrap();
    let mut compiler = boon::Compiler::new();
    compiler
        .add_resource("json-output.schema.json", schema)
        .unwrap();
    let mut schemas = boon::Schemas::new();
    let index = compiler
        .compile(&format!("json-output.schema.json#{pointer}"), &mut schemas)
        .unwrap();
    (schemas, index)
}

/// Validates `instance` against the definition at `pointer` in the schema file.
fn validate(instance: &str, pointer: &str) {
    let (schemas, index) = schema(pointer);
    let instance: serde_json::Value = serde_json::from_str(instance).unwrap();
    let result = schemas.validate(&instance, index);
    assert!(
        result.is_ok(),
        "{instance} does not match the schema: {:#}",
        result.unwrap_err()
    );
}

#[test]
fn json_prints_one_document_with_the_version_and_every_diagnostic() {
    let output = json(&records());
    assert_snapshot!(output);
    validate(&output, "");
}

#[test]
fn jsonl_prints_one_diagnostic_with_the_version_per_line() {
    let output = jsonl(&records());
    assert_snapshot!(output);
    assert_eq!(output.lines().count(), 2);
    for line in output.lines() {
        validate(line, "/$defs/line");
    }
}

#[test]
fn json_of_zero_diagnostics_has_an_empty_list() {
    let output = json(&[]);
    assert_eq!(output, "{\"version\":1,\"diagnostics\":[]}\n");
    validate(&output, "");
}

#[test]
fn jsonl_of_zero_diagnostics_prints_nothing() {
    assert_eq!(jsonl(&[]), "");
}

#[test]
fn columns_count_code_points_and_offsets_count_bytes() {
    let records = records();
    let anchor = &records[0];
    // `      Ä: ` is 9 characters and 10 bytes, and the six lines before it are 75 bytes.
    assert_eq!(
        (anchor.start.line, anchor.start.column, anchor.start.offset),
        (7, 10, 85)
    );
    // `&ä` is 2 characters and 3 bytes.
    assert_eq!(
        (anchor.end.line, anchor.end.column, anchor.end.offset),
        (7, 12, 88)
    );
}

#[test]
fn an_empty_span_ends_where_it_starts() {
    let file = file();
    let diagnostic = Diagnostic::new(&UNUSED_ANCHOR, Span::new(file.id(), 0, 0), "empty");
    let records = json_records(&file, &[diagnostic]);
    assert_eq!(records[0].start, records[0].end);
    validate(&json(&records), "");
}

#[test]
fn the_schema_rejects_another_version_and_a_missing_key() {
    let (schemas, index) = schema("");
    let instance = serde_json::json!({ "version": 2, "diagnostics": [] });
    assert!(schemas.validate(&instance, index).is_err());
    let (schemas, index) = schema("/$defs/line");
    let mut line: serde_json::Value =
        serde_json::from_str(jsonl(&records()).lines().next().unwrap()).unwrap();
    line.as_object_mut().unwrap().remove("version");
    assert!(schemas.validate(&line, index).is_err());
}
