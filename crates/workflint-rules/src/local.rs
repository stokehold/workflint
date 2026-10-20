//! The metadata of a project's local actions and local reusable workflows, which rules look up
//! by `uses:` value.
//!
//! [`LocalMetadata`] is the interface that the rule [`Context`](crate::Context) holds.
//! `workflint-linter` implements it with a per-run cache that reads the files of the project.
//! [`NoProject`] finds nothing, as the reference does for a file outside any project, and
//! [`MemoryProject`] holds the files in memory for the tests of rules and rule pages.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use workflint_diagnostics::quote::quote;
use workflint_diagnostics::{FileId, SourceFile};
use workflint_syntax::{ActionMetadata, ReusableWorkflowMetadata, contains_expression};

/// The local actions and local reusable workflows of the project of the file being linted.
///
/// Each lookup takes the `uses:` value as written, which the reference uses as the cache key.
/// Only a value that starts with `./` is local. The file behind a value is read and decoded once
/// per run, and the first lookup of a value in the run is the one that reports its error.
pub trait LocalMetadata: fmt::Debug {
    /// Looks up the local action at `spec`, the reference's `LocalActionsCache.FindMetadata`.
    ///
    /// The metadata is in `action.yaml` or, if that cannot be read, `action.yml` in the
    /// directory `spec` names. An action without either file is [`Lookup::None`], because the
    /// reference does not report it: the directory is often a submodule or is checked out while
    /// the workflow runs.
    fn action(&self, spec: &str) -> Lookup<LocalAction>;

    /// Looks up the local reusable workflow at `spec`, the reference's
    /// `LocalReusableWorkflowCache.FindMetadata`.
    ///
    /// A `spec` that contains an expression is [`Lookup::None`]. A missing file is an error.
    fn reusable_workflow(&self, spec: &str) -> Lookup<ReusableWorkflowMetadata>;

    /// Registers `metadata`, from the `on.workflow_call` of the workflow being linted, under the
    /// workflow's path from the project root, the reference's
    /// `LocalReusableWorkflowCache.WriteWorkflowCallEvent`.
    ///
    /// A later lookup of that path in the run finds `metadata` without reading the file. Nothing
    /// changes if the path was already registered or looked up.
    fn register_workflow_call(&self, metadata: ReusableWorkflowMetadata);

    /// Records that `spec`, a `uses:` value that starts with `./`, is not a valid reference to
    /// a local reusable workflow, which the rule that found it has reported. Later lookups of
    /// `spec` in the run return [`Lookup::None`] without reading a file, as in the reference's
    /// `RuleWorkflowCall.VisitJobPre`. Nothing changes if `spec` was already looked up.
    fn record_invalid_workflow(&self, spec: &str);

    /// Returns whether the file at the relative path `file`, written with `/` separators, exists
    /// in the directory of `action`. A file that exists but cannot be inspected counts as
    /// existing, as in the reference.
    fn action_file_exists(&self, action: &LocalAction, file: &str) -> bool;
}

/// What a lookup in [`LocalMetadata`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup<T> {
    /// There is nothing to check against: there is no project, `spec` is not local, the action
    /// does not exist, or an earlier lookup of `spec` returned [`Lookup::Error`].
    None,
    /// The decoded metadata.
    Found {
        /// The metadata, shared by every lookup of the same `spec`.
        metadata: Arc<T>,
        /// Whether this is the first lookup of `spec` in the run. The checks of the metadata
        /// itself run only then, so their diagnostics are reported once.
        first: bool,
    },
    /// The file could not be read or decoded. Only the first lookup of `spec` in the run returns
    /// the error. Later lookups return [`Lookup::None`].
    Error(LocalMetadataError),
}

/// A local action: its decoded metadata and where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalAction {
    /// The decoded `action.yaml` or `action.yml`.
    pub metadata: ActionMetadata,
    /// The directory of the action: the project root joined with `spec`, as Go's
    /// `filepath.Join` joins them.
    pub dir: PathBuf,
    /// The path of the metadata file, `dir` joined with `action.yaml` or `action.yml`.
    pub file: PathBuf,
}

