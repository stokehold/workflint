//! The `workflow-call` rule: the format of each job's `uses:`, and the inputs and secrets of
//! each call to a local reusable workflow.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `rule_workflow_call.go`.

use workflint_diagnostics::quote::{quote, sorted_quotes};
use workflint_diagnostics::{Category, RuleMeta, Severity};
use workflint_syntax::{
    Job, JobUses, JobUsesKind, Keyed, KeyedEntry, ReusableWorkflowMetadata, Template, Workflow,
    WorkflowCall as Call, WorkflowCallSecrets,
};

use crate::local::{LocalMetadataError, Lookup};
use crate::{Context, Rule};

/// A job's `uses:` is neither `./path` nor `owner/repo/path@ref`.
pub(crate) static INVALID_REUSABLE_WORKFLOW_REF: RuleMeta = RuleMeta {
    id: "invalid-reusable-workflow-ref",
    description: "A job's `uses:` is not `./path/to/workflow.yml` or `owner/repo/path/to/workflow.yml@ref`",
    kind: "workflow-call",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-reusable-workflow-ref.md",
};

/// The file of a local reusable workflow cannot be read.
pub(crate) static REUSABLE_WORKFLOW_NOT_FOUND: RuleMeta = RuleMeta {
    id: "reusable-workflow-not-found",
    description: "The file of a called local reusable workflow cannot be read",
    kind: "workflow-call",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/reusable-workflow-not-found.md",
};

/// A local reusable workflow is not valid YAML, has no `workflow_call` event, or its
/// `on.workflow_call` cannot be decoded.
pub(crate) static BROKEN_REUSABLE_WORKFLOW: RuleMeta = RuleMeta {
    id: "broken-reusable-workflow",
    description: "A called local reusable workflow has no valid `on.workflow_call` section",
    kind: "workflow-call",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/broken-reusable-workflow.md",
};

/// A call does not set an input that the local reusable workflow requires.
pub(crate) static MISSING_REQUIRED_WORKFLOW_INPUT: RuleMeta = RuleMeta {
    id: "missing-required-workflow-input",
    description: "A call does not set an input that the local reusable workflow requires",
    kind: "workflow-call",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/missing-required-workflow-input.md",
};

/// `with:` sets an input that the local reusable workflow does not declare.
pub(crate) static UNDEFINED_WORKFLOW_INPUT: RuleMeta = RuleMeta {
    id: "undefined-workflow-input",
    description: "`with:` sets an input that the local reusable workflow does not declare",
    kind: "workflow-call",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/undefined-workflow-input.md",
};

/// A call does not pass a secret that the local reusable workflow requires.
pub(crate) static MISSING_REQUIRED_WORKFLOW_SECRET: RuleMeta = RuleMeta {
    id: "missing-required-workflow-secret",
    description: "A call does not pass a secret that the local reusable workflow requires",
    kind: "workflow-call",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/missing-required-workflow-secret.md",
};

/// `secrets:` passes a secret that the local reusable workflow does not declare.
pub(crate) static UNDEFINED_WORKFLOW_SECRET: RuleMeta = RuleMeta {
    id: "undefined-workflow-secret",
    description: "`secrets:` passes a secret that the local reusable workflow does not declare",
    kind: "workflow-call",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/undefined-workflow-secret.md",
};

/// The reference's `RuleWorkflowCall`.
pub(crate) struct WorkflowCall;

impl<'a> Rule<'a> for WorkflowCall {
    /// Registers the workflow's `on.workflow_call`, so that later callers in the run do not read
    /// its file.
    fn workflow_pre(&mut self, workflow: &'a Workflow<'a>, cx: &mut Context<'_>) {
        if let Some(metadata) = ReusableWorkflowMetadata::from_workflow(workflow) {
            cx.local_metadata().register_workflow_call(metadata);
        }
    }

    fn job_pre(&mut self, job: &'a KeyedEntry<'a, Job<'a>>, cx: &mut Context<'_>) {
        let Some(call) = &job.value.workflow_call else {
            return;
        };
        // An empty or non-scalar `uses` is invalid, and the parser has reported it.
        let Some(uses) = call.uses.value() else {
            return;
        };
        match uses.kind {
            JobUsesKind::Expression | JobUsesKind::Repository { .. } => {}
            JobUsesKind::Local { .. } => check_local(call, uses, cx),
            JobUsesKind::Invalid(_) => {
                let spec = uses.value();
                // A later lookup would report the file as missing on top of this error.
                if spec.starts_with("./") {
                    cx.local_metadata().record_invalid_workflow(spec);
                }
                cx.report_in(
                    uses.template.source,
                    &INVALID_REUSABLE_WORKFLOW_REF,
                    uses.span(),
                    format!(
                        "reusable workflow call {} at \"uses\" is not following the format \"owner/repo/path/to/workflow.yml@ref\" nor \"./path/to/workflow.yml\". see https://docs.github.com/en/actions/learn-github-actions/reusing-workflows for more details",
                        quote(spec),
                    ),
                );
            }
        }
    }
}

