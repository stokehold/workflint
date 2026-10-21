//! The `pyflakes` rule: findings of pyflakes, or of ruff, in the scripts of `shell: python`
//! steps, and scripts that the Python linter could not check.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `rule_pyflakes.go`. The rule picks the
//! Python steps as the reference does and passes their scripts, with each `${{ }}` replaced, to
//! the [`ExternalTools`](crate::tools::ExternalTools) of the context, with the `python.linter`
//! of the config. `auto`, the default, runs ruff if it is installed, and pyflakes otherwise
//! (`docs/differences.md#python-linter-auto`). Each finding points at its line and column in the
//! workflow file, where the reference points at the `run:` key
//! (`docs/differences.md#script-finding-positions`). A script that the linter could not check
//! gets one `pyflakes-failed` diagnostic, where the reference stops the run
//! (`docs/differences.md#tool-failures`).

use std::ops::Range;

use workflint_diagnostics::{Category, RuleMeta, Severity, Span};
use workflint_syntax::{Defaults, Exec, Field, Job, KeyedEntry, Step, Template, Workflow};

use super::shellcheck::sanitize;
use crate::config::PythonLinter;
use crate::kind::{self, Kind};
use crate::tools::{ColumnUnit, PythonFinding, PythonOutcome};
use crate::{Context, Rule};

/// pyflakes, or ruff, reports a problem in a Python `run:` script.
pub(crate) static PYFLAKES_ISSUE: RuleMeta = RuleMeta {
    id: "pyflakes-issue",
    description: "pyflakes or ruff reports a problem in a Python `run:` script",
    kind: "pyflakes",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/pyflakes-issue.md",
};

/// The Python linter did not check a `run:` script: it could not start, failed, was killed, or
/// printed output that workflint cannot read.
pub(crate) static PYFLAKES_FAILED: RuleMeta = RuleMeta {
    id: "pyflakes-failed",
    description: "pyflakes or ruff could not check a Python `run:` script",
    kind: "pyflakes",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/pyflakes-failed.md",
};

/// The reference's `RulePyflakes`. It keeps whether the workflow's and the current job's
/// `defaults.run.shell` are Python, `None` when they set no shell, and the scripts of the file.
pub(crate) struct Pyflakes<'a> {
    workflow_python: Option<bool>,
    job_python: Option<bool>,
    scripts: Vec<Pending<'a>>,
}

/// A script waiting for the Python linter.
struct Pending<'a> {
    /// The `run:` value.
    run: &'a Template<'a>,
    /// The span of the `run:` key, where a finding goes that has no place in the script.
    key: Span,
    /// The `run:` value with each `${{ }}` replaced, which the linter reads on stdin.
    script: String,
}

impl Pyflakes<'_> {
    pub(crate) fn new() -> Self {
        Self {
            workflow_python: None,
            job_python: None,
            scripts: Vec::new(),
        }
    }
}

impl<'a> Rule<'a> for Pyflakes<'a> {
    fn kind(&self) -> &'static Kind {
        &kind::PYFLAKES
    }

    fn workflow_pre(&mut self, workflow: &'a Workflow<'a>, _cx: &mut Context<'_>) {
        self.workflow_python = default_is_python(&workflow.defaults);
    }

    fn job_pre(&mut self, job: &'a KeyedEntry<'a, Job<'a>>, _cx: &mut Context<'_>) {
        self.job_python = default_is_python(&job.value.defaults);
    }

    fn step(&mut self, step: &'a Step<'a>, cx: &mut Context<'_>) {
        if cx.config().python_linter == PythonLinter::Off {
            return;
        }
        let Some(Exec::Run(exec)) = &step.exec else {
            return;
        };
        let (Some(run), Some(key)) = (exec.run.value(), exec.run.key()) else {
            return;
        };
        let python = is_python(&exec.shell)
            .or(self.job_python)
            .or(self.workflow_python)
            .unwrap_or(false);
        if !python {
            return;
        }
        self.scripts.push(Pending {
            run,
            key: key.span(),
            script: sanitize(run.value()),
        });
    }

    fn job_post(&mut self, _job: &'a KeyedEntry<'a, Job<'a>>, _cx: &mut Context<'_>) {
        self.job_python = None;
    }

    fn workflow_post(&mut self, _workflow: &'a Workflow<'a>, cx: &mut Context<'_>) {
        self.workflow_python = None;
        if self.scripts.is_empty() {
            return;
        }
        let scripts: Vec<String> = self
            .scripts
            .iter()
            .map(|pending| pending.script.clone())
            .collect();
        let outcomes = cx.tools().python(cx.config().python_linter, &scripts);
        for (pending, outcome) in self.scripts.iter().zip(outcomes) {
            report(pending, &outcome, cx);
        }
    }
}