/// Why a local action or a local reusable workflow has no metadata. It displays as the reference's
/// error message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalMetadataError {
    /// The metadata file of an action is not valid YAML or cannot be decoded.
    Action {
        /// The directory of the action.
        dir: PathBuf,
        /// The decoding error, as [`ActionMetadata::decode`] returns it.
        message: String,
    },
    /// The file of a reusable workflow cannot be read.
    ReadWorkflow {
        /// The `uses:` value.
        spec: String,
        /// Go's text of the read error, such as
        /// `open /repo/.github/workflows/x.yaml: no such file or directory`.
        message: String,
    },
    /// The file of a reusable workflow is not valid YAML or its `on.workflow_call` cannot be
    /// decoded.
    Workflow {
        /// The `uses:` value.
        spec: String,
        /// The decoding error, as [`ReusableWorkflowMetadata::decode`] returns it.
        message: String,
    },
}

impl fmt::Display for LocalMetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Action { dir, message } => write!(
                f,
                "could not parse action metadata in {}: {message}",
                quote(&dir.to_string_lossy())
            ),
            Self::ReadWorkflow { spec, message } => write!(
                f,
                "could not read reusable workflow file for {}: {message}",
                quote(spec)
            ),
            Self::Workflow { spec, message } => write!(
                f,
                "error while parsing reusable workflow {}: {message}",
                quote(spec)
            ),
        }
    }
}

impl std::error::Error for LocalMetadataError {}

/// Whether the reference looks up `spec` as a local action.
#[must_use]
pub fn is_local_action(spec: &str) -> bool {
    spec.starts_with("./")
}

/// Whether the reference looks up `spec` as a local reusable workflow.
#[must_use]
pub fn is_local_reusable_workflow(spec: &str) -> bool {
    spec.starts_with("./") && !contains_expression(spec)
}

/// The metadata of a file outside any project: every lookup finds nothing, as in the reference.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoProject;

impl LocalMetadata for NoProject {
    fn action(&self, _spec: &str) -> Lookup<LocalAction> {
        Lookup::None
    }

    fn reusable_workflow(&self, _spec: &str) -> Lookup<ReusableWorkflowMetadata> {
        Lookup::None
    }

    fn register_workflow_call(&self, _metadata: ReusableWorkflowMetadata) {}

    fn record_invalid_workflow(&self, _spec: &str) {}

    fn action_file_exists(&self, _action: &LocalAction, _file: &str) -> bool {
        false
    }
}

/// A project held in memory, for the tests of rules and rule pages.
///
/// Paths are relative to the project root and written with `/`, such as `.github/actions/greet`.
/// An action's [`dir`](LocalAction::dir) is its path from the root. Lookups decode the files as
/// the linter's cache does and remember which lookup of a spec came first.
///
/// ```
/// use workflint_rules::local::{LocalMetadata, Lookup, MemoryProject};
///
/// let project = MemoryProject::new()
///     .with_file(".github/actions/greet/action.yml", "name: Greet\nruns:\n  using: node24\n  main: index.js\n")
///     .with_file(".github/actions/greet/index.js", "");
/// let Lookup::Found { metadata: action, first } = project.action("./.github/actions/greet") else {
///     panic!("the action is found");
/// };
/// assert!(first);
/// assert_eq!(action.metadata.name, "Greet");
/// assert!(project.action_file_exists(&action, "index.js"));
/// assert!(matches!(project.action("./.github/actions/greet"), Lookup::Found { first: false, .. }));
/// assert_eq!(project.action("./.github/actions/missing"), Lookup::None);
/// ```
#[derive(Debug, Default)]
pub struct MemoryProject {
    files: BTreeMap<String, String>,
    linted: Option<String>,
    looked_up: Mutex<Looked>,
}

/// The specs that a [`MemoryProject`] has looked up or registered.
#[derive(Debug, Default)]
struct Looked {
    actions: BTreeSet<String>,
    workflows: BTreeMap<String, Option<Arc<ReusableWorkflowMetadata>>>,
}

impl MemoryProject {
    /// Creates an empty project.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds the file at `path` with the text `text`.
    #[must_use]
    pub fn with_file(mut self, path: &str, text: &str) -> Self {
        self.files.insert(normalize(path), text.to_owned());
        self
    }

    /// Sets the path of the workflow being linted, under which
    /// [`register_workflow_call`](LocalMetadata::register_workflow_call) registers its metadata.
    #[must_use]
    pub fn linting(mut self, path: &str) -> Self {
        self.linted = Some(format!("./{}", normalize(path)));
        self
    }

