//! The rules of workflint and the registry that lists them.
//!
//! A [`Rule`] implements some of the five hooks of the reference's `Pass`, and [`visit`] calls
//! them over a parsed [`Workflow`](workflint_syntax::Workflow) in the reference's order.
//! [`create_rules`] creates the rules for one file in the reference's order, and [`run`] does
//! both and returns the diagnostics. The [`registry`] lists the rule metadata of every rule,
//! including the rules that `workflint-yaml`, `workflint-syntax`, and `workflint-expr` report
//! (ADR 0024, ADR 0028). [`kind`] has the description of each actionlint kind. Rules look up the
//! project's local actions and local reusable workflows through [`local::LocalMetadata`], which
//! the [`Context`] holds, and the settings of the config file through [`RuleConfig`].
//!
//! This is an internal crate with no stability promise.

mod config;
pub mod kind;
pub mod local;
mod path_match;
pub mod registry;
mod rule;
mod rules;
mod runner;
mod visitor;

pub use config::RuleConfig;
pub use rule::{Context, Rule};
pub use rules::{CREATION_ORDER, create_rules};
pub use visitor::{check_file, run, run_in_project, run_with_config, visit};
