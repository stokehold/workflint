//! The typed AST of a workflow and the parser that builds it.
//!
//! This is an internal crate with no stability promise. A [`Workflow`] holds
//! every section, job, step, and event that the reference's parser builds.
//! [`parse`] fills it from a [`workflint_yaml::Document`] and reports the
//! reference's `syntax-check` errors as diagnostics of the rules in
//! [`rules`]. The rules of `workflint-rules` read the AST.
//!
//! The AST borrows from the document. Every key and every string value is a
//! [`Str`], which points at its YAML node, so it has the node's span, tag,
//! style, and [source map](Str::source_map).
//!
//! Three types describe what a section holds under a key:
//!
//! - [`Field`]: a value that cannot be an expression, such as `name` or
//!   `steps`. It is absent, invalid, or valid.
//! - [`Value`]: a value that may be a literal or a single `${{ }}`
//!   expression, such as `continue-on-error` or `env`. It is absent,
//!   invalid, a literal, or an [`Expr`].
//! - [`Keyed`]: a collection keyed by case-insensitive names, such as jobs
//!   or `env`, in source order.
//!
//! "Invalid" means the key is written but its value is not usable. The
//! parser has reported it, so rules skip it instead of reporting it again.
//!
//! ```
//! use workflint_diagnostics::{FileId, SourceFile};
//! use workflint_syntax::{Field, Keyed, KeyedEntry, Str};
//! use workflint_yaml::{NodeKind, parse};
//!
//! let file = SourceFile::new(FileId::new(0), "ci.yml", "GITHUB_TOKEN: x\n").unwrap();
//! let document = parse(&file).document.unwrap();
//! let root = document.node(document.root().unwrap()).unwrap();
//! let NodeKind::Mapping(mapping) = &root.kind else { panic!() };
//! let entry = document.entries(mapping)[0];
//! let key = Str::new(document.node(entry.key).unwrap()).unwrap();
//! let value = Str::new(document.node(entry.value).unwrap()).unwrap();
//!
//! let env = Keyed::new(vec![KeyedEntry::new(key, value.span(), Some(value))]);
//! let field = Field::Valid { key: None, span: root.span, value: env };
//!
//! let token = field.value().and_then(|env| env.get("github_token")).unwrap();
//! assert_eq!(token.key.value(), "GITHUB_TOKEN");
//! ```
//!
//! # Sharing
//!
//! After alias resolution the YAML tree is a directed acyclic graph: an
//! anchored node is reachable from each of its aliases. Every collection in
//! the AST, a [`Keyed`] or an `Arc<[T]>`, is shared on clone, and every
//! other node has a bounded number of fields, so cloning a subtree is cheap.
//!
//! [`parse`] parses an anchored subtree again at each alias, as the
//! reference does, because the same mapping can mean different things in
//! different sections. It first counts the YAML nodes with aliases
//! expanded, and parses nothing if there are more than the 50,000 that
//! GitHub reads, so the work stays linear in the size of the input.
//!
//! # Positions
//!
//! A [`Str`] span starts where the reference's `posAt(n)` points. Where the
//! reference stores the position of a key instead of a value, such as the
//! `Pos` of `permissions` or the `RunPos` of a step, the field's doc
//! comment says so. The key is the `key` of the [`Field`] or [`Value`], or
//! of the [`KeyedEntry`].

mod event;
mod expression;
mod job;
mod keyed;
mod leaf;
mod matrix;
mod parser;
pub mod rules;
mod step;
mod strconv;
mod value;
mod workflow;

#[cfg(test)]
mod testing;

pub use event::{
    Event, ImageVersionEvent, RepositoryDispatchEvent, ScheduleEntry, ScheduleEvent, WebhookEvent,
    WorkflowCallEvent, WorkflowCallEventInput, WorkflowCallEventOutput, WorkflowCallEventSecret,
    WorkflowCallInputType, WorkflowDispatchEvent, WorkflowDispatchInput, WorkflowDispatchInputType,
};
pub use expression::{contains_expression, is_expression_assigned};
pub use job::{
    Container, Credentials, Environment, Job, Runner, Services, Snapshot, Strategy, WorkflowCall,
    WorkflowCallSecrets,
};
pub use keyed::{Keyed, KeyedEntry};
pub use leaf::{Expr, Str};
pub use matrix::{Matrix, MatrixCombination, MatrixRow, RawValue};
pub use parser::{Parsed, parse};
pub use step::{Exec, ExecAction, ExecRun, Step};
pub use value::{Field, Value};
pub use workflow::{Concurrency, Defaults, DefaultsRun, Env, Permissions, Workflow};
