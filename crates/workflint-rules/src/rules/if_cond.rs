//! The `if-cond` rule: `if:` conditions that are always true or constant.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `rule_if_cond.go`.

use std::fmt::Write as _;

use workflint_diagnostics::quote::quote;
use workflint_diagnostics::{Category, RuleMeta, Severity};
use workflint_expr::{Expr, ExprKind, LogicalOp, parse_expression};
use workflint_syntax::{Job, KeyedEntry, Placeholder, Step, Template};

use crate::{Context, Rule};

/// An `if:` condition has text around its `${{ }}`, or more than one `${{ }}`, so GitHub reads
/// it as a string, which is always true.
pub(crate) static IF_ALWAYS_TRUE: RuleMeta = RuleMeta {
    id: "if-always-true",
    description: "An `if:` condition has text around its `${{ }}`, so it is always true",
    kind: "if-cond",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/if-always-true.md",
};

/// An `if:` condition is a constant expression, such as `true` or `contains('foo', 'f')`.
pub(crate) static CONSTANT_CONDITION: RuleMeta = RuleMeta {
    id: "constant-condition",
    description: "An `if:` condition has the same value in every run",
    kind: "if-cond",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/constant-condition.md",
};

/// The reference's `RuleIfCond`. It checks `jobs.<job_id>.if`, `jobs.<job_id>.snapshot.if`, and
/// `steps[*].if`.
pub(crate) struct IfCond;

impl<'a> Rule<'a> for IfCond {
    fn job_pre(&mut self, job: &'a KeyedEntry<'a, Job<'a>>, cx: &mut Context<'_>) {
        check(job.value.condition.value(), cx);
        if let Some(snapshot) = job.value.snapshot.value() {
            check(snapshot.condition.value(), cx);
        }
    }

    fn step(&mut self, step: &'a Step<'a>, cx: &mut Context<'_>) {
        check(step.condition.value(), cx);
    }
}

/// Checks one condition, as the reference's `checkIfCond` does, with the placeholders that the
/// parser stored instead of a search for the first `}}`
/// (`docs/differences.md#if-cond-closing-braces`).
fn check(condition: Option<&Template<'_>>, cx: &mut Context<'_>) {
    let Some(condition) = condition else {
        return;
    };
    let value = condition.value();
    let placeholders = condition.placeholders();
    let Some(first) = placeholders.first() else {
        return;
    };
    if first.is_implicit() {
        // The parser reads the whole value as one expression when it has no `${{` before its
        // first `}}`. The reference treats a `${{` after a `}}` as a placeholder with text
        // before it.
        if value.contains("${{") && value.contains("}}") {
            report_always_true(condition, None, cx);
        } else if first.expr.as_ref().is_ok_and(Expr::is_constant) {
            report_constant(condition, value, cx);
        }
        return;
    }
    if first.start > 0 || first_end(value, first) < value.len() || placeholders.len() > 1 {
        let suggestion = suggest(value, placeholders);
        report_always_true(condition, suggestion.as_deref(), cx);
        return;
    }
    if let Ok(expr) = &first.expr
        && expr.is_constant()
    {
        report_constant(condition, inner_text(value, first), cx);
    }
}

/// Returns where the first placeholder ends. After a parse error the placeholder's end is where
/// the lexer stopped, which may be before its `}}`, so the end is the first `}}` after its
/// `${{`, as in the reference.
fn first_end(value: &str, placeholder: &Placeholder) -> usize {
    let range = placeholder.range();
    if placeholder.expr.is_ok() {
        return range.end;
    }
    let offset = placeholder.to_value(0..0).start;
    value
        .get(offset..)
        .and_then(|rest| rest.find("}}"))
        .map_or(value.len(), |found| offset + found + "}}".len())
}

/// Returns the text between the `${{` and `}}` of a parsed placeholder, trimmed.
fn inner_text<'v>(value: &'v str, placeholder: &Placeholder) -> &'v str {
    let range = placeholder.range();
    let start = placeholder.to_value(0..0).start;
    let end = range.end.saturating_sub("}}".len()).max(start);
    value.get(start..end).unwrap_or_default().trim()
}

