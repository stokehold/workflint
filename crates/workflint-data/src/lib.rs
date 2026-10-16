//! GitHub platform data for workflint (ADR 0011).
//!
//! [`TypeDesc`] describes the type of an expression value without depending on
//! `workflint-expr`, which converts it to its own type. [`GLOBAL_CONTEXTS`] lists the type of
//! each context that every expression can read, and [`global_context`] looks one up by name.
//! [`BUILTIN_FUNCTIONS`] lists the signatures of the built-in functions, and
//! [`builtin_function`] looks one up by name. [`UNTRUSTED_INPUTS`] holds the context
//! properties whose values someone outside the repository can set, such as
//! `github.event.issue.title`.
//!
//! Every table is `static` data, so reading one allocates nothing. The crate reads no files,
//! environment variables, or clocks: it is `no_std`.
//!
//! This is an internal crate with no stability promise.

#![no_std]

mod functions;
mod global_contexts;
mod types;
mod untrusted_inputs;

pub use functions::{BUILTIN_FUNCTIONS, Function, Signature, builtin_function};
pub use global_contexts::{Context, GLOBAL_CONTEXTS, global_context};
pub use types::{ObjectDesc, Property, Rest, TypeDesc};
pub use untrusted_inputs::{UNTRUSTED_INPUTS, UntrustedInput};
