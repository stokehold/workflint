//! Micro-benchmarks for lexing, parsing, and type-checking the reference's 1,858 real-world
//! expressions (ADR 0019), as the reference's `BenchmarkExprRealWorld` does, and for typing a
//! wide `fromJSON()` literal.
//!
//! The input is `tests/conformance/actionlint/testdata/bench/expressions.txt`, actionlint's
//! file under its MIT license in `tests/conformance/actionlint/LICENSE.txt`. It is outside this
//! crate, so the published package does not contain it. Each benchmark of these expressions
//! handles every expression once per iteration, so the time per expression is the reported
//! time divided by 1,858.
//!
//! The checks run in a new [`TypeEnv`], which allows no context and no special function, as the
//! reference's benchmarks run its checker without a workflow key.

#![allow(clippy::expect_used)]

use divan::Bencher;
use divan::counter::ItemsCount;
use workflint_data::UNTRUSTED_INPUTS;
use workflint_expr::{Expr, Lexer, TokenKind, TypeEnv, parse_expression};

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

/// Each expression, parsed before the measurement.
fn trees() -> Vec<Expr> {
    inputs()
        .iter()
        .filter_map(|src| parse_expression(src).ok())
        .map(|(expr, _)| expr)
        .collect()
}

/// An environment that also reports untrusted input, as for an expression in a script.
fn untrusted_env() -> TypeEnv {
    let mut env = TypeEnv::new();
    env.set_untrusted_inputs(UNTRUSTED_INPUTS);
    env
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

/// Lexes, parses, and type-checks each expression against the global contexts.
#[divan::bench]
fn parse_and_check(bencher: Bencher<'_, '_>) {
    bench_parse_and_check(bencher, &TypeEnv::new());
}

/// Lexes, parses, and type-checks each expression with the check for untrusted input on, as
/// the reference's `LexParseSema` does.
#[divan::bench]
fn parse_and_check_untrusted(bencher: Bencher<'_, '_>) {
    bench_parse_and_check(bencher, &untrusted_env());
}

fn bench_parse_and_check(bencher: Bencher<'_, '_>, env: &TypeEnv) {
    let inputs = inputs();
    bencher
        .counter(ItemsCount::new(inputs.len()))
        .bench_local(|| {
            let mut errors = 0_usize;
            for src in &inputs {
                if let Ok((expr, _)) = parse_expression(divan::black_box(src)) {
                    errors += env.check(&expr).1.len();
                }
            }
            errors
        });
}

/// Type-checks each expression, parsed before the measurement, against the global contexts,
/// as the reference's `Sema-trust` does.
#[divan::bench]
fn check(bencher: Bencher<'_, '_>) {
    bench_check(bencher, &TypeEnv::new());
}

/// Type-checks each expression, parsed before the measurement, with the check for untrusted
/// input on, as the reference's `Sema-untrust` does.
#[divan::bench]
fn check_untrusted(bencher: Bencher<'_, '_>) {
    bench_check(bencher, &untrusted_env());
}

fn bench_check(bencher: Bencher<'_, '_>, env: &TypeEnv) {
    let trees = trees();
    bencher
        .counter(ItemsCount::new(trees.len()))
        .bench_local(|| {
            let mut errors = 0_usize;
            for expr in &trees {
                errors += env.check(divan::black_box(expr)).1.len();
            }
            errors
        });
}

/// Type-checks `fromJSON()` with an array of 32,000 objects whose keys all differ. Typing
/// merges every element into one element type, so this measures that the merge grows linearly.
#[divan::bench]
fn from_json_wide_array(bencher: Bencher<'_, '_>) {
    let elements: Vec<String> = (0..32_000).map(|i| format!(r#"{{"k{i}":1}}"#)).collect();
    let src = format!("fromJSON('[{}]') }}}}", elements.join(","));
    let (expr, _) = parse_expression(&src).expect("the call parses");
    let env = TypeEnv::new();
    bencher.bench_local(|| env.check(divan::black_box(&expr)));
}
