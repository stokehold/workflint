//! GitHub platform data for workflint (ADR 0011).
//!
//! [`TypeDesc`] describes the type of an expression value without depending on
//! `workflint-expr`, which converts it to its own type. [`GLOBAL_CONTEXTS`] lists the type of
//! each context that every expression can read, and [`global_context`] looks one up by name.
//! [`BUILTIN_FUNCTIONS`] lists the signatures of the built-in functions, and
//! [`builtin_function`] looks one up by name. [`WORKFLOW_KEYS`] lists the contexts and special
//! functions that expressions at each workflow key may use, and [`workflow_key`] looks one up.
//! [`UNTRUSTED_INPUTS`] holds the context properties whose values someone outside the repository
//! can set, such as `github.event.issue.title`, and [`SAFE_FUNCTIONS`] the functions whose
//! arguments may hold them in a script. [`BUILT_IN_SECRETS`] lists the secrets that every
//! workflow can read, and [`RESERVED_CONFIG_VARIABLE_PREFIX`] and
//! [`is_config_variable_name_char`] the naming rules of configuration variables.
//!
//! The tables that the rules read: [`WEBHOOK_EVENTS`] and [`filter_events`] for the events of
//! `on:`, with [`MAX_WORKFLOW_DISPATCH_INPUTS`], [`MIN_SCHEDULE_INTERVAL_SECONDS`], and
//! [`TIME_ZONES`]. [`GITHUB_HOSTED_RUNNER_LABELS`], [`runner_label_os_compat`],
//! [`RUNNER_PROVIDERS`], and [`resolve_runner`] for `runs-on`, [`PERMISSION_SCOPES`] for `permissions:`, [`shell_names`] for `shell:`, and
//! [`DEPRECATED_COMMANDS`] for `run:`. [`RUNS_USING_VALUES`], [`BRANDING_COLORS`],
//! [`BRANDING_ICONS`], and [`DOCKER_REGISTRY_PREFIXES`] for action metadata.
//! [`POPULAR_ACTIONS`] and [`popular_action`] for the inputs and outputs of popular actions,
//! and [`OUTDATED_POPULAR_ACTION_SPECS`] and [`is_outdated_popular_action`] for those whose
//! runner GitHub no longer has.
//!
//! Every table is `static` data, so reading one allocates nothing. The crate reads no files,
//! environment variables, or clocks: it is `no_std`.
//!
//! This is an internal crate with no stability promise.

#![no_std]

mod availability;
mod branding;
mod config_variables;
mod deprecated_commands;
mod docker_registries;
mod event_filters;
mod event_limits;
mod functions;
mod global_contexts;
mod go_case;
mod permissions;
mod popular_actions;
mod runner_labels;
mod runner_providers;
mod runs_using;
mod shells;
mod time_zones;
mod types;
mod untrusted_inputs;
mod webhooks;

pub use availability::{
    Availability, SPECIAL_FUNCTIONS, WORKFLOW_KEYS, WorkflowKey, special_function_keys,
    workflow_key,
};
pub use branding::{BRANDING_COLORS, BRANDING_ICONS, is_branding_color, is_branding_icon};
pub use config_variables::{RESERVED_CONFIG_VARIABLE_PREFIX, is_config_variable_name_char};
pub use deprecated_commands::{DEPRECATED_COMMANDS, DeprecatedCommand, deprecated_command};
pub use docker_registries::{DOCKER_REGISTRY_PREFIXES, is_on_docker_registry};
pub use event_filters::{EventFilter, filter_events};
pub use event_limits::{MAX_WORKFLOW_DISPATCH_INPUTS, MIN_SCHEDULE_INTERVAL_SECONDS};
pub use functions::{BUILTIN_FUNCTIONS, Function, Signature, builtin_function};
pub use global_contexts::{BUILT_IN_SECRETS, Context, GLOBAL_CONTEXTS, global_context};
pub use permissions::{
    PERMISSION_SCOPES, PERMISSIONS_FOR_ALL_SCOPES, PermissionScope, permission_scope,
};
pub use popular_actions::{
    ActionInput, ActionOutput, OUTDATED_POPULAR_ACTION_SPECS, POPULAR_ACTIONS, PopularAction,
    is_outdated_popular_action, popular_action,
};
pub use runner_labels::{
    GITHUB_HOSTED_RUNNER_LABELS, RUNNER_LABEL_OS_COMPATS, RunnerLabelOsCompat, RunnerOsCompat,
    SELF_HOSTED_RUNNER_OS_LABELS, SELF_HOSTED_RUNNER_OTHER_LABELS, is_self_hosted_other_label,
    runner_label_os_compat,
};
pub use runner_providers::{
    ProviderLabel, RUNNER_PROVIDERS, ResolvedRunner, RunnerKind, RunnerOs, RunnerProvider,
    resolve_runner, runner_label_os, runner_provider,
};
pub use runs_using::{RUNS_USING_VALUES, is_runs_using_value};
pub use shells::{ShellPlatform, shell_names};
pub use time_zones::{TIME_ZONES, TZDATA_VERSION, is_time_zone};
pub use types::{ObjectDesc, Property, Rest, TypeDesc};
pub use untrusted_inputs::{SAFE_FUNCTIONS, UNTRUSTED_INPUTS, UntrustedInput};
pub use webhooks::{ActivityTypes, WEBHOOK_EVENTS, WebhookEvent, webhook_event};
