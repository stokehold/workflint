//! Micro-benchmarks for lexing and parsing the reference's 1,858 real-world expressions
//! (ADR 0019), as the reference's `BenchmarkExprRealWorld` does.
//!
//! The input is `tests/conformance/actionlint/testdata/bench/expressions.txt`, actionlint's
//! file under its MIT license in `tests/conformance/actionlint/LICENSE.txt`. It is outside this
//! crate, so the published package does not contain it. Each benchmark handles every
//! expression once per iteration, so the time per expression is the reported time divided by
//! 1,858.

use divan::Bencher;
use divan::counter::ItemsCount;
use workflint_expr::{Lexer, TokenKind, parse_expression};

fn main() {
    divan::main();
}

const EXPRESSIONS: &str =
    include_str!("../../../tests/conformance/actionlint/testdata/bench/expressions.txt");

/// Each expression followed by the `}}` that ends it, as the lexer expects.
fn inputs() -> Vec<String> {
    EXPRESSIONS
        .lines()
        .map(|line| format!("{line}}}}}"))
        .collect()
}

/// Lexes each expression one token at a time up to its `}}`.
#[divan::bench]
fn lex(bencher: Bencher<'_, '_>) {
    let inputs = inputs();
    bencher
        .counter(ItemsCount::new(inputs.len()))
        .bench_local(|| {
            let mut tokens = 0_usize;
            for src in &inputs {
                let mut lexer = Lexer::new(divan::black_box(src));
                while lexer.next_token().kind != TokenKind::End {
                    tokens += 1;
                }
            }
            tokens
        });
}

/// Lexes and parses each expression into its syntax tree.
#[divan::bench]
fn parse(bencher: Bencher<'_, '_>) {
    let inputs = inputs();
    bencher
        .counter(ItemsCount::new(inputs.len()))
        .bench_local(|| {
            let mut parsed = 0_usize;
            for src in &inputs {
                if parse_expression(divan::black_box(src)).is_ok() {
                    parsed += 1;
                }
            }
            parsed
        });
}
