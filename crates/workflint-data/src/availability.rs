//! The contexts and special functions that each workflow key allows.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `WorkflowKeyAvailability`,
//! `SpecialFunctionNames`, and `AllContexts` in `availability.go`, which the reference generates
//! from the context availability table in GitHub's documentation. Until M7 replaces it with a
//! generated file (ADR 0011), keep it equal to that file. The tests in `tests/availability.rs`
//! compare them.

/// The contexts and special functions that expressions at one workflow key may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Availability {
    /// The contexts, in lower case and sorted. An empty list allows no context.
    pub contexts: &'static [&'static str],
    /// The special functions from [`SPECIAL_FUNCTIONS`], in lower case and sorted. An empty
    /// list allows none of them.
    pub special_functions: &'static [&'static str],
}

impl Availability {
    /// Allows no context and no special function, as at a workflow key that the table does not
    /// list.
    pub const NONE: Self = Self {
        contexts: &[],
        special_functions: &[],
    };
}

/// A workflow key, such as `jobs.<job_id>.steps.if`, and what its expressions may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkflowKey {
    /// The key as GitHub's documentation writes it, with placeholders such as `<job_id>`.
    pub name: &'static str,
    /// What expressions at the key may use.
    pub availability: Availability,
}

/// The functions that only some workflow keys allow, in lower case and sorted.
pub static SPECIAL_FUNCTIONS: &[&str] = &["always", "cancelled", "failure", "hashfiles", "success"];

const fn key(name: &'static str, availability: Availability) -> WorkflowKey {
    WorkflowKey { name, availability }
}

const fn allows(
    contexts: &'static [&'static str],
    special_functions: &'static [&'static str],
) -> Availability {
    Availability {
        contexts,
        special_functions,
    }
}

// Each constant is one `case` of the reference's `WorkflowKeyAvailability`.
const JOB_OUTPUTS: Availability = allows(
    &[
        "env", "github", "inputs", "job", "matrix", "needs", "runner", "secrets", "steps",
        "strategy", "vars",
    ],
    &[],
);
const STEP: Availability = allows(
    &[
        "env", "github", "inputs", "job", "matrix", "needs", "runner", "secrets", "steps",
        "strategy", "vars",
    ],
    &["hashfiles"],
);
const CONTAINER_ENV: Availability = allows(
    &[
        "env", "github", "inputs", "job", "matrix", "needs", "runner", "secrets", "strategy",
        "vars",
    ],
    &[],
);
const ENVIRONMENT_URL: Availability = allows(
    &[
        "env", "github", "inputs", "job", "matrix", "needs", "runner", "steps", "strategy", "vars",
    ],
    &[],
);
const STEP_IF: Availability = allows(
    &[
        "env", "github", "inputs", "job", "matrix", "needs", "runner", "steps", "strategy", "vars",
    ],
    &["always", "cancelled", "failure", "hashfiles", "success"],
);
const CREDENTIALS: Availability = allows(
    &[
        "env", "github", "inputs", "matrix", "needs", "secrets", "strategy", "vars",
    ],
    &[],
);
const JOB_DEFAULTS_RUN: Availability = allows(
    &[
        "env", "github", "inputs", "matrix", "needs", "strategy", "vars",
    ],
    &[],
);
const WORKFLOW_CALL_OUTPUT: Availability = allows(&["github", "inputs", "jobs", "vars"], &[]);
const JOB_ENV: Availability = allows(
    &[
        "github", "inputs", "matrix", "needs", "secrets", "strategy", "vars",
    ],
    &[],
);
const JOB: Availability = allows(
    &["github", "inputs", "matrix", "needs", "strategy", "vars"],
    &[],
);
const JOB_STRATEGY: Availability = allows(&["github", "inputs", "needs", "vars"], &[]);
const JOB_IF: Availability = allows(
    &["github", "inputs", "needs", "vars"],
    &["always", "cancelled", "failure", "success"],
);
const WORKFLOW_ENV: Availability = allows(&["github", "inputs", "secrets", "vars"], &[]);
const WORKFLOW: Availability = allows(&["github", "inputs", "vars"], &[]);
// The reference adds this key itself, because GitHub's table lacks it.
const SNAPSHOT_IF: Availability = allows(
    &["github", "inputs", "matrix", "needs", "strategy", "vars"],
    &[],
);

