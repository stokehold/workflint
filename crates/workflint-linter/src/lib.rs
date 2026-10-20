//! The part of workflint that reads the file system around the linted files.
//!
//! [`ProjectRoots`] finds the project of a file as the reference does, and a [`LocalCache`]
//! per project reads and decodes its local actions and local reusable workflows once per run.
//! The rules see the cache through [`LocalCache::for_file`], which implements
//! [`LocalMetadata`](workflint_rules::local::LocalMetadata). [`path`] computes paths as Go's
//! `path/filepath` does.
//!
//! M5 adds file discovery, the config, and the parallel driver.
//!
//! This is an internal crate with no stability promise.

mod cache;
pub mod path;
mod project;

pub use cache::{FileMetadata, LocalCache};
pub use project::ProjectRoots;
