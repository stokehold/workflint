//! The `runner-label` rule: unknown `runs-on` labels, and labels that no one runner can have
//! together.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `rule_runner_label.go`, lines 119-333. A
//! label with `${{ }}` that is not exactly `${{ matrix.<prop> }}` is evaluated to the strings it
//! can be (`docs/differences.md#runs-on-expressions`), where the reference skips it. Labels of
//! third-party runner providers are known (`docs/differences.md#runner-provider-labels`).

use std::borrow::Cow;

use workflint_data::{
    GITHUB_HOSTED_RUNNER_LABELS, RunnerOsCompat, SELF_HOSTED_RUNNER_OS_LABELS,
    SELF_HOSTED_RUNNER_OTHER_LABELS, is_self_hosted_other_label, runner_label_os_compat,
    runner_provider,
};
use workflint_diagnostics::quote::{quote, quotes_all};
use workflint_diagnostics::{Category, RuleMeta, Severity, Span};
use workflint_expr::ExprKind;
use workflint_syntax::{
    Job, KeyedEntry, Matrix, MatrixCombination, MatrixRow, Placeholder, RawValue, Template,
    contains_expression, is_expression_assigned,
};

use crate::path_match::path_match;
use crate::{Context, Rule};

/// A `runs-on` label is not a GitHub-hosted label, a self-hosted preset, or a configured label.
pub(crate) static UNKNOWN_RUNNER_LABEL: RuleMeta = RuleMeta {
    id: "unknown-runner-label",
    description: "A `runs-on` label is not a known runner label",
    kind: "runner-label",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/unknown-runner-label.md",
};

/// Two `runs-on` labels of one job name runner images that no one runner can have, such as
/// `ubuntu-latest` and `windows-latest`.
pub(crate) static RUNNER_LABEL_CONFLICT: RuleMeta = RuleMeta {
    id: "runner-label-conflict",
    description: "Two `runs-on` labels of a job cannot be on the same runner",
    kind: "runner-label",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/runner-label-conflict.md",
};

/// A configured runner label pattern is not a valid glob.
pub(crate) static INVALID_RUNNER_LABEL_PATTERN: RuleMeta = RuleMeta {
    id: "invalid-runner-label-pattern",
    description: "A runner label pattern in the config is not a valid glob",
    kind: "runner-label",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-runner-label-pattern.md",
};

/// The reference's `RuleRunnerLabel`. It checks `runs-on` of each job.
pub(crate) struct RunnerLabel {
    /// The label patterns of the config's `self-hosted-runner.labels`, each a Go `path.Match`
    /// pattern. The linter passes none until M5.2 loads the config.
    known: Vec<String>,
}

/// A label to check: its text and where to report it. A label from a matrix is reported at the
/// matrix value, and a value of an evaluated label at the label.
#[derive(Clone)]
struct Label<'a> {
    value: Cow<'a, str>,
    span: Span,
}

/// The labels of one job that have images so far, the reference's `compats`, keeping the first
/// label of each set of images in the order they came. The reference keeps a Go map and names
/// a random conflicting partner. This names the earliest (`docs/differences.md#deterministic-order`).
#[derive(Default)]
struct Seen<'a> {
    labels: Vec<(RunnerOsCompat, Label<'a>)>,
}

impl RunnerLabel {
    /// Creates the rule with the configured label patterns.
    pub(crate) fn new(known: Vec<String>) -> Self {
        Self { known }
    }

