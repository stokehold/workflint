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
//! [`tool`] runs external tools such as shellcheck with a timeout and a limit on processes, and
//! [`Tools`] runs shellcheck on the scripts that the rules collect, once per script and run.
//!
//! [`Linter::discover`] returns the workflow files of a run without file arguments, and
//! [`Input::stdin`] reads a workflow from stdin.
//!
//! This is an internal crate with no stability promise.

mod cache;
mod discover;
mod ignore;
mod linter;
pub mod path;
mod project;
pub mod registry;
pub mod rules;
pub mod tool;
mod tools;

pub use cache::{CachePoisoned, FileMetadata, FileRecord, LocalCache, Prior};
pub use discover::DiscoveryError;
pub use ignore::{IgnorePattern, PatternError};
pub use linter::{
    Check, FILE_SPAN, Input, Linted, Linter, ReadError, STDIN, StdinError, has_internal_error,
};
pub use project::ProjectRoots;
pub use tools::Tools;
