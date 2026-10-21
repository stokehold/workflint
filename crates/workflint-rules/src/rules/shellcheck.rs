//! The `shellcheck` rule: findings of shellcheck in `run:` scripts, and scripts that shellcheck
//! could not check.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `rule_shellcheck.go`. The rule picks each
//! step's shell and prepares its script as the reference does, and passes the scripts of the
//! file to the [`ExternalTools`](crate::tools::ExternalTools) of the context, which run
//! shellcheck. Each finding points at its line and column in the workflow file, mapped through
//! the source map of the `run:` value, where the reference points at the `run:` key
//! (`docs/differences.md#script-finding-positions`). A script that shellcheck could not check gets
//! one `shellcheck-failed` diagnostic, where the reference stops the run
//! (`docs/differences.md#tool-failures`).

use std::ops::Range;

use workflint_diagnostics::{Category, RuleMeta, Severity, Span};
use workflint_syntax::{Defaults, Exec, Field, Job, KeyedEntry, Step, Template, Workflow};
use workflint_yaml::simple_lowercase;

use crate::kind::{self, Kind};
use crate::tools::{ScriptShell, ShellScript, ShellcheckFinding, ShellcheckOutcome};
use crate::{Context, Rule};

/// shellcheck reports a problem in a `run:` script.
pub(crate) static SHELLCHECK_ISSUE: RuleMeta = RuleMeta {
    id: "shellcheck-issue",
    description: "shellcheck reports a problem in a `run:` script",
    kind: "shellcheck",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/shellcheck-issue.md",
};

/// shellcheck did not check a `run:` script: it could not start, failed, was killed, or printed
/// output that is not its JSON.
pub(crate) static SHELLCHECK_FAILED: RuleMeta = RuleMeta {
    id: "shellcheck-failed",
    description: "shellcheck could not check a `run:` script",
    kind: "shellcheck",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/shellcheck-failed.md",
};

/// The reference's `RuleShellcheck`. It keeps the shells of the workflow's and the current job's
/// `defaults.run.shell` and of the job's runner, and the scripts of the file.
pub(crate) struct Shellcheck<'a> {
    workflow_shell: Option<&'a str>,
    job_shell: Option<&'a str>,
    runner_shell: Option<&'static str>,
    scripts: Vec<Pending<'a>>,
}

/// A script waiting for shellcheck.
struct Pending<'a> {
    /// The `run:` value.
    run: &'a Template<'a>,
    /// The span of the `run:` key, where a finding goes that has no place in the script.
    key: Span,
    script: ShellScript,
}

impl Shellcheck<'_> {
    pub(crate) fn new() -> Self {
        Self {
            workflow_shell: None,
            job_shell: None,
            runner_shell: None,
            scripts: Vec::new(),
        }
    }
}

impl<'a> Rule<'a> for Shellcheck<'a> {
    fn kind(&self) -> &'static Kind {
        &kind::SHELLCHECK
    }

    fn workflow_pre(&mut self, workflow: &'a Workflow<'a>, _cx: &mut Context<'_>) {
        self.workflow_shell = default_shell(&workflow.defaults);
    }

    fn job_pre(&mut self, job: &'a KeyedEntry<'a, Job<'a>>, _cx: &mut Context<'_>) {
        self.job_shell = default_shell(&job.value.defaults);
        // The default shell on Windows is PowerShell.
        let windows = job
            .value
            .runs_on
            .value()
            .and_then(|runner| runner.labels.literal())
            .into_iter()
            .flat_map(|labels| labels.iter().flatten())
            .any(|label| {
                let label = simple_lowercase(label.value());
                label == "windows" || label.starts_with("windows-")
            });
        if windows {
            self.runner_shell = Some("pwsh");
        }
    }

    fn step(&mut self, step: &'a Step<'a>, cx: &mut Context<'_>) {
        if !cx.tools().has_shellcheck() {
            return;
        }
        let Some(Exec::Run(exec)) = &step.exec else {
            return;
        };
        let (Some(run), Some(key)) = (exec.run.value(), exec.run.key()) else {
            return;
        };
        // An invalid `shell` is an empty string in the reference, which no shell matches.
        let shell = match &*exec.shell {
            Field::Valid { value, .. } => value.value(),
            Field::Invalid { .. } => "",
            Field::Absent => self
                .job_shell
                .or(self.workflow_shell)
                .or(self.runner_shell)
                .unwrap_or("bash"),
        };
        let Some(shell) = script_shell(shell) else {
            return;
        };
        self.scripts.push(Pending {
            run,
            key: key.span(),
            script: ShellScript {
                shell,
                text: stdin(shell, &sanitize(run.value())),
            },
        });
    }

    fn job_post(&mut self, _job: &'a KeyedEntry<'a, Job<'a>>, _cx: &mut Context<'_>) {
        self.job_shell = None;
        self.runner_shell = None;
    }

    fn workflow_post(&mut self, _workflow: &'a Workflow<'a>, cx: &mut Context<'_>) {
        self.workflow_shell = None;
        if self.scripts.is_empty() {
            return;
        }
        let scripts: Vec<ShellScript> = self
            .scripts
            .iter()
            .map(|pending| pending.script.clone())
            .collect();
        let outcomes = cx.tools().shellcheck(&scripts);
        for (pending, outcome) in self.scripts.iter().zip(outcomes) {
            report(pending, &outcome, cx);
        }
    }
}