/// The workflow keys that allow some context, sorted by name. At any other key, such as
/// `jobs.<job_id>.steps.shell`, expressions may use no context and no special function.
pub static WORKFLOW_KEYS: &[WorkflowKey] = &[
    key("concurrency", WORKFLOW),
    key("env", WORKFLOW_ENV),
    key("jobs.<job_id>.concurrency", JOB),
    key("jobs.<job_id>.container", JOB),
    key("jobs.<job_id>.container.credentials", CREDENTIALS),
    key("jobs.<job_id>.container.env.<env_id>", CONTAINER_ENV),
    key("jobs.<job_id>.container.image", JOB),
    key("jobs.<job_id>.continue-on-error", JOB),
    key("jobs.<job_id>.defaults.run", JOB_DEFAULTS_RUN),
    key("jobs.<job_id>.env", JOB_ENV),
    key("jobs.<job_id>.environment", JOB),
    key("jobs.<job_id>.environment.url", ENVIRONMENT_URL),
    key("jobs.<job_id>.if", JOB_IF),
    key("jobs.<job_id>.name", JOB),
    key("jobs.<job_id>.outputs.<output_id>", JOB_OUTPUTS),
    key("jobs.<job_id>.runs-on", JOB),
    key("jobs.<job_id>.secrets.<secrets_id>", JOB_ENV),
    key("jobs.<job_id>.services", JOB),
    key(
        "jobs.<job_id>.services.<service_id>.credentials",
        CREDENTIALS,
    ),
    key(
        "jobs.<job_id>.services.<service_id>.env.<env_id>",
        CONTAINER_ENV,
    ),
    key("jobs.<job_id>.snapshot.if", SNAPSHOT_IF),
    key("jobs.<job_id>.steps.continue-on-error", STEP),
    key("jobs.<job_id>.steps.env", STEP),
    key("jobs.<job_id>.steps.if", STEP_IF),
    key("jobs.<job_id>.steps.name", STEP),
    key("jobs.<job_id>.steps.run", STEP),
    key("jobs.<job_id>.steps.timeout-minutes", STEP),
    key("jobs.<job_id>.steps.with", STEP),
    key("jobs.<job_id>.steps.working-directory", STEP),
    key("jobs.<job_id>.strategy", JOB_STRATEGY),
    key("jobs.<job_id>.timeout-minutes", JOB),
    key("jobs.<job_id>.with.<with_id>", JOB),
    key("on.workflow_call.inputs.<inputs_id>.default", WORKFLOW),
    key(
        "on.workflow_call.outputs.<output_id>.value",
        WORKFLOW_CALL_OUTPUT,
    ),
    key("run-name", WORKFLOW),
];

/// Returns what expressions at the workflow key `name` may use, or `None` if the table does
/// not list the key. Names are compared exactly.
#[must_use]
pub fn workflow_key(name: &str) -> Option<&'static Availability> {
    let index = WORKFLOW_KEYS
        .binary_search_by(|key| key.name.cmp(name))
        .ok()?;
    WORKFLOW_KEYS.get(index).map(|key| &key.availability)
}

/// Returns the workflow keys that allow the special function `name`, which must be lower case,
/// in sorted order. Returns `None` if the function is not special, so every key allows it.
#[must_use]
pub fn special_function_keys(name: &str) -> Option<impl Iterator<Item = &'static str> + use<>> {
    let name: &'static str = SPECIAL_FUNCTIONS.iter().find(|special| **special == name)?;
    let keys = WORKFLOW_KEYS
        .iter()
        .filter(move |key| key.availability.special_functions.contains(&name))
        .map(|key| key.name);
    Some(keys)
}