/// Whether `shell` runs Python: `Some(true)` for `python` or a command that starts with
/// `python `, `Some(false)` for any other shell, and `None` when no shell is set. The reference
/// reads an invalid `shell` as the empty string, which is not Python.
fn is_python(shell: &Field<'_, Template<'_>>) -> Option<bool> {
    match shell {
        Field::Valid { value, .. } => {
            let shell = value.value();
            Some(shell == "python" || shell.starts_with("python "))
        }
        Field::Invalid { .. } => Some(false),
        Field::Absent => None,
    }
}

/// Whether the `defaults.run.shell` of `defaults` runs Python, as [`is_python`] says, or `None`
/// if it sets no shell.
fn default_is_python(defaults: &Field<'_, Defaults<'_>>) -> Option<bool> {
    defaults
        .value()
        .and_then(|defaults| defaults.run.value())
        .and_then(|run| is_python(&run.shell))
}

/// Reports the findings of `pending`, or the failure of the linter on it.
fn report(pending: &Pending<'_>, outcome: &PythonOutcome, cx: &mut Context<'_>) {
    let source = pending.run.source;
    match outcome {
        PythonOutcome::Findings(findings) => {
            let spans = source.spans(cx.file());
            for finding in findings {
                let span =
                    range(&pending.script, finding).map_or(pending.key, |range| spans.span(range));
                cx.report_in(
                    source,
                    &PYFLAKES_ISSUE,
                    span,
                    format!("pyflakes reported issue in this script: {}", finding.text),
                );
            }
        }
        PythonOutcome::Failed(message) => {
            cx.report_in(source, &PYFLAKES_FAILED, pending.key, message.clone());
        }
    }
}

/// The byte range of `finding` in `script`, or `None` if its line is before the first.
fn range(script: &str, finding: &PythonFinding) -> Option<Range<usize>> {
    let start = offset(script, finding.line, finding.column, finding.unit)?;
    let end = finding
        .end
        .and_then(|(line, column)| offset(script, line, Some(column), finding.unit))
        .unwrap_or(start);
    Some(start..end.max(start))
}

/// The byte offset in `script` of a line and column of the linter, both from 1, with the column
/// counted in `unit`. No column gives the start of the line. A column inside a character gives
/// the character, and a line or column past the end gives the end.
fn offset(script: &str, line: i64, column: Option<i64>, unit: ColumnUnit) -> Option<usize> {
    let index = usize::try_from(line.checked_sub(1)?).ok()?;
    let Some(start) = script
        .split_inclusive('\n')
        .scan(0, |at, line| {
            let start = *at;
            *at += line.len();
            Some(start)
        })
        .nth(index)
    else {
        return Some(script.len());
    };
    let text = script.get(start..).unwrap_or_default();
    let text = text.split('\n').next().unwrap_or_default();
    let skip = column
        .and_then(|column| usize::try_from(column.saturating_sub(1)).ok())
        .unwrap_or(0);
    let at = match unit {
        ColumnUnit::Bytes => text
            .char_indices()
            .map(|(byte, _)| byte)
            .take_while(|&byte| byte <= skip)
            .last()
            .filter(|_| skip < text.len())
            .unwrap_or(text.len()),
        ColumnUnit::Chars => text
            .char_indices()
            .nth(skip)
            .map_or(text.len(), |(byte, _)| byte),
    };
    Some(start + at)
}

#[cfg(test)]
mod tests;
