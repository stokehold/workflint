//! GitHub platform data for workflint (ADR 0011).
//!
//! [`TypeDesc`] describes the type of an expression value without depending on
//! `workflint-expr`, which converts it to its own type. [`GLOBAL_CONTEXTS`] lists the type of
//! each context that every expression can read, and [`global_context`] looks one up by name.
//!
//! Every table is `static` data, so reading one allocates nothing. The crate reads no files,
//! environment variables, or clocks: it is `no_std`.
//!
//! This is an internal crate with no stability promise.

#![no_std]

mod global_contexts;
mod types;

pub use global_contexts::{Context, GLOBAL_CONTEXTS, global_context};
pub use types::{ObjectDesc, Property, Rest, TypeDesc};
