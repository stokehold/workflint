//! The checks that the `expr_lex`, `expr_parse`, and `expr_check` fuzz targets in `fuzz/` run on
//! each input.
//!
//! The fuzz targets include this file with `#[path]`, and `real_world.rs` and
//! `fuzz_regressions.rs` run it on expressions, so a crash input found by the fuzzer fails the
//! same check as a test. Each check panics when an invariant does not hold. The input is the
//! text after `${{`.

#![allow(clippy::panic, clippy::unwrap_used)]

use std::ops::Range;

use workflint_expr::rules::{INVALID_EXPRESSION_SYNTAX, INVALID_NUMBER_LITERAL};
use workflint_expr::{Expr, ExprError, ExprKind, Lexer, TokenKind, Visit, lex_expression};
use workflint_expr::{Token, TypeEnv, parse_expression};

/// Lexes `src` one token at a time up to the end of the expression or the first error.
///
/// The lexer does not panic and stops: each token other than the last is non-empty, so it
/// returns at most one token per byte before the end. Every token's range lies inside `src` on
/// character boundaries, and the tokens never overlap or move backwards. Without an error, the
/// last token is the `}}` that ends the expression, with the byte order mark if the input starts
/// with `}}` after one, and the lexer's offset is just past it.
/// The error, if any, is valid as [`check_error`] says. [`lex_expression`] returns the same
/// tokens and the same error.
pub(crate) fn check_lex(src: &str) {
    let mut lexer = Lexer::new(src);
    let mut tokens = Vec::new();
    loop {
        let token = lexer.next_token();
        let what = format!("{token:?} in {src:?}");
        check_range(src, &token.range(), &what);
        if let Some(previous) = tokens.last().map(Token::range) {
            assert!(token.range().start >= previous.end, "{what}");
        }
        tokens.push(token);
        if token.kind == TokenKind::End {
            break;
        }
        assert!(!token.range().is_empty(), "{what}");
        assert!(
            tokens.len() <= src.len(),
            "the lexer does not stop on {src:?}"
        );
    }
    let error = lexer.error().cloned();
    if let Some(error) = &error {
        check_error(src, error);
        assert!(
            src[error.range()].chars().count() <= 1,
            "{error:?} in {src:?}"
        );
    } else {
        // A byte order mark at the start of the input stays in the first token.
        let end = tokens.last().map(Token::range).unwrap();
        assert!(
            matches!(src.get(end.clone()), Some("}}" | "\u{feff}}}")),
            "{end:?} in {src:?}"
        );
        assert_eq!(usize::try_from(lexer.offset()).unwrap(), end.end, "{src:?}");
    }
    match (lex_expression(src), error) {
        (Ok((collected, end)), None) => {
            assert_eq!(collected, tokens, "{src:?}");
            assert_eq!(end, lexer.offset(), "{src:?}");
        }
        (Err(collected), Some(error)) => assert_eq!(collected, error, "{src:?}"),
        (collected, error) => {
            panic!("lex_expression gives {collected:?}, the lexer {error:?}, for {src:?}")
        }
    }
}

/// Parses `src` up to the end of the expression or the first error.
///
/// The parser does not panic. In the tree, every node's range lies inside `src` on character
/// boundaries and inside its parent's range, and so do the node's token and the ranges of its
/// names. [`Expr::walk`] enters and leaves each node once, in nested order. The offset that the
/// parser returns is just past the `}}` that ends the expression, and the lexer reads the same
/// expression without an error. The error, if any, is valid as [`check_error`] says, and
/// covers at most one character or one token of the lexer.
pub(crate) fn check_parse(src: &str) {
    let (expr, end) = match parse_expression(src) {
        Ok(parsed) => parsed,
        Err(error) => {
            check_error(src, &error);
            let range = error.range();
            assert!(
                src[range.clone()].chars().count() <= 1 || token_ranges(src).contains(&range),
                "{error:?} in {src:?}"
            );
            return;
        }
    };
    check_node(src, &expr, &(0..src.len()));
    check_walk(src, &expr);
    let end = usize::try_from(end).unwrap();
    assert!(
        src.get(..end).is_some_and(|text| text.ends_with("}}")),
        "{end} in {src:?}"
    );
    match lex_expression(src) {
        Ok((_, lexed)) => assert_eq!(usize::try_from(lexed).unwrap(), end, "{src:?}"),
        Err(error) => panic!("the parser accepts {src:?}, but the lexer reports {error:?}"),
    }
}

