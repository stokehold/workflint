//! Linting takes time linear in the number of diagnostics inside one scalar and in the number of
//! steps in a job. Each test lints a workflow with 8000 of one thing and must finish in under one
//! second in a release build, which CI checks. A debug build is about ten times slower.
//!
//! Mapping the span of each diagnostic decoded the whole scalar again, so 8000 diagnostics in
//! one `run` took minutes. Adding each step with an `id` copied the steps before it, so 8000
//! steps took 1.85 seconds in a release build.

#![allow(clippy::unwrap_used)]

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use workflint_diagnostics::{Diagnostic, FileId, SourceFile, sort_and_dedup};
use workflint_rules::local::NoProject;

const N: usize = 8000;

/// Lints `workflow` as the binary does, without the file system, checks that it finishes within
/// the budget, and returns the sorted diagnostics.
fn lint_in_budget(workflow: &str) -> Vec<Diagnostic> {
    let file = SourceFile::new(FileId::new(0), "test.yaml", workflow).unwrap();

    let start = Instant::now();
    let parsed = workflint_yaml::parse(&file);
    let mut diagnostics = parsed.diagnostics;
    let document = parsed.document.unwrap();
    let syntax = workflint_syntax::parse(&file, &document);
    diagnostics.extend(syntax.diagnostics);
    diagnostics.extend(workflint_rules::run_in_project(
        &file,
        &syntax.workflow,
        &NoProject,
    ));
    sort_and_dedup(&file, &mut diagnostics);
    let elapsed = start.elapsed();

    let limit = if cfg!(debug_assertions) {
        Duration::from_secs(10)
    } else {
        Duration::from_secs(1)
    };
    assert!(elapsed < limit, "took {elapsed:?}");
    diagnostics
}

/// Returns a workflow with one job whose steps are `steps`, each a YAML block indented for a
/// sequence entry.
fn job(steps: &str) -> String {
    format!("on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n{steps}")
}

/// Returns how many of `diagnostics` the rule `id` reports.
fn count(diagnostics: &[Diagnostic], id: &str) -> usize {
    diagnostics.iter().filter(|d| d.rule.id == id).count()
}

#[test]
fn untrusted_inputs_on_many_lines_of_one_block_scalar() {
    let mut steps = String::from("      - run: |\n");
    for _ in 0..N {
        steps.push_str("          echo ${{ github.event.issue.title }}\n");
    }
    let diagnostics = lint_in_budget(&job(&steps));
    assert_eq!(count(&diagnostics, "untrusted-input"), N);
}

#[test]
fn undefined_variables_in_one_plain_scalar() {
    let steps = format!("      - run: echo {}\n", "${{ foo }} ".repeat(N));
    let diagnostics = lint_in_budget(&job(&steps));
    assert_eq!(count(&diagnostics, "undefined-variable"), N);
}

#[test]
fn objects_in_one_template() {
    let steps = format!("      - run: echo {}\n", "${{ github }} ".repeat(N));
    let diagnostics = lint_in_budget(&job(&steps));
    assert_eq!(count(&diagnostics, "object-in-template"), N);
}

#[test]
fn parse_errors_in_one_scalar() {
    let steps = format!("      - run: echo {}\n", "${{ ) }} ".repeat(N));
    let diagnostics = lint_in_budget(&job(&steps));
    assert_eq!(diagnostics.len(), N);
}

#[test]
fn stray_closing_braces_in_one_scalar() {
    let steps = format!("      - run: echo {}\n", "}} ".repeat(N));
    let diagnostics = lint_in_budget(&job(&steps));
    assert_eq!(count(&diagnostics, "unmatched-closing-braces"), N);
}

#[test]
fn many_opening_braces_in_one_scalar() {
    let steps = format!("      - run: echo {}\n", "${{ ".repeat(N));
    lint_in_budget(&job(&steps));
}

#[test]
fn glob_problems_in_one_pattern() {
    let workflow = format!(
        "on:\n  push:\n    branches: ['{}']\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo\n",
        "a~".repeat(N)
    );
    let diagnostics = lint_in_budget(&workflow);
    assert_eq!(count(&diagnostics, "invalid-ref-name"), N);
}

#[test]
fn steps_with_an_id_each() {
    let mut steps = String::new();
    for i in 0..N {
        let _ = writeln!(steps, "      - id: s{i}\n        run: echo");
    }
    let _ = writeln!(steps, "      - run: echo ${{{{ steps.s0.outcome }}}}");
    let diagnostics = lint_in_budget(&job(&steps));
    assert_eq!(diagnostics, []);
}