/// The reference's `checkWorkflowCallUsesLocal`: looks up the callee and checks the call's
/// inputs and secrets against it.
fn check_local(call: &Call<'_>, uses: &JobUses<'_>, cx: &mut Context<'_>) {
    let spec = uses.value();
    let metadata = match cx.local_metadata().reusable_workflow(spec) {
        Lookup::None => return,
        Lookup::Found { metadata, .. } => metadata,
        Lookup::Error(err) => {
            report_lookup_error(cx, uses, &err);
            return;
        }
    };
    let callee = quote(spec);

    let inputs = call.inputs.value();
    let required = metadata.inputs.values().filter(|input| input.required);
    for input in required {
        if !inputs.is_some_and(|inputs| inputs.contains_key(&input.name)) {
            cx.report_in(
                uses.template.source,
                &MISSING_REQUIRED_WORKFLOW_INPUT,
                uses.span(),
                format!(
                    "input {} is required by {callee} reusable workflow",
                    quote(&input.name)
                ),
            );
        }
    }
    let declared = || metadata.inputs.values().map(|input| input.name.as_str());
    for entry in first_entries(inputs) {
        if metadata.inputs.contains_key(entry.folded()) {
            continue;
        }
        let name = entry.key.value();
        let mut message = format!(
            "input {} is not defined in {callee} reusable workflow. {}",
            quote(name),
            defined("input", metadata.inputs.len(), declared()),
        );
        if metadata.dispatch_inputs.contains(entry.folded()) {
            message.push_str(&dispatch_hint(name));
        }
        cx.report_in(
            entry.key,
            &UNDEFINED_WORKFLOW_INPUT,
            entry.key.span(),
            message,
        );
    }

    let secrets = match call.secrets.value() {
        Some(WorkflowCallSecrets::Inherit(_)) => return,
        Some(WorkflowCallSecrets::Map(secrets)) => Some(secrets),
        None => None,
    };
    let required = metadata.secrets.values().filter(|secret| secret.required);
    for secret in required {
        if !secrets.is_some_and(|secrets| secrets.contains_key(&secret.name)) {
            cx.report_in(
                uses.template.source,
                &MISSING_REQUIRED_WORKFLOW_SECRET,
                uses.span(),
                format!(
                    "secret {} is required by {callee} reusable workflow",
                    quote(&secret.name)
                ),
            );
        }
    }
    let declared = || metadata.secrets.values().map(|secret| secret.name.as_str());
    for entry in first_entries(secrets) {
        if metadata.secrets.contains_key(entry.folded()) {
            continue;
        }
        cx.report_in(
            entry.key,
            &UNDEFINED_WORKFLOW_SECRET,
            entry.key.span(),
            format!(
                "secret {} is not defined in {callee} reusable workflow. {}",
                quote(entry.key.value()),
                defined("secret", metadata.secrets.len(), declared()),
            ),
        );
    }
}

/// Reports `err`, which the lookup of the local reusable workflow at `uses` returned, at
/// `uses`. Only the first lookup of a callee in the run returns its error, so the `expression`
/// rule reports it too when it looks the callee up first.
pub(crate) fn report_lookup_error(
    cx: &mut Context<'_>,
    uses: &JobUses<'_>,
    err: &LocalMetadataError,
) {
    let rule = match err {
        LocalMetadataError::ReadWorkflow { .. } => &REUSABLE_WORKFLOW_NOT_FOUND,
        LocalMetadataError::Workflow { .. } | LocalMetadataError::Action { .. } => {
            &BROKEN_REUSABLE_WORKFLOW
        }
    };
    cx.report_in(uses.template.source, rule, uses.span(), err.to_string());
}

/// The entries of `keyed` in source order without the ones that repeat an earlier key, which
/// the parser has reported and the reference drops.
fn first_entries<'k, 'a>(
    keyed: Option<&'k Keyed<'a, Option<Template<'a>>>>,
) -> impl Iterator<Item = &'k KeyedEntry<'a, Option<Template<'a>>>> {
    keyed.into_iter().flat_map(|keyed| {
        keyed.iter().filter(|entry| {
            keyed
                .get(entry.folded())
                .is_some_and(|first| std::ptr::eq(first, *entry))
        })
    })
}

/// The reference's note on what the callee defines: `no input is defined`, `defined input is
/// "a"`, or `defined inputs are "a", "b"`, for `what` in `input` and `secret`.
fn defined<'n>(what: &str, count: usize, names: impl Iterator<Item = &'n str>) -> String {
    match count {
        0 => format!("no {what} is defined"),
        1 => format!("defined {what} is {}", sorted_quotes(names)),
        _ => format!("defined {what}s are {}", sorted_quotes(names)),
    }
}

/// The hint for an input that the callee declares under `workflow_dispatch` only
/// (rhysd/actionlint#336).
fn dispatch_hint(name: &str) -> String {
    format!(
        ". {} is defined in \"workflow_dispatch\" inputs, but a reusable workflow call sets only \"workflow_call\" inputs",
        quote(name)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_note_names_the_defined_inputs_in_sorted_order() {
        assert_eq!(defined("input", 0, [].into_iter()), "no input is defined");
        assert_eq!(
            defined("secret", 1, ["A"].into_iter()),
            "defined secret is \"A\""
        );
        assert_eq!(
            defined("input", 2, ["b", "a"].into_iter()),
            "defined inputs are \"a\", \"b\""
        );
    }
}