/// Parses `src` and, if that succeeds, type-checks the tree against the global contexts.
///
/// The checker does not panic, and a second check returns the same type and errors. Every
/// error has a message and a rule other than the two syntax rules, and its range lies inside
/// the tree's range on character boundaries. [`Expr::is_constant`] does not panic.
pub(crate) fn check_types(src: &str) {
    let Ok((expr, _)) = parse_expression(src) else {
        return;
    };
    let env = TypeEnv::new();
    let (ty, errors) = env.check(&expr);
    for error in &errors {
        let what = format!("{error:?} in {src:?}");
        assert!(!error.message.is_empty(), "{what}");
        assert!(
            ![INVALID_EXPRESSION_SYNTAX.id, INVALID_NUMBER_LITERAL.id].contains(&error.rule.id),
            "{what}"
        );
        check_range(src, &error.range(), &what);
        check_inside(&error.range(), &expr.range(), &what);
    }
    assert_eq!(env.check(&expr), (ty, errors), "{src:?}");
    let _ = expr.is_constant();
}

/// An error has a message and one of the two rules of the lexer and the parser. Its range lies
/// inside `src` on character boundaries.
fn check_error(src: &str, error: &ExprError) {
    let what = format!("{error:?} in {src:?}");
    assert!(!error.message.is_empty(), "{what}");
    assert!(
        [INVALID_EXPRESSION_SYNTAX.id, INVALID_NUMBER_LITERAL.id].contains(&error.rule.id),
        "{what}"
    );
    check_range(src, &error.range(), &what);
}

/// The ranges of the tokens that the lexer returns for `src`, up to the end of the expression
/// or the first error.
fn token_ranges(src: &str) -> Vec<Range<usize>> {
    let mut lexer = Lexer::new(src);
    let mut ranges = Vec::new();
    loop {
        let token = lexer.next_token();
        ranges.push(token.range());
        if token.kind == TokenKind::End || ranges.len() > src.len() {
            return ranges;
        }
    }
}

/// Checks `expr` and its children, recursively. `parent` is the range of the node that
/// contains `expr`, or all of `src` for the root.
fn check_node(src: &str, expr: &Expr, parent: &Range<usize>) {
    let range = expr.range();
    let what = format!("{expr:?} in {src:?}");
    check_range(src, &range, &what);
    check_inside(&range, parent, &what);
    check_inside(&expr.token.range(), &range, &what);
    let mut names = Vec::new();
    let mut children = Vec::new();
    match &expr.kind {
        ExprKind::Variable(name) => names.push(name),
        ExprKind::Null
        | ExprKind::Bool(_)
        | ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::String(_) => {}
        ExprKind::Property { receiver, name } => {
            children.push(receiver.as_ref());
            names.push(name);
        }
        ExprKind::ObjectFilter { receiver } => children.push(receiver),
        ExprKind::Index { operand, index } => children.extend([operand.as_ref(), index]),
        ExprKind::Not { operand } => children.push(operand),
        ExprKind::Compare { left, right, .. } => children.extend([left.as_ref(), right]),
        ExprKind::Logical { operands, .. } => {
            assert!(operands.len() >= 2, "{what}");
            children.extend(operands);
        }
        ExprKind::Call { callee, args } => {
            names.push(callee);
            children.extend(args);
        }
    }
    for name in names {
        check_inside(&name.range(), &range, &what);
        assert_eq!(src.get(name.range()), Some(name.original()), "{what}");
    }
    for child in children {
        check_node(src, child, &range);
    }
}

/// [`Expr::walk`] enters each node before it leaves it, with the node it entered last and has
/// not left as the parent, and leaves every node it enters.
fn check_walk(src: &str, expr: &Expr) {
    let mut stack: Vec<&Expr> = Vec::new();
    let mut count = 0_usize;
    expr.walk(&mut |node, parent, visit| match visit {
        Visit::Enter => {
            assert!(
                same_node(parent, stack.last().copied()),
                "{node:?} in {src:?}"
            );
            stack.push(node);
            count += 1;
        }
        Visit::Leave => {
            let entered = stack.pop();
            assert!(same_node(Some(node), entered), "{node:?} in {src:?}");
            assert!(
                same_node(parent, stack.last().copied()),
                "{node:?} in {src:?}"
            );
        }
    });
    assert!(stack.is_empty(), "{src:?}");
    assert!(count >= 1, "{src:?}");
}

fn same_node(a: Option<&Expr>, b: Option<&Expr>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => std::ptr::eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

fn check_range(src: &str, range: &Range<usize>, what: &str) {
    assert!(range.start <= range.end, "{range:?}: {what}");
    assert!(src.get(range.clone()).is_some(), "{range:?}: {what}");
}

fn check_inside(inner: &Range<usize>, outer: &Range<usize>, what: &str) {
    assert!(
        outer.start <= inner.start && inner.end <= outer.end,
        "{inner:?} is outside {outer:?}: {what}"
    );
}
