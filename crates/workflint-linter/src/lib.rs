//! The part of workflint that reads the file system around the linted files.
//!
//! [`ProjectRoots`] finds the project of a file as the reference does, and a [`LocalCache`]
//! per project reads and decodes its local actions and local reusable workflows once per run.
//! The rules see the cache through [`LocalCache::for_file`], which implements
//! [`LocalMetadata`](workflint_rules::local::LocalMetadata), and [`LocalCache::commit`] makes
//! the answers of files linted in parallel the same as in file order. [`path`] computes paths as
//! Go's `path/filepath` does.
//!
//! M5 adds file discovery, the config, and the rest of the parallel driver.
//!
//! This is an internal crate with no stability promise.

mod cache;
pub mod path;
mod project;

pub use cache::{FileMetadata, FileRecord, LocalCache, Prior};
pub use project::ProjectRoots;