    /// Checks one label, as the reference's `verifyRunnerLabel` does, and returns its images, or
    /// `None` for a label without images, such as `self-hosted` or a custom label.
    fn verify(&self, label: &Label<'_>, cx: &mut Context<'_>) -> Option<RunnerOsCompat> {
        if let Some(os) = runner_label_os_compat(&label.value) {
            return Some(os);
        }
        if is_self_hosted_other_label(&label.value) {
            return None;
        }
        for pattern in &self.known {
            match path_match(pattern, &label.value) {
                Ok(true) => return None,
                Ok(false) => {}
                Err(err) => {
                    cx.report(
                        &INVALID_RUNNER_LABEL_PATTERN,
                        label.span,
                        format!(
                            "label pattern {} is an invalid glob. kindly check list of labels in actionlint.yaml config file: {err}",
                            quote(pattern)
                        ),
                    );
                    return None;
                }
            }
        }
        // The reference knows no provider labels (`docs/differences.md#runner-provider-labels`).
        // They have no images, so they conflict with no label.
        if runner_provider(&label.value).is_some() {
            return None;
        }
        let known: Vec<&str> = self.known.iter().map(String::as_str).collect();
        cx.report(
            &UNKNOWN_RUNNER_LABEL,
            label.span,
            format!(
                "label {} is unknown. available labels are {}. if it is a custom label for self-hosted runner, set list of labels in actionlint.yaml config file",
                quote(&label.value),
                quotes_all([
                    GITHUB_HOSTED_RUNNER_LABELS,
                    SELF_HOSTED_RUNNER_OTHER_LABELS,
                    SELF_HOSTED_RUNNER_OS_LABELS,
                    known.as_slice(),
                ]),
            ),
        );
        None
    }

    /// Checks the only label of a job, which cannot conflict, as the reference's `checkLabel`
    /// does.
    fn check_label<'a>(
        &self,
        label: &'a Template<'a>,
        matrix: Option<&'a Matrix<'a>>,
        cx: &mut Context<'_>,
    ) {
        if contains_expression(label.value()) {
            let labels = expression_labels(&plain(label), label.placeholders(), matrix);
            for label in labels {
                self.verify(&label, cx);
            }
            return;
        }
        self.verify(&plain(label), cx);
    }

    /// Checks a label and whether it conflicts with the labels before it, as the reference's
    /// `checkLabelAndConflict` does. The values of a label with `${{ }}` are checked against
    /// the labels before it, and then each is added to them, so the values do not conflict
    /// with each other.
    fn check_label_and_conflict<'a>(
        &self,
        label: Label<'a>,
        placeholders: &[Placeholder],
        matrix: Option<&'a Matrix<'a>>,
        seen: &mut Seen<'a>,
        cx: &mut Context<'_>,
    ) {
        if contains_expression(&label.value) {
            let labels = expression_labels(&label, placeholders, matrix);
            let compats: Vec<_> = labels.iter().map(|label| self.verify(label, cx)).collect();
            let compats: Vec<_> = compats
                .into_iter()
                .zip(&labels)
                .map(|(os, label)| os.filter(|os| seen.check_conflict(*os, label, cx)))
                .collect();
            for (os, label) in compats.into_iter().zip(labels) {
                if let Some(os) = os {
                    seen.add(os, label);
                }
            }
            return;
        }
        if let Some(os) = self.verify(&label, cx)
            && seen.check_conflict(os, &label, cx)
        {
            seen.add(os, label);
        }
    }
}

impl<'a> Seen<'a> {
    /// Reports `label` if its images share none with those of an earlier label, naming the
    /// earliest such label, and returns whether it conflicts with none.
    fn check_conflict(&self, os: RunnerOsCompat, label: &Label<'_>, cx: &mut Context<'_>) -> bool {
        let Some((_, partner)) = self.labels.iter().find(|(seen, _)| !seen.intersects(os)) else {
            return true;
        };
        let defined_at = cx.file().line_col(partner.span.start);
        cx.report(
            &RUNNER_LABEL_CONFLICT,
            label.span,
            format!(
                "label {} conflicts with label {} defined at {defined_at}. note: to run your job on each workers, use matrix",
                quote(&label.value),
                quote(&partner.value),
            ),
        );
        false
    }

    /// Adds a label that conflicts with none before it, unless a label with the same images came
    /// first.
    fn add(&mut self, os: RunnerOsCompat, label: Label<'a>) {
        if !self.labels.iter().any(|(seen, _)| *seen == os) {
            self.labels.push((os, label));
        }
    }
}

