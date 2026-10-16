//! The rules whose diagnostics the workflow parser reports (ADR 0024).
//!
//! Each section parser reports through these values, so a rule has one
//! `RuleMeta` however many sections report it.

use workflint_diagnostics::{Category, RuleMeta, Severity};

/// The file has no YAML document, or only comments: `workflow is empty`.
pub static EMPTY_WORKFLOW: RuleMeta = syntax_check(
    "empty-workflow",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/empty-workflow.md",
);

/// A mapping has a `<<` key, quoted or not. GitHub Actions does not apply
/// YAML merge keys, so the entry is skipped.
pub static MERGE_KEY_UNSUPPORTED: RuleMeta = syntax_check(
    "merge-key-unsupported",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/merge-key-unsupported.md",
);

/// A required section or key is missing, such as `on` or `jobs` at the
/// workflow root, or `type` of a `workflow_call` input.
pub static MISSING_REQUIRED_KEY: RuleMeta = syntax_check(
    "missing-required-key",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/missing-required-key.md",
);

/// A mapping has a key that its section does not define.
pub static UNEXPECTED_KEY: RuleMeta = syntax_check(
    "unexpected-key",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/unexpected-key.md",
);

/// A mapping repeats a key. The first entry is kept, and the duplicate's
/// value is not parsed.
pub static DUPLICATE_KEY: RuleMeta = syntax_check(
    "duplicate-key",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/duplicate-key.md",
);

/// A mapping or sequence that must have entries is empty or null.
pub static EMPTY_SECTION: RuleMeta = syntax_check(
    "empty-section",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/empty-section.md",
);

/// A string that must not be empty is empty.
pub static EMPTY_STRING: RuleMeta = syntax_check(
    "empty-string",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/empty-string.md",
);

/// A node has the wrong kind, such as a sequence where a mapping is
/// expected.
pub static UNEXPECTED_NODE_KIND: RuleMeta = syntax_check(
    "unexpected-node-kind",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/unexpected-node-kind.md",
);

/// A boolean, integer, or float value has the wrong tag or does not parse.
pub static INVALID_LITERAL_VALUE: RuleMeta = syntax_check(
    "invalid-literal-value",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-literal-value.md",
);

/// A value that must be a literal or a single `${{ }}` expression is other
/// text.
pub static EXPECTED_SINGLE_EXPRESSION: RuleMeta = syntax_check(
    "expected-single-expression",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/expected-single-expression.md",
);

/// The workflow has more YAML nodes, with its aliases expanded, than GitHub
/// reads. GitHub rejects it with `Maximum YAML nodes exceeded`.
pub static TOO_MANY_YAML_NODES: RuleMeta = syntax_check(
    "too-many-yaml-nodes",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/too-many-yaml-nodes.md",
);

/// A `workflow_dispatch` input has a `type` that is not `string`, `number`,
/// `boolean`, `choice`, or `environment`.
pub static INVALID_DISPATCH_INPUT_TYPE: RuleMeta = syntax_check(
    "invalid-dispatch-input-type",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-dispatch-input-type.md",
);

/// A `workflow_call` input has a `type` that is not `boolean`, `number`, or
/// `string`.
pub static INVALID_WORKFLOW_CALL_INPUT_TYPE: RuleMeta = syntax_check(
    "invalid-workflow-call-input-type",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-workflow-call-input-type.md",
);

/// `schedule` is named in a string or a sequence item of `on`, so it has no
/// `cron` entries.
pub static SCHEDULE_WITHOUT_CONFIG: RuleMeta = syntax_check(
    "schedule-without-config",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/schedule-without-config.md",
);

/// A job that calls a reusable workflow with `uses` has a key that only a
/// job with steps may have, such as `runs-on` or `services`.
pub static INVALID_REUSABLE_CALL_KEY: RuleMeta = syntax_check(
    "invalid-reusable-call-key",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-reusable-call-key.md",
);

/// A job without `uses` has `with` or `secrets`, which only a job that
/// calls a reusable workflow may have.
pub static REUSABLE_CALL_KEY_WITHOUT_USES: RuleMeta = syntax_check(
    "reusable-call-key-without-uses",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/reusable-call-key-without-uses.md",
);

/// A literal `timeout-minutes` or `max-parallel` is zero or less.
pub static NON_POSITIVE_NUMBER: RuleMeta = syntax_check(
    "non-positive-number",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/non-positive-number.md",
);

/// `credentials` of a container or service is not one expression and lacks
/// `username` or `password`. The parser drops the credentials.
pub static INCOMPLETE_CREDENTIALS: RuleMeta = syntax_check(
    "incomplete-credentials",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/incomplete-credentials.md",
);

/// A step has neither `run` nor `uses`.
pub static STEP_WITHOUT_RUN_OR_USES: RuleMeta = syntax_check(
    "step-without-run-or-uses",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/step-without-run-or-uses.md",
);

/// Every rule in this module, in the order of their IDs.
pub static ALL: [&RuleMeta; 19] = [
    &DUPLICATE_KEY,
    &EMPTY_SECTION,
    &EMPTY_STRING,
    &EMPTY_WORKFLOW,
    &EXPECTED_SINGLE_EXPRESSION,
    &INCOMPLETE_CREDENTIALS,
    &INVALID_DISPATCH_INPUT_TYPE,
    &INVALID_LITERAL_VALUE,
    &INVALID_REUSABLE_CALL_KEY,
    &INVALID_WORKFLOW_CALL_INPUT_TYPE,
    &MERGE_KEY_UNSUPPORTED,
    &MISSING_REQUIRED_KEY,
    &NON_POSITIVE_NUMBER,
    &REUSABLE_CALL_KEY_WITHOUT_USES,
    &SCHEDULE_WITHOUT_CONFIG,
    &STEP_WITHOUT_RUN_OR_USES,
    &TOO_MANY_YAML_NODES,
    &UNEXPECTED_KEY,
    &UNEXPECTED_NODE_KIND,
];

/// A parse-stage rule of kind `syntax-check`. `docs_url` is the page that
/// M4.x adds (ADR 0024).
const fn syntax_check(id: &'static str, docs_url: &'static str) -> RuleMeta {
    RuleMeta {
        id,
        kind: "syntax-check",
        category: Category::Correctness,
        default_severity: Severity::Error,
        docs_url,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_rule_links_the_page_named_after_its_id() {
        for rule in ALL {
            assert_eq!(
                rule.docs_url,
                format!(
                    "https://github.com/stokehold/workflint/blob/main/docs/rules/{}.md",
                    rule.id
                )
            );
            assert_eq!(rule.kind, "syntax-check");
        }
    }

    #[test]
    fn the_list_is_sorted_by_id() {
        let ids: Vec<_> = ALL.iter().map(|rule| rule.id).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted);
    }
}
