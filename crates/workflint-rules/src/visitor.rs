//! The visitor, the reference's `Visitor` (`pass.go`).

use workflint_diagnostics::{Diagnostic, SourceFile};
use workflint_syntax::Workflow;

use crate::local::{LocalMetadata, NoProject};
use crate::{Context, Rule, create_rules};

/// Calls the hooks of `rules` over `workflow`, each hook for every rule in the order of `rules`.
///
/// The order is the reference's: the workflow-pre hook, then for each job the job-pre hook, the
/// step hook for each step, and the job-post hook, then the workflow-post hook. Jobs are visited
/// in source order, where the reference visits them in Go's random map order
/// (`docs/differences.md#deterministic-order`). An invalid `jobs` or `steps` value, which the
/// parser has reported, has no jobs or steps to visit. The context learns the alias paths of
/// `workflow`, so [`Context::report_in`] can label diagnostics with them.
pub fn visit<'a>(
    workflow: &'a Workflow<'a>,
    rules: &mut [Box<dyn Rule<'a> + 'a>],
    cx: &mut Context<'_>,
) {
    cx.aliases = workflow.aliases.clone();
    for rule in rules.iter_mut() {
        rule.workflow_pre(workflow, cx);
    }
    for job in workflow.jobs.value().into_iter().flatten() {
        for rule in rules.iter_mut() {
            rule.job_pre(job, cx);
        }
        for step in job
            .value
            .steps
            .value()
            .into_iter()
            .flat_map(|steps| steps.iter())
        {
            for rule in rules.iter_mut() {
                rule.step(step, cx);
            }
        }
        for rule in rules.iter_mut() {
            rule.job_post(job, cx);
        }
    }
    for rule in rules.iter_mut() {
        rule.workflow_post(workflow, cx);
    }
}

/// Creates the rules for `file` with [`create_rules`], visits `workflow` with them outside any
/// project, and returns their diagnostics, unsorted.
#[must_use]
pub fn run<'a>(file: &SourceFile, workflow: &'a Workflow<'a>) -> Vec<Diagnostic> {
    run_in_project(file, workflow, &NoProject)
}

/// Does what [`run`] does, for a file whose project has the local actions and local reusable
/// workflows in `local`.
#[must_use]
pub fn run_in_project<'a>(
    file: &SourceFile,
    workflow: &'a Workflow<'a>,
    local: &dyn LocalMetadata,
) -> Vec<Diagnostic> {
    let mut rules = create_rules();
    let mut cx = Context::with_local_metadata(file, local);
    visit(workflow, &mut rules, &mut cx);
    cx.into_diagnostics()
}