fn report_always_true(condition: &Template<'_>, suggestion: Option<&str>, cx: &mut Context<'_>) {
    let mut message = format!(
        "if: condition {} is always evaluated to true because extra characters are around ${{{{ }}}}",
        quote(condition.value()),
    );
    if let Some(suggestion) = suggestion {
        let _ = write!(message, ". did you mean {}?", quote(suggestion));
    }
    cx.report_in(condition.source, &IF_ALWAYS_TRUE, condition.span(), message);
}

fn report_constant(condition: &Template<'_>, text: &str, cx: &mut Context<'_>) {
    cx.report_in(
        condition.source,
        &CONSTANT_CONDITION,
        condition.span(),
        format!(
            "constant expression {} in condition. remove the if: section",
            quote(text.trim())
        ),
    );
}

/// Returns the single `${{ }}` that a condition such as `${{ success() }} || ${{ failure() }}`
/// means (rhysd/actionlint#367), or `None` if there is none.
///
/// The suggestion joins the text around the placeholders with the text inside them. It wraps a
/// placeholder in parentheses when an operator next to it binds tighter than the placeholder's
/// own operator. It is offered only when the text around the placeholders is more than
/// whitespace, every placeholder parses, the joined text parses as one expression, and that
/// expression is not constant.
fn suggest(value: &str, placeholders: &[Placeholder]) -> Option<String> {
    let mut joined = String::new();
    let mut outside = String::new();
    let mut pos = 0;
    for (index, placeholder) in placeholders.iter().enumerate() {
        let expr = placeholder.expr.as_ref().ok()?;
        let range = placeholder.range();
        let before = value.get(pos..range.start)?;
        let after = match placeholders.get(index + 1) {
            Some(next) => value.get(range.end..next.range().start)?,
            None => value.get(range.end..)?,
        };
        outside.push_str(before);
        joined.push_str(before);
        let inner = inner_text(value, placeholder);
        let neighbour = operator_precedence(before.trim_end(), |text, op| text.ends_with(op)).max(
            operator_precedence(after.trim_start(), |text, op| text.starts_with(op)),
        );
        if precedence(expr) < neighbour {
            joined.push('(');
            joined.push_str(inner);
            joined.push(')');
        } else {
            joined.push_str(inner);
        }
        pos = range.end;
    }
    let rest = value.get(pos..)?;
    outside.push_str(rest);
    joined.push_str(rest);
    if outside.trim().is_empty() {
        return None;
    }
    let joined = joined.trim();
    let text = format!("{joined}}}}}");
    let (expr, end) = parse_expression(&text).ok()?;
    let complete = usize::try_from(end).is_ok_and(|end| end == text.len());
    (complete && !expr.is_constant()).then(|| format!("${{{{ {joined} }}}}"))
}

/// How tightly the operator at the top of `expr` binds: `||` is 1, `&&` is 2, a comparison is 3,
/// and anything else binds tighter than every operator.
fn precedence(expr: &Expr) -> u8 {
    match &expr.kind {
        ExprKind::Logical {
            op: LogicalOp::Or, ..
        } => 1,
        ExprKind::Logical {
            op: LogicalOp::And, ..
        } => 2,
        ExprKind::Compare { .. } => 3,
        _ => u8::MAX,
    }
}

/// How tightly the operator next to a placeholder binds, given the text on one side of it and a
/// test for the end of that text that faces the placeholder: 0 for no text, 1 for `||`, 2 for
/// `&&`, and the most for anything else, which makes the placeholder take parentheses.
fn operator_precedence(text: &str, faces: fn(&str, &str) -> bool) -> u8 {
    if text.is_empty() {
        0
    } else if faces(text, "||") {
        1
    } else if faces(text, "&&") {
        2
    } else {
        u8::MAX
    }
}
