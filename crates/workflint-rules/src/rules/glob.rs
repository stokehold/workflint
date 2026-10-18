//! The `glob` rule: branch, tag, and path filters, `image_version` versions, and
//! `snapshot.version`.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `rule_glob.go`. The validator is
//! [`workflint_syntax::glob`].

use std::sync::Arc;

use workflint_diagnostics::{Category, RuleMeta, Severity};
use workflint_syntax::glob::{GlobProblem, GlobProblemKind, validate_path_glob, validate_ref_glob};
use workflint_syntax::{Event, Field, Job, KeyedEntry, Str, Template, Workflow};

use crate::{Context, Rule};

/// A filter pattern is not valid glob syntax, or contains a newline.
pub(crate) static INVALID_GLOB: RuleMeta = RuleMeta {
    id: "invalid-glob",
    description: "A filter pattern is not valid glob syntax, or contains a newline",
    kind: "glob",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-glob.md",
};

/// A branch or tag pattern has a character that a Git ref name cannot have.
pub(crate) static INVALID_REF_NAME: RuleMeta = RuleMeta {
    id: "invalid-ref-name",
    description: "A branch or tag pattern has a character that a Git ref name cannot have",
    kind: "glob",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-ref-name.md",
};

/// A path pattern has white space at either end.
pub(crate) static GLOB_PATH_WHITESPACE: RuleMeta = RuleMeta {
    id: "glob-path-whitespace",
    description: "A path pattern has white space at either end",
    kind: "glob",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/glob-path-whitespace.md",
};

/// A path pattern is `.` or `..`, or starts with `./` or `../`.
pub(crate) static GLOB_PATH_DOT: RuleMeta = RuleMeta {
    id: "glob-path-dot",
    description: "A path pattern is `.` or `..`, or starts with `./` or `../`",
    kind: "glob",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/glob-path-dot.md",
};

/// The note that ends every message.
const CHEAT_SHEET: &str = "note: filter pattern syntax is explained at https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#filter-pattern-cheat-sheet";

/// The reference's `RuleGlob`. It checks the filters of each event in `on`, and the
/// `snapshot.version` of each job.
pub(crate) struct Glob;

impl<'a> Rule<'a> for Glob {
    fn workflow_pre(&mut self, workflow: &'a Workflow<'a>, cx: &mut Context<'_>) {
        let Some(events) = workflow.on.value() else {
            return;
        };
        for event in events.iter().flatten() {
            match event {
                Event::Webhook(event) => {
                    check_filter(&event.branches, validate_ref_glob, cx);
                    check_filter(&event.branches_ignore, validate_ref_glob, cx);
                    check_filter(&event.tags, validate_ref_glob, cx);
                    check_filter(&event.tags_ignore, validate_ref_glob, cx);
                    check_filter(&event.paths, validate_path_glob, cx);
                    check_filter(&event.paths_ignore, validate_path_glob, cx);
                }
                Event::ImageVersion(event) => {
                    check_filter(&event.versions, validate_ref_glob, cx);
                }
                Event::Schedule(_)
                | Event::WorkflowDispatch(_)
                | Event::RepositoryDispatch(_)
                | Event::WorkflowCall(_) => {}
            }
        }
    }

    fn job_pre(&mut self, job: &'a KeyedEntry<'a, Job<'a>>, cx: &mut Context<'_>) {
        if let Some(version) = job
            .value
            .snapshot
            .value()
            .and_then(|snapshot| snapshot.version.value())
        {
            check(*version, validate_ref_glob, cx);
        }
    }
}

/// Checks each pattern of a filter with `validate`. An item that is not a string is skipped,
/// because the parser has reported it.
fn check_filter(
    filter: &Field<'_, Arc<[Option<Template<'_>>]>>,
    validate: fn(&str) -> Vec<GlobProblem>,
    cx: &mut Context<'_>,
) {
    let Some(patterns) = filter.value() else {
        return;
    };
    for pattern in patterns.iter().flatten() {
        check(pattern.source, validate, cx);
    }
}

/// Reports each problem that `validate` finds in `pattern`. An empty pattern is skipped, because
/// the parser has reported it.
///
/// A problem with a byte range is reported at the source of that range. A problem without one is
/// about the whole pattern, so it is reported at the whole value
/// (docs/differences.md#positions-inside-scalars).
fn check(pattern: Str<'_>, validate: fn(&str) -> Vec<GlobProblem>, cx: &mut Context<'_>) {
    let value = pattern.value();
    if value.is_empty() {
        return;
    }
    for problem in validate(value) {
        let rule = match problem.kind {
            GlobProblemKind::Syntax => &INVALID_GLOB,
            GlobProblemKind::RefName => &INVALID_REF_NAME,
            GlobProblemKind::PathWhitespace => &GLOB_PATH_WHITESPACE,
            GlobProblemKind::PathDot => &GLOB_PATH_DOT,
        };
        let range = problem.range.unwrap_or(0..value.len());
        let span = pattern
            .source_span(cx.file(), range)
            .unwrap_or_else(|| pattern.span());
        cx.report(rule, span, format!("{}. {CHEAT_SHEET}", problem.message));
    }
}

#[cfg(test)]
mod tests;