/// The `defaults.run.shell` of `defaults`, unless it is empty or invalid, which the reference
/// reads as no shell.
fn default_shell<'a>(defaults: &'a Field<'a, Defaults<'a>>) -> Option<&'a str> {
    defaults
        .value()
        .and_then(|defaults| defaults.run.value())
        .and_then(|run| run.shell.value())
        .map(Template::value)
        .filter(|shell| !shell.is_empty())
}

/// The shell that shellcheck checks a script of `shell` for, or `None` if shellcheck does not
/// support it. The match is case-sensitive, as in the reference.
fn script_shell(shell: &str) -> Option<ScriptShell> {
    if shell == "bash" || shell.starts_with("bash ") {
        Some(ScriptShell::Bash)
    } else if shell == "sh" || shell.starts_with("sh ") {
        Some(ScriptShell::Sh)
    } else {
        None
    }
}

/// Replaces each `${{ ... }}` in `script` with as many bytes of `_`, the reference's
/// `sanitizeExpressionsInScript`. A `${{` without a `}}` after it stays. The result has the
/// length of `script`, so an offset in it is the same offset in `script`. The `pyflakes` rule
/// uses it too.
pub(crate) fn sanitize(script: &str) -> String {
    let mut out = String::with_capacity(script.len());
    let mut rest = script;
    while let Some(start) = rest.find("${{") {
        let Some(end) = rest.get(start..).and_then(|tail| tail.find("}}")) else {
            break;
        };
        let end = start + end + 2;
        out.push_str(rest.get(..start).unwrap_or_default());
        out.extend(std::iter::repeat_n('_', end - start));
        rest = rest.get(end..).unwrap_or_default();
    }
    out.push_str(rest);
    out
}

/// The text that shellcheck reads on stdin: the options that GitHub runs the shell with, on a
/// line of their own, then the script and a line break.
fn stdin(shell: ScriptShell, script: &str) -> String {
    let setup = match shell {
        ScriptShell::Bash => "set -eo pipefail",
        ScriptShell::Sh => "set -e",
    };
    format!("{setup}\n{script}\n")
}

/// Reports the findings of `pending`, or the failure of shellcheck on it.
fn report(pending: &Pending<'_>, outcome: &ShellcheckOutcome, cx: &mut Context<'_>) {
    let source = pending.run.source;
    match outcome {
        ShellcheckOutcome::Findings(findings) => {
            let spans = source.spans(cx.file());
            let script = sanitize(pending.run.value());
            for finding in findings {
                let span = range(&script, finding).map_or(pending.key, |range| spans.span(range));
                cx.report_in(source, &SHELLCHECK_ISSUE, span, message(finding));
            }
        }
        ShellcheckOutcome::Failed(message) => {
            cx.report_in(source, &SHELLCHECK_FAILED, pending.key, message.clone());
        }
    }
}

/// The reference's message: the code, level, line, and column of the finding, with the line
/// counted in the script, and shellcheck's message without its final period.
fn message(finding: &ShellcheckFinding) -> String {
    let text = finding
        .message
        .strip_suffix('.')
        .unwrap_or(&finding.message);
    format!(
        "shellcheck reported issue in this script: SC{}:{}:{}:{}: {text}",
        finding.code,
        finding.level,
        finding.line.saturating_sub(1),
        finding.column
    )
}

/// The byte range of `finding` in `script`, the `run:` value with each `${{ }}` replaced, or
/// `None` if the finding starts on the line that [`stdin`] adds before the script.
fn range(script: &str, finding: &ShellcheckFinding) -> Option<Range<usize>> {
    let start = offset(script, finding.line, finding.column)?;
    let end = match (finding.end_line, finding.end_column) {
        (Some(line), Some(column)) => offset(script, line, column).unwrap_or(start),
        _ => start,
    };
    Some(start..end.max(start))
}

/// The byte offset in `script` of a line and column of shellcheck, which count in the text on
/// stdin. Line 1 is the line that [`stdin`] adds, so it has no offset. A column inside a tab
/// gives the tab, and a line or column past the end gives the end.
fn offset(script: &str, line: i64, column: i64) -> Option<usize> {
    let index = usize::try_from(line.checked_sub(2)?).ok()?;
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
    let mut at = 1_i64;
    for (byte, c) in text.char_indices() {
        let next = if c == '\t' {
            (at - 1) / 8 * 8 + 9
        } else {
            at + 1
        };
        if column < next {
            return Some(start + byte);
        }
        at = next;
    }
    Some(start + text.len())
}

#[cfg(test)]
mod tests;
