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
//! Every table is `static` data, so reading one allocates nothing. The crate reads no files,
//! environment variables, or clocks: it is `no_std`.
//!
//! This is an internal crate with no stability promise.

#![no_std]

mod availability;
mod config_variables;
mod functions;
mod global_contexts;
mod types;
mod untrusted_inputs;

pub use availability::{
    Availability, SPECIAL_FUNCTIONS, WORKFLOW_KEYS, WorkflowKey, special_function_keys,
    workflow_key,
};
pub use config_variables::{RESERVED_CONFIG_VARIABLE_PREFIX, is_config_variable_name_char};
pub use functions::{BUILTIN_FUNCTIONS, Function, Signature, builtin_function};
pub use global_contexts::{BUILT_IN_SECRETS, Context, GLOBAL_CONTEXTS, global_context};
pub use types::{ObjectDesc, Property, Rest, TypeDesc};
pub use untrusted_inputs::{SAFE_FUNCTIONS, UNTRUSTED_INPUTS, UntrustedInput};
