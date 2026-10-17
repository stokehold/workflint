//! The per-project cache of local actions and local reusable workflows, the reference's
//! `LocalActionsCache` (`action_metadata.go:195`) and `LocalReusableWorkflowCache`
//! (`reusable_workflow.go:169`).

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use workflint_diagnostics::{FileId, SourceFile};
use workflint_rules::local::{
    LocalAction, LocalMetadata, LocalMetadataError, Lookup, is_local_action,
    is_local_reusable_workflow,
};
use workflint_syntax::{ActionMetadata, MetadataError, ReusableWorkflowMetadata};

use crate::path::clean;

/// The entries of one kind, keyed by `uses:` value. An entry is `None` when the file is missing
/// or invalid, so that only its first lookup reports it.
type Entries<T> = Mutex<BTreeMap<String, Arc<OnceLock<Option<Arc<T>>>>>>;

/// The local actions and local reusable workflows of one project, read and decoded once per run.
///
/// The binary creates one cache per project root and lints each file of the project through
/// [`for_file`](Self::for_file). Each entry is computed once, even when several threads look it
/// up at the same time, and the lookup that computes it is the first. The binary lints files one
/// after another, so the first lookup is the first use in file order, then source position.
#[derive(Debug)]
pub struct LocalCache {
    root: PathBuf,
    actions: Entries<LocalAction>,
    workflows: Entries<ReusableWorkflowMetadata>,
}

impl LocalCache {
    /// Creates an empty cache for the project at `root`, an absolute and cleaned path.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            actions: Mutex::default(),
            workflows: Mutex::default(),
        }
    }

    /// Returns the project root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the view of the cache for linting the file at `path`, an absolute and cleaned
    /// path. The view registers the file's `on.workflow_call` under its path from the root.
    #[must_use]
    pub fn for_file(&self, path: &Path) -> FileMetadata<'_> {
        FileMetadata {
            cache: self,
            spec: workflow_spec(&self.root, path),
        }
    }

    /// Reads and decodes the local action at `spec`.
    fn load_action(&self, spec: &str) -> Result<Option<LocalAction>, LocalMetadataError> {
        let dir = clean(&self.root.join(spec));
        let found = ["action.yaml", "action.yml"].into_iter().find_map(|name| {
            let file = dir.join(name);
            fs::read(&file).ok().map(|bytes| (file, bytes))
        });
        // The reference does not report a missing action (#25, #40): it is often a submodule or
        // a directory that the workflow checks out while it runs.
        let Some((file, bytes)) = found else {
            return Ok(None);
        };
        match decode(&file, &bytes, ActionMetadata::decode) {
            Ok(metadata) => Ok(Some(LocalAction {
                metadata,
                dir,
                file,
            })),
            Err(message) => Err(LocalMetadataError::Action { dir, message }),
        }
    }

    /// Reads and decodes the local reusable workflow at `spec`.
    fn load_workflow(
        &self,
        spec: &str,
    ) -> Result<Option<ReusableWorkflowMetadata>, LocalMetadataError> {
        let file = clean(&self.root.join(spec));
        let bytes = fs::read(&file).map_err(|err| LocalMetadataError::ReadWorkflow {
            spec: spec.to_owned(),
            message: go_read_error(&file, &err),
        })?;
        decode(&file, &bytes, ReusableWorkflowMetadata::decode)
            .map(Some)
            .map_err(|message| LocalMetadataError::Workflow {
                spec: spec.to_owned(),
                // The reference replaces each line break with a space.
                message: message.replace('\n', " "),
            })
    }
}

/// The view of a [`LocalCache`] for linting one file.
#[derive(Debug)]
pub struct FileMetadata<'c> {
    cache: &'c LocalCache,
    /// The `uses:` value that names the linted file, or `None` if it is outside the root.
    spec: Option<String>,
}

impl LocalMetadata for FileMetadata<'_> {
    fn action(&self, spec: &str) -> Lookup<LocalAction> {
        if !is_local_action(spec) {
            return Lookup::None;
        }
        lookup(&self.cache.actions, spec, || self.cache.load_action(spec))
    }

    fn reusable_workflow(&self, spec: &str) -> Lookup<ReusableWorkflowMetadata> {
        if !is_local_reusable_workflow(spec) {
            return Lookup::None;
        }
        lookup(&self.cache.workflows, spec, || {
            self.cache.load_workflow(spec)
        })
    }

    fn register_workflow_call(&self, metadata: ReusableWorkflowMetadata) {
        if let Some(spec) = &self.spec {
            // A spec that is already set keeps its value.
            let _ = slot(&self.cache.workflows, spec).set(Some(Arc::new(metadata)));
        }
    }

    fn action_file_exists(&self, action: &LocalAction, file: &str) -> bool {
        let path = clean(&action.dir.join(from_slash(file)));
        // As in the reference, only a file that is known not to exist is missing.
        !matches!(fs::metadata(path), Err(err) if err.kind() == io::ErrorKind::NotFound)
    }
}

