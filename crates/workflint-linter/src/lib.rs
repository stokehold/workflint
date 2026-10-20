//! The part of workflint that reads the file system around the linted files.
//!
//! [`ProjectRoots`] finds the project of a file as the reference does, and a [`LocalCache`]
//! per project reads and decodes its local actions and local reusable workflows once per run.
//! The rules see the cache through [`LocalCache::for_file`], which implements
//! [`LocalMetadata`](workflint_rules::local::LocalMetadata), and [`LocalCache::commit`] makes
//! the answers of files linted in parallel the same as in file order. [`path`] computes paths as
//! Go's `path/filepath` does.
//!
//! The [`Linter`] is the parallel driver. It takes the inputs of a run, lints them in parallel
//! with the cache of each project, and returns each file's diagnostics in input order. A panic in
//! the lint of one file becomes an [`internal-error`](rules::INTERNAL_ERROR) diagnostic on that
//! file. The [`registry`] lists every rule, with the rules of [`rules`] that the driver reports.
//!
//! M5 adds file discovery and the config.
//!
//! This is an internal crate with no stability promise.

mod cache;
mod linter;
pub mod path;
mod project;
pub mod registry;
pub mod rules;

pub use cache::{CachePoisoned, FileMetadata, FileRecord, LocalCache, Prior};
pub use linter::{Check, Input, Linted, Linter, ReadError, has_internal_error};
pub use project::ProjectRoots;
