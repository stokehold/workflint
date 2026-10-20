//! The types of the global contexts.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `BuiltinGlobalVariableTypes` in
//! `expr_sema.go`, lines 261-357. Until M7 replaces it with a generated file (ADR 0011), keep
//! it equal to that table plus `github.artifacts`, `github.artifacts_list`,
//! `job.workflow_file_path`, `job.workflow_ref`, `job.workflow_repository`, and
//! `job.workflow_sha`, which workflint adds (ADR 0033,
//! `docs/differences.md#platform-data-updates`). The test
//! `global_contexts_are_the_reference_table_and_the_added_properties` compares them.
//!
//! The added properties and their types come from
//! <https://docs.github.com/en/actions/reference/workflows-and-actions/contexts>, checked on
//! 2026-10-20.
//!
//! The reference has no `jobs` context here. It exists only in `on.workflow_call.outputs`,
//! where the checker adds it.

use crate::types::{Property as P, TypeDesc as T};

/// A context that every expression can read, such as `github` or `env`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Context {
    /// The name, in lower case.
    pub name: &'static str,
    /// The type of the context's value.
    pub ty: T,
}

const fn context(name: &'static str, ty: T) -> Context {
    Context { name, ty }
}

/// The global contexts, sorted by name.
///
/// `matrix`, `needs`, `steps`, and `inputs` are empty strict objects. The expression checker
/// replaces them with types built from the workflow.
pub static GLOBAL_CONTEXTS: &[Context] = &[
    context("env", T::map(&T::String)),
    context(
        "github",
        T::strict(&[
            P::new("action", T::String),
            // Composite actions only.
            P::new("action_path", T::String),
            P::new("action_ref", T::String),
            P::new("action_repository", T::String),
            // Composite actions only.
            P::new("action_status", T::String),
            P::new("actor", T::String),
            P::new("actor_id", T::String),
            P::new("api_url", T::String),
            // Undocumented.
            P::new("artifact_cache_size_limit", T::Number),
            P::new("artifacts", T::String),
            P::new("artifacts_list", T::String),
            P::new("base_ref", T::String),
            P::new("env", T::String),
            // The reference does not type the event payload.
            P::new("event", T::loose(&[])),
            P::new("event_name", T::String),
            P::new("event_path", T::String),
            P::new("graphql_url", T::String),
            P::new("head_ref", T::String),
            P::new("job", T::String),
            // Undocumented.
            P::new("output", T::String),
            P::new("path", T::String),
            P::new("ref", T::String),
            P::new("ref_name", T::String),
            P::new("ref_protected", T::Bool),
            P::new("ref_type", T::String),
            P::new("repository", T::String),
            P::new("repository_id", T::String),
            P::new("repository_owner", T::String),
            P::new("repository_owner_id", T::String),
            // Undocumented.
            P::new("repository_visibility", T::String),
            // `repositoryUrl` in GitHub's documentation.
            P::new("repositoryurl", T::String),
            P::new("retention_days", T::Number),
            P::new("run_attempt", T::String),
            P::new("run_id", T::String),
            P::new("run_number", T::String),
            P::new("secret_source", T::String),
            P::new("server_url", T::String),
            P::new("sha", T::String),
            // Undocumented.
            P::new("state", T::String),
            // Undocumented.
            P::new("step_summary", T::String),
            P::new("token", T::String),
            P::new("triggering_actor", T::String),
            P::new("workflow", T::String),
            P::new("workflow_ref", T::String),
            P::new("workflow_sha", T::String),
            P::new("workspace", T::String),
        ]),
    ),
    context("inputs", T::strict(&[])),
    context(
        "job",
        T::strict(&[
            P::new("check_run_id", T::Number),
            P::new(
                "container",
                T::strict(&[P::new("id", T::String), P::new("network", T::String)]),
            ),
            P::new(
                "services",
                T::map(&T::strict(&[
                    P::new("id", T::String),
                    P::new("network", T::String),
                    P::new("ports", T::map(&T::String)),
                ])),
            ),
            P::new("status", T::String),
            P::new("workflow_file_path", T::String),
            P::new("workflow_ref", T::String),
            P::new("workflow_repository", T::String),
            P::new("workflow_sha", T::String),
        ]),
    ),
    context("matrix", T::strict(&[])),
    context("needs", T::strict(&[])),
    context(
        "runner",
        T::strict(&[
            P::new("arch", T::String),
            P::new("debug", T::String),
            P::new("environment", T::String),
            P::new("name", T::String),
            P::new("os", T::String),
            P::new("temp", T::String),
            P::new("tool_cache", T::String),
        ]),
    ),
    context("secrets", T::map(&T::String)),
    context("steps", T::strict(&[])),
    context(
        "strategy",
        T::loose(&[
            P::new("fail-fast", T::Bool),
            P::new("job-index", T::Number),
            P::new("job-total", T::Number),
            P::new("max-parallel", T::Number),
        ]),
    ),
    context("vars", T::map(&T::String)),
];

/// The secrets that every workflow can read besides the ones it declares, sorted and in lower
/// case. GitHub passes `GITHUB_TOKEN` to every run, and the caller of a workflow passes
/// `ACTIONS_STEP_DEBUG` and `ACTIONS_RUNNER_DEBUG`. Each has type `string`.
///
/// Ported from the reference's `UpdateSecrets` in `expr_sema.go`. The test
/// `built_in_secrets_match_the_reference_update_secrets` compares them.
pub static BUILT_IN_SECRETS: &[&str] =
    &["actions_runner_debug", "actions_step_debug", "github_token"];

/// Returns the type of the global context `name`, which must be lower case.
#[must_use]
pub fn global_context(name: &str) -> Option<&'static T> {
    let index = GLOBAL_CONTEXTS
        .binary_search_by(|context| context.name.cmp(name))
        .ok()?;
    GLOBAL_CONTEXTS.get(index).map(|context| &context.ty)
}
