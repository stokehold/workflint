//! The `matrix` rule: duplicate values in a matrix row, and `exclude` entries that match no row.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `rule_matrix.go`. `exclude` is matched
//! against the rows only, because GitHub applies `include` after `exclude`
//! (`docs/differences.md#matrix-exclude-before-include`).

mod value;

use workflint_diagnostics::quote::{quote, sorted_quotes};
use workflint_diagnostics::{Category, RuleMeta, Severity, Span};
use workflint_syntax::{Job, KeyedEntry, MatrixCombination, MatrixRow, RawValue, Str, Value};

use crate::{Context, Rule};

/// A matrix row has the same value twice.
pub(crate) static DUPLICATE_MATRIX_VALUE: RuleMeta = RuleMeta {
    id: "duplicate-matrix-value",
    description: "A matrix row has the same value twice",
    kind: "matrix",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/duplicate-matrix-value.md",
};

/// `exclude` is written in a matrix that has no rows.
pub(crate) static EXCLUDE_WITHOUT_MATRIX: RuleMeta = RuleMeta {
    id: "exclude-without-matrix",
    description: "`exclude` is written in a matrix that has no rows",
    kind: "matrix",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/exclude-without-matrix.md",
};

/// An `exclude` entry names a key that is not a matrix row.
pub(crate) static UNKNOWN_EXCLUDE_KEY: RuleMeta = RuleMeta {
    id: "unknown-exclude-key",
    description: "An `exclude` entry names a key that is not a matrix row",
    kind: "matrix",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/unknown-exclude-key.md",
};

/// An `exclude` value matches no value of its matrix row.
pub(crate) static UNMATCHED_EXCLUDE_VALUE: RuleMeta = RuleMeta {
    id: "unmatched-exclude-value",
    description: "An `exclude` value matches no value of its matrix row",
    kind: "matrix",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/unmatched-exclude-value.md",
};

/// The reference's `RuleMatrix`. It checks the matrix of each job.
pub(crate) struct Matrix;

impl<'a> Rule<'a> for Matrix {
    fn job_pre(&mut self, job: &'a KeyedEntry<'a, Job<'a>>, cx: &mut Context<'_>) {
        let Some(strategy) = job.value.strategy.value() else {
            return;
        };
        let Value::Literal {
            key, span, value, ..
        } = &strategy.matrix
        else {
            return;
        };
        for row in &value.rows {
            if let MatrixRow::Values(values) = &row.value {
                check_duplicates(row.key.value(), values, cx);
            }
        }
        // The reference's `Matrix.Pos` is the `matrix` key.
        let position = key.as_ref().map_or(*span, Str::span);
        check_exclude(value, position, cx);
    }
}

/// Reports each value of the row `name` that equals an earlier value.
fn check_duplicates(name: &str, values: &[RawValue<'_>], cx: &mut Context<'_>) {
    let mut seen: Vec<&RawValue<'_>> = Vec::with_capacity(values.len());
    for value in values {
        if let Some(first) = seen.iter().find(|first| value::equals(first, value)) {
            let at = cx.file().line_col(first.span().start);
            cx.report(
                &DUPLICATE_MATRIX_VALUE,
                value.span(),
                format!(
                    "duplicate value {} is found in matrix {}. the same value is at {at}",
                    value::display(value),
                    quote(name),
                ),
            );
        } else {
            seen.push(value);
        }
    }
}

/// Reports each key and value of `exclude` that no row has. `position` is where the matrix is.
///
/// The reference adds the values of `include` to the rows first, and skips the check when
/// `include` has an expression. GitHub applies `include` after `exclude`, so the rows alone are
/// what `exclude` can remove.
fn check_exclude(matrix: &workflint_syntax::Matrix<'_>, position: Span, cx: &mut Context<'_>) {
    let Some(exclude) = matrix.exclude.literal() else {
        return;
    };
    if exclude.is_empty() {
        return;
    }
    if matrix.rows.is_empty() {
        cx.report(
            &EXCLUDE_WITHOUT_MATRIX,
            position,
            "\"exclude\" section exists but no matrix variation exists",
        );
        return;
    }
    for combination in exclude.iter() {
        let MatrixCombination::Assigns { assigns, .. } = combination else {
            continue;
        };
        for assign in assigns {
            let name = assign.folded();
            let Some(row) = matrix.rows.get(name) else {
                cx.report(
                    &UNKNOWN_EXCLUDE_KEY,
                    assign.key.span(),
                    format!(
                        "{} in \"exclude\" section does not exist in matrix. available matrix configurations are {}",
                        quote(name),
                        sorted_quotes(matrix.rows.iter().map(KeyedEntry::folded)),
                    ),
                );
                continue;
            };
            let values: &[RawValue<'_>] = match &row.value {
                MatrixRow::Values(values) => values,
                MatrixRow::Expression(_) => continue,
                // The parser has reported the row. The reference keeps it with no values.
                MatrixRow::Invalid => &[],
            };
            if values
                .iter()
                .any(|value| value::matches(value, &assign.value))
            {
                continue;
            }
            let possible: Vec<String> = values.iter().map(value::display).collect();
            cx.report(
                &UNMATCHED_EXCLUDE_VALUE,
                assign.value.span(),
                format!(
                    "value {} in \"exclude\" does not match in matrix {} combinations. possible values are {}",
                    value::display(&assign.value),
                    quote(name),
                    possible.join(", "),
                ),
            );
        }
    }
}