impl<'a> Rule<'a> for RunnerLabel {
    fn job_pre(&mut self, job: &'a KeyedEntry<'a, Job<'a>>, cx: &mut Context<'_>) {
        let Some(runner) = job.value.runs_on.value() else {
            return;
        };
        let matrix = job
            .value
            .strategy
            .value()
            .and_then(|strategy| strategy.matrix.literal());
        let mut seen = Seen::default();
        if let Some(expr) = runner.labels.expression() {
            // A whole `runs-on: ${{ }}` or `labels: ${{ }}`, the reference's `LabelsExpr`,
            // which it checks as one of several labels.
            let label = Label {
                value: Cow::Borrowed(expr.source.value()),
                span: expr.span(),
            };
            let placeholders = std::slice::from_ref(expr.placeholder());
            self.check_label_and_conflict(label, placeholders, matrix, &mut seen, cx);
            return;
        }
        let Some(labels) = runner.labels.literal() else {
            return;
        };
        // The reference counts an invalid label, which it keeps as `""`, so the count includes
        // the slots of invalid labels, which are then skipped.
        if let [only] = &labels[..] {
            if let Some(label) = only {
                self.check_label(label, matrix, cx);
            }
            return;
        }
        for label in labels.iter().flatten() {
            let placeholders = label.placeholders();
            self.check_label_and_conflict(plain(label), placeholders, matrix, &mut seen, cx);
        }
    }
}

fn plain<'a>(label: &'a Template<'a>) -> Label<'a> {
    Label {
        value: Cow::Borrowed(label.value()),
        span: label.span(),
    }
}

/// Returns the labels that a label with `${{ }}` can be. A label of the exact form
/// `${{ matrix.<prop> }}` has the values of the matrix, as the reference's
/// `tryToGetLabelsInMatrix` gives them. Any other label is evaluated, and its values are
/// reported at the label. A label whose values cannot be worked out has none.
fn expression_labels<'a>(
    label: &Label<'_>,
    placeholders: &[Placeholder],
    matrix: Option<&'a Matrix<'a>>,
) -> Vec<Label<'a>> {
    if let Some(prop) = matrix_prop(&label.value, placeholders) {
        return matrix
            .map(|matrix| matrix_values(prop, matrix))
            .unwrap_or_default();
    }
    eval::label_values(&label.value, placeholders, matrix)
        .unwrap_or_default()
        .into_iter()
        .map(|value| Label {
            value: Cow::Owned(value),
            span: label.span,
        })
        .collect()
}

/// Returns `<prop>`, folded, when `value` is exactly `${{ matrix.<prop> }}` and its first
/// placeholder parses.
fn matrix_prop<'p>(value: &str, placeholders: &'p [Placeholder]) -> Option<&'p str> {
    if !is_expression_assigned(value) {
        return None;
    }
    let expr = placeholders.first()?.expr.as_ref().ok()?;
    let ExprKind::Property { receiver, name } = &expr.kind else {
        return None;
    };
    let ExprKind::Variable(variable) = &receiver.kind else {
        return None;
    };
    (variable.as_str() == "matrix").then_some(name.as_str())
}

/// Returns the string values of the matrix row `prop` and of `prop` in each `include` item, in
/// that order. Values that are not scalars, or that contain an expression, are left out.
fn matrix_values<'a>(prop: &str, matrix: &'a Matrix<'a>) -> Vec<Label<'a>> {
    let mut labels = Vec::new();
    if let Some(row) = matrix.rows.get(prop)
        && let MatrixRow::Values(values) = &row.value
    {
        labels.extend(values.iter().filter_map(scalar_label));
    }
    if let Some(include) = matrix.include.literal() {
        for combination in include.iter() {
            if let MatrixCombination::Assigns { assigns, .. } = combination
                && let Some(assign) = assigns.get(prop)
            {
                labels.extend(scalar_label(&assign.value));
            }
        }
    }
    labels
}

/// Returns a matrix value as a label if it is a scalar without an expression.
fn scalar_label<'a>(value: &'a RawValue<'a>) -> Option<Label<'a>> {
    match value {
        RawValue::Scalar(scalar) if !contains_expression(scalar.value()) => Some(plain(scalar)),
        _ => None,
    }
}

mod combinations;
mod eval;
#[cfg(test)]
mod tests;