/// Returns the slot of `spec` in `entries`, creating it if needed.
fn slot<T>(entries: &Entries<T>, spec: &str) -> Arc<OnceLock<Option<Arc<T>>>> {
    let mut entries = entries.lock().unwrap_or_else(PoisonError::into_inner);
    Arc::clone(entries.entry(spec.to_owned()).or_default())
}

/// Looks up `spec` in `entries`, computing its entry with `load` if this is the first lookup.
///
/// Only the lookup that computes the entry is first, and only it gets the error.
fn lookup<T>(
    entries: &Entries<T>,
    spec: &str,
    load: impl FnOnce() -> Result<Option<T>, LocalMetadataError>,
) -> Lookup<T> {
    let slot = slot(entries, spec);
    let mut computed = None;
    let entry = slot.get_or_init(|| match load() {
        Ok(found) => {
            computed = Some(None);
            found.map(Arc::new)
        }
        Err(err) => {
            computed = Some(Some(err));
            None
        }
    });
    match (computed, entry) {
        (Some(Some(err)), _) => Lookup::Error(err),
        (computed, Some(metadata)) => Lookup::Found {
            metadata: Arc::clone(metadata),
            first: computed.is_some(),
        },
        (_, None) => Lookup::None,
    }
}

/// Decodes the bytes of the file at `path` with `decode`, and returns the reference's error text
/// if they are not text or `decode` fails.
fn decode<T>(
    path: &Path,
    bytes: &[u8],
    decode: impl FnOnce(&SourceFile) -> Result<T, MetadataError>,
) -> Result<T, String> {
    // go-yaml reports bytes that do not decode without a line, as `yaml: <problem>`.
    let text = workflint_yaml::decode(bytes).map_err(|err| format!("yaml: {}", err.problem()))?;
    let file = SourceFile::new(FileId::new(0), path.to_string_lossy(), text.into_owned())
        .map_err(|err| err.to_string())?;
    decode(&file).map_err(|err| err.message().to_owned())
}

/// Returns the `uses:` value that names the file at `path` from `root`, the reference's
/// `convWorkflowPathToSpec`: its path from the root with `/` separators, after `./`.
///
/// Returns `None` for a path outside the root. Paths are compared by whole names
/// (`docs/differences.md#project-root-boundary`).
fn workflow_spec(root: &Path, path: &Path) -> Option<String> {
    let relative = clean(path).strip_prefix(root).ok()?.to_path_buf();
    let names: Option<Vec<&str>> = relative
        .components()
        .map(|component| match component {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect();
    Some(format!("./{}", names?.join("/")))
}

/// Converts the `/` separators of `path` to the separators of the OS, as Go's
/// `filepath.FromSlash` does.
fn from_slash(path: &str) -> PathBuf {
    path.split('/').collect()
}

/// Returns Go's text of the error that `os.ReadFile` returns for `path`, such as
/// `open /repo/x.yaml: no such file or directory`.
///
/// Go names the system call that failed, then the OS's message. On Unix, Go's own table of
/// messages is the C library's in lowercase. On Windows, Go prints the message that Windows
/// gives, as Rust does. A directory opens, and fails when it is read.
fn go_read_error(path: &Path, err: &io::Error) -> String {
    let op = if err.kind() == io::ErrorKind::IsADirectory {
        "read"
    } else {
        "open"
    };
    let message = err.to_string();
    // Rust appends ` (os error N)` to the OS's message.
    let message = match err.raw_os_error() {
        Some(code) => message
            .strip_suffix(&format!(" (os error {code})"))
            .unwrap_or(&message)
            .to_owned(),
        None => message,
    };
    let message = if cfg!(windows) {
        message
    } else {
        let mut chars = message.chars();
        match chars.next() {
            Some(first) => first.to_lowercase().chain(chars).collect(),
            None => message,
        }
    };
    format!("{op} {}: {message}", path.display())
}
