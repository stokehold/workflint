//! Lexes, parses, and type-checks the reference's real-world expressions, and runs the checks of
//! the fuzz targets on them and on every expression in the reference's fixtures.
//!
//! `testdata/bench/expressions.txt` holds 1,858 expressions from public workflows, one per line,
//! without `${{` and `}}`. The reference's `TestExprSemanticsCheckRealWorld` parses each of them
//! without an error and type-checks it against the global contexts. The benchmarks in
//! `benches/expressions.rs` measure the same file.

#![allow(clippy::expect_used, clippy::panic)]

mod invariants;

use std::path::{Path, PathBuf};

use workflint_expr::{TypeEnv, lex_expression, parse_expression};

const EXPRESSIONS: &str =
    include_str!("../../../tests/conformance/actionlint/testdata/bench/expressions.txt");

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/conformance/actionlint/testdata")
}

fn yaml_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            yaml_files(&path, out);
        } else if path
            .extension()
            .is_some_and(|ext| ext == "yaml" || ext == "yml")
        {
            out.push(path);
        }
    }
}

/// The text after each `${{` in `text`, up to the end of the line that has the next `}}`, or of
/// the same line if no `}}` follows. `cargo xtask fuzz-seeds` cuts the seed inputs of the expression fuzz targets the same way.
fn expressions_in(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("${{") {
        let after = &rest[start + 3..];
        let close = after.find("}}").unwrap_or(0);
        let end = after[close..]
            .find('\n')
            .map_or(after.len(), |newline| close + newline + 1);
        found.push(&after[..end]);
        rest = after;
    }
    found
}

#[test]
fn every_real_world_expression_lexes_and_parses_without_an_error() {
    let mut count = 0;
    for line in EXPRESSIONS.lines() {
        let src = format!("{line}}}}}");
        if let Err(err) = lex_expression(&src) {
            panic!("lexing {line:?} failed at {}: {err}", err.start);
        }
        match parse_expression(&src) {
            Ok((_, end)) => assert_eq!(end as usize, src.len(), "{line:?}"),
            Err(err) => panic!("parsing {line:?} failed at {}: {err}", err.start),
        }
        count += 1;
    }
    assert_eq!(count, 1858);
}

#[test]
fn every_real_world_expression_parses_and_type_checks() {
    let env = TypeEnv::new();
    for line in EXPRESSIONS.lines() {
        let src = format!("{line}}}}}");
        match parse_expression(&src) {
            Ok((expr, _)) => {
                let _ = env.check(&expr);
            }
            Err(err) => panic!("parsing {line:?} failed at {}: {err}", err.start),
        }
    }
}

#[test]
fn every_real_world_expression_passes_the_fuzz_checks() {
    for line in EXPRESSIONS.lines() {
        let src = format!("{line}}}}}");
        invariants::check_lex(&src);
        invariants::check_parse(&src);
        invariants::check_types(&src);
    }
}

#[test]
fn every_expression_in_the_reference_fixtures_passes_the_fuzz_checks() {
    let mut files = Vec::new();
    yaml_files(&fixtures(), &mut files);
    let mut count = 0;
    for path in &files {
        let bytes = std::fs::read(path).expect("fixtures are readable");
        let text = String::from_utf8_lossy(&bytes);
        for src in expressions_in(&text) {
            // The check's messages do not name the file, so print it first. The test runner
            // shows the output of a failed test.
            println!("{}", path.display());
            invariants::check_lex(src);
            invariants::check_parse(src);
            invariants::check_types(src);
            count += 1;
        }
    }
    assert!(count >= 600, "found only {count} expressions");
}

#[test]
fn an_expression_ends_at_the_line_with_the_next_closing_braces() {
    let text = "a: ${{ x }} b\nc: ${{ 'y\n' }} d\ne: ${{ z";
    assert_eq!(expressions_in(text), [" x }} b\n", " 'y\n' }} d\n", " z"]);
}