    fn looked(&self) -> std::sync::MutexGuard<'_, Looked> {
        self.looked_up
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn source(path: &str, text: &str) -> Option<SourceFile> {
        SourceFile::new(FileId::new(0), path, text).ok()
    }
}

impl LocalMetadata for MemoryProject {
    fn action(&self, spec: &str) -> Lookup<LocalAction> {
        if !is_local_action(spec) {
            return Lookup::None;
        }
        let first = self.looked().actions.insert(spec.to_owned());
        let dir = normalize(spec);
        let found = ["action.yaml", "action.yml"].into_iter().find_map(|name| {
            let path = join(&dir, name);
            self.files.get(&path).map(|text| (path, text))
        });
        let Some((path, text)) = found else {
            return Lookup::None;
        };
        let decoded = Self::source(&path, text).map(|file| ActionMetadata::decode(&file));
        match decoded {
            Some(Ok(metadata)) => Lookup::Found {
                metadata: Arc::new(LocalAction {
                    metadata,
                    dir: PathBuf::from(&dir),
                    file: PathBuf::from(path),
                }),
                first,
            },
            Some(Err(err)) if first => Lookup::Error(LocalMetadataError::Action {
                dir: PathBuf::from(dir),
                message: err.message().to_owned(),
            }),
            _ => Lookup::None,
        }
    }

    fn reusable_workflow(&self, spec: &str) -> Lookup<ReusableWorkflowMetadata> {
        if !is_local_reusable_workflow(spec) {
            return Lookup::None;
        }
        let mut looked = self.looked();
        if let Some(entry) = looked.workflows.get(spec) {
            return entry
                .as_ref()
                .map_or(Lookup::None, |metadata| Lookup::Found {
                    metadata: Arc::clone(metadata),
                    first: false,
                });
        }
        let path = normalize(spec);
        let Some(text) = self.files.get(&path) else {
            looked.workflows.insert(spec.to_owned(), None);
            return Lookup::Error(LocalMetadataError::ReadWorkflow {
                spec: spec.to_owned(),
                message: format!("open {path}: no such file or directory"),
            });
        };
        let decoded = Self::source(&path, text).map(|file| ReusableWorkflowMetadata::decode(&file));
        match decoded {
            Some(Ok(metadata)) => {
                let metadata = Arc::new(metadata);
                looked
                    .workflows
                    .insert(spec.to_owned(), Some(Arc::clone(&metadata)));
                Lookup::Found {
                    metadata,
                    first: true,
                }
            }
            Some(Err(err)) => {
                looked.workflows.insert(spec.to_owned(), None);
                Lookup::Error(LocalMetadataError::Workflow {
                    spec: spec.to_owned(),
                    message: err.message().to_owned(),
                })
            }
            None => {
                looked.workflows.insert(spec.to_owned(), None);
                Lookup::None
            }
        }
    }

    fn register_workflow_call(&self, metadata: ReusableWorkflowMetadata) {
        if let Some(spec) = &self.linted {
            self.looked()
                .workflows
                .entry(spec.clone())
                .or_insert_with(|| Some(Arc::new(metadata)));
        }
    }

    fn record_invalid_workflow(&self, spec: &str) {
        self.looked()
            .workflows
            .entry(spec.to_owned())
            .or_insert(None);
    }

    fn action_file_exists(&self, action: &LocalAction, file: &str) -> bool {
        let path = join(&action.dir.to_string_lossy(), file);
        if path.is_empty() || self.files.contains_key(&path) {
            return true;
        }
        // A path that some file is in is a directory, which exists as the cache sees it.
        let dir = format!("{path}/");
        self.files
            .range(dir.clone()..)
            .next()
            .is_some_and(|(name, _)| name.starts_with(&dir))
    }
}

/// Cleans a `/`-separated relative path as Go's `path.Clean` does, without the `.` it returns for
/// an empty path: it removes empty and `.` names, and each `..` with the name before it.
fn normalize(path: &str) -> String {
    let mut names: Vec<&str> = Vec::new();
    for name in path.split('/') {
        match name {
            "" | "." => {}
            ".." if names.last().is_some_and(|last| *last != "..") => {
                names.pop();
            }
            _ => names.push(name),
        }
    }
    names.join("/")
}

/// Joins two `/`-separated relative paths.
fn join(dir: &str, name: &str) -> String {
    normalize(&format!("{dir}/{name}"))
}
