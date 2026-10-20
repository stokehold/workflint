//! The per-project cache of local actions and local reusable workflows, the reference's
//! `LocalActionsCache` (`action_metadata.go:195`) and `LocalReusableWorkflowCache`
//! (`reusable_workflow.go:169`).

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use workflint_diagnostics::{FileId, SourceFile};
use workflint_rules::local::{
    LocalAction, LocalMetadata, LocalMetadataError, Lookup, is_local_action,
    is_local_reusable_workflow,
};
use workflint_syntax::{ActionMetadata, MetadataError, ReusableWorkflowMetadata};

use crate::path::clean;

/// What reading and decoding the file behind a `uses:` value gave. `Ok(None)` is a local action
/// without a metadata file.
type Loaded<T> = Result<Option<Arc<T>>, LocalMetadataError>;

/// The files of one kind read in the run, keyed by `uses:` value. Each is read once, whichever
/// thread asks first.
type Store<T> = Mutex<BTreeMap<String, Arc<OnceLock<Loaded<T>>>>>;

/// The `uses:` values of one kind that the run has used, with the metadata that later lookups
/// find. A value is `None` when there is nothing to check against: the file is missing or
/// invalid, or its first lookup reported an error.
type Claims<T> = BTreeMap<String, Option<Arc<T>>>;

/// The state of the reference's caches: what a lookup finds at some point of the run.
#[derive(Debug, Clone, Default)]
struct State {
    actions: Claims<LocalAction>,
    workflows: Claims<ReusableWorkflowMetadata>,
}

/// The local actions and local reusable workflows of one project, read and decoded once per run.
///
/// In the reference, the first lookup of a `uses:` value in the run reads the file, reports its
/// error, and checks its metadata. A registration or an invalid `uses:` before any lookup stops
/// the file from being read. Which file is first depends on the order of the files, so the files
/// are linted in parallel in three steps that give the same diagnostics as linting them one
/// after another:
///
/// 1. Each file is linted through its own view from [`for_file`](Self::for_file), which answers
///    as if no other file had used the cache. The view records what the file did.
/// 2. [`commit`](Self::commit) replays the records in file order. It returns the state that the
///    earlier files left for a file whose answers that state would change.
/// 3. Each such file is linted again through [`for_file_after`](Self::for_file_after).
///
/// Which lookups and registrations a file makes depends only on the file, not on the answers,
/// so the second lint makes the same ones.
///
/// A panic while a lock of the cache is held leaves the cache in an unknown state. The views
/// then answer nothing and mark their record [poisoned](FileRecord::is_poisoned), and
/// [`commit`](Self::commit) fails, so that the driver reports the files whose diagnostics the
/// cache may have made wrong.
#[derive(Debug)]
pub struct LocalCache {
    root: PathBuf,
    actions: Store<LocalAction>,
    workflows: Store<ReusableWorkflowMetadata>,
    /// The state after the files committed so far.
    committed: Mutex<State>,
}

impl LocalCache {
    /// Creates an empty cache for the project at `root`, an absolute and cleaned path.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            actions: Mutex::default(),
            workflows: Mutex::default(),
            committed: Mutex::default(),
        }
    }

    /// Returns the project root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the view of the cache for linting the file at `path`, an absolute and cleaned
    /// path, as if no other file had used the cache. The view registers the file's
    /// `on.workflow_call` under its path from the root.
    #[must_use]
    pub fn for_file(&self, path: &Path) -> FileMetadata<'_> {
        self.view(path, State::default())
    }

    /// Returns the view of the cache for linting the file at `path` again, after the files
    /// before it left `prior`, which [`commit`](Self::commit) returned for the file.
    #[must_use]
    pub fn for_file_after(&self, path: &Path, prior: Prior) -> FileMetadata<'_> {
        self.view(path, prior.0)
    }

    fn view(&self, path: &Path, state: State) -> FileMetadata<'_> {
        FileMetadata {
            cache: self,
            spec: workflow_spec(&self.root, path),
            state: RefCell::new(state),
            events: RefCell::default(),
            poisoned: Cell::new(false),
        }
    }

    /// Replays the lookups and registrations of a file after those of every file committed
    /// before it. Call it once per file, in file order.
    ///
    /// Returns `None` if the file's view gave the answers that the files before it allow.
    /// Otherwise the diagnostics of the file are wrong, and the result is the state to lint it
    /// again with, through [`for_file_after`](Self::for_file_after).
    ///
    /// # Errors
    ///
    /// Fails if a panic left a lock of the cache poisoned.
    pub fn commit(&self, record: FileRecord) -> Result<Option<Prior>, CachePoisoned> {
        let mut committed = self.committed.lock().map_err(|_| CachePoisoned)?;
        let mut prior = State::default();
        // The `uses:` values the file used before the current event, whose state in
        // `committed` is now the file's own.
        let mut seen_actions = BTreeSet::new();
        let mut seen_workflows = BTreeSet::new();
        let mut same = true;
        for event in record.events {
            match event {
                Event::Action { spec, answer } => {
                    if seen_actions.insert(spec.clone()) {
                        keep_prior(&mut prior.actions, &committed.actions, &spec);
                    }
                    let truth = lookup(&mut committed.actions, &spec, || {
                        self.load_action(&spec).ok_or(CachePoisoned)
                    })?;
                    same &= same_answer(&truth, &answer);
                }
                Event::Workflow { spec, answer } => {
                    if seen_workflows.insert(spec.clone()) {
                        keep_prior(&mut prior.workflows, &committed.workflows, &spec);
                    }
                    let truth = lookup(&mut committed.workflows, &spec, || {
                        self.load_workflow(&spec).ok_or(CachePoisoned)
                    })?;
                    same &= same_answer(&truth, &answer);
                }
                Event::Claim { spec, metadata } => {
                    if seen_workflows.insert(spec.clone()) {
                        keep_prior(&mut prior.workflows, &committed.workflows, &spec);
                    }
                    claim(&mut committed.workflows, &spec, metadata);
                }
            }
        }
        Ok((!same).then_some(Prior(prior)))
    }

    /// Returns the local action at `spec`, reading and decoding it on the first call, or `None`
    /// if the store is poisoned.
    fn load_action(&self, spec: &str) -> Option<Loaded<LocalAction>> {
        load(&self.actions, spec, || self.read_action(spec))
    }

    /// Returns the local reusable workflow at `spec`, reading and decoding it on the first call,
    /// or `None` if the store is poisoned.
    fn load_workflow(&self, spec: &str) -> Option<Loaded<ReusableWorkflowMetadata>> {
        load(&self.workflows, spec, || self.read_workflow(spec))
    }

    /// Reads and decodes the local action at `spec`.
    fn read_action(&self, spec: &str) -> Result<Option<LocalAction>, LocalMetadataError> {
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
    fn read_workflow(
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

/// The state that the files before a file left for the `uses:` values the file uses.
#[derive(Debug)]
pub struct Prior(State);

/// A panic while a lock of a [`LocalCache`] was held left the cache in an unknown state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "a panic while linting an earlier file left the cache of local actions and reusable workflows unusable"
)]
pub struct CachePoisoned;

/// What linting one file did with the cache, in order, for [`LocalCache::commit`].
#[derive(Debug, Default)]
pub struct FileRecord {
    events: Vec<Event>,
    /// Whether a lookup found the cache poisoned and answered nothing.
    poisoned: bool,
}

#[derive(Debug)]
enum Event {
    /// A lookup of a local action, with the answer the view gave.
    Action {
        spec: String,
        answer: Lookup<LocalAction>,
    },
    /// A lookup of a local reusable workflow, with the answer the view gave.
    Workflow {
        spec: String,
        answer: Lookup<ReusableWorkflowMetadata>,
    },
    /// A registered `on.workflow_call`, or `None` for an invalid `uses:` value.
    Claim {
        spec: String,
        metadata: Option<Arc<ReusableWorkflowMetadata>>,
    },
}

/// The view of a [`LocalCache`] for linting one file.
#[derive(Debug)]
pub struct FileMetadata<'c> {
    cache: &'c LocalCache,
    /// The `uses:` value that names the linted file, or `None` if it is outside the root.
    spec: Option<String>,
    /// The state of the caches as this file sees it.
    state: RefCell<State>,
    events: RefCell<Vec<Event>>,
    poisoned: Cell<bool>,
}

impl FileRecord {
    /// Whether the file did nothing with the cache, so that [`LocalCache::commit`] would find
    /// nothing to change.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Whether a lookup of the file found the cache poisoned, and so answered nothing where the
    /// cache may know an answer.
    #[must_use]
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }
}

impl FileMetadata<'_> {
    /// Returns what the file did with the cache, for [`LocalCache::commit`].
    #[must_use]
    pub fn into_record(self) -> FileRecord {
        FileRecord {
            events: self.events.into_inner(),
            poisoned: self.poisoned.get(),
        }
    }

    /// Returns what `load` returns, or, if the store is poisoned, marks the view poisoned and
    /// returns nothing.
    fn unless_poisoned<T>(&self, load: Option<Loaded<T>>) -> Loaded<T> {
        load.unwrap_or_else(|| {
            self.poisoned.set(true);
            Ok(None)
        })
    }
}

impl LocalMetadata for FileMetadata<'_> {
    fn action(&self, spec: &str) -> Lookup<LocalAction> {
        if !is_local_action(spec) {
            return Lookup::None;
        }
        let Ok(answer) = lookup(&mut self.state.borrow_mut().actions, spec, || {
            Ok::<_, Infallible>(self.unless_poisoned(self.cache.load_action(spec)))
        });
        self.events.borrow_mut().push(Event::Action {
            spec: spec.to_owned(),
            answer: answer.clone(),
        });
        answer
    }

    fn reusable_workflow(&self, spec: &str) -> Lookup<ReusableWorkflowMetadata> {
        if !is_local_reusable_workflow(spec) {
            return Lookup::None;
        }
        let Ok(answer) = lookup(&mut self.state.borrow_mut().workflows, spec, || {
            Ok::<_, Infallible>(self.unless_poisoned(self.cache.load_workflow(spec)))
        });
        self.events.borrow_mut().push(Event::Workflow {
            spec: spec.to_owned(),
            answer: answer.clone(),
        });
        answer
    }

    fn register_workflow_call(&self, metadata: ReusableWorkflowMetadata) {
        if let Some(spec) = &self.spec {
            self.claim_workflow(spec, Some(Arc::new(metadata)));
        }
    }

    fn record_invalid_workflow(&self, spec: &str) {
        self.claim_workflow(spec, None);
    }

    fn action_file_exists(&self, action: &LocalAction, file: &str) -> bool {
        let path = clean(&action.dir.join(from_slash(file)));
        // As in the reference, only a file that is known not to exist is missing.
        !matches!(fs::metadata(path), Err(err) if err.kind() == io::ErrorKind::NotFound)
    }
}

impl FileMetadata<'_> {
    fn claim_workflow(&self, spec: &str, metadata: Option<Arc<ReusableWorkflowMetadata>>) {
        claim(
            &mut self.state.borrow_mut().workflows,
            spec,
            metadata.clone(),
        );
        self.events.borrow_mut().push(Event::Claim {
            spec: spec.to_owned(),
            metadata,
        });
    }
}

/// Returns the entry of `spec` in `store`, computing it with `read` if no thread has, or `None`
/// if the store is poisoned.
///
/// A panic in `read` leaves the entry empty, so the next call reads it again.
fn load<T>(
    store: &Store<T>,
    spec: &str,
    read: impl FnOnce() -> Result<Option<T>, LocalMetadataError>,
) -> Option<Loaded<T>> {
    let slot = {
        let mut store = store.lock().ok()?;
        Arc::clone(store.entry(spec.to_owned()).or_default())
    };
    Some(
        slot.get_or_init(|| read().map(|found| found.map(Arc::new)))
            .clone(),
    )
}

/// Looks up `spec` in `claims` as the reference's cache does. The first lookup claims `spec`
/// with what `load` returns, and only it is first and gets the error. Fails, claiming nothing,
/// if `load` fails.
fn lookup<T, E>(
    claims: &mut Claims<T>,
    spec: &str,
    load: impl FnOnce() -> Result<Loaded<T>, E>,
) -> Result<Lookup<T>, E> {
    if let Some(claimed) = claims.get(spec) {
        return Ok(match claimed {
            Some(metadata) => Lookup::Found {
                metadata: Arc::clone(metadata),
                first: false,
            },
            None => Lookup::None,
        });
    }
    let loaded = load()?;
    let found = loaded.as_ref().ok().and_then(Option::clone);
    claims.insert(spec.to_owned(), found);
    Ok(match loaded {
        Ok(Some(metadata)) => Lookup::Found {
            metadata,
            first: true,
        },
        Ok(None) => Lookup::None,
        Err(err) => Lookup::Error(err),
    })
}

/// Claims `spec` with `metadata` unless it is already claimed.
fn claim<T>(claims: &mut Claims<T>, spec: &str, metadata: Option<Arc<T>>) {
    claims.entry(spec.to_owned()).or_insert(metadata);
}

/// Copies the claim on `spec` in `committed`, if there is one, to `prior`.
fn keep_prior<T>(prior: &mut Claims<T>, committed: &Claims<T>, spec: &str) {
    if let Some(claimed) = committed.get(spec) {
        prior.insert(spec.to_owned(), claimed.clone());
    }
}

/// Whether two answers to the same lookup are the same, with the same metadata.
fn same_answer<T>(a: &Lookup<T>, b: &Lookup<T>) -> bool {
    match (a, b) {
        (Lookup::None, Lookup::None) => true,
        (
            Lookup::Found {
                metadata: a,
                first: a_first,
            },
            Lookup::Found {
                metadata: b,
                first: b_first,
            },
        ) => a_first == b_first && Arc::ptr_eq(a, b),
        (Lookup::Error(a), Lookup::Error(b)) => a == b,
        _ => false,
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

#[cfg(test)]
mod tests {
    #![allow(clippy::panic)]

    use super::*;

    /// Panics on another thread while `mutex` is locked, which poisons it.
    fn poison<T: Send>(mutex: &Mutex<T>) {
        let result = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let _guard = mutex.lock();
                    panic!("poisoning the lock");
                })
                .join()
        });
        assert!(result.is_err());
        assert!(mutex.is_poisoned());
    }

    fn cache() -> LocalCache {
        // No test reads a file, so the root need not exist.
        LocalCache::new(PathBuf::from("/project"))
    }

    #[test]
    fn a_lookup_in_a_poisoned_store_finds_nothing_and_marks_the_record_poisoned() {
        let cache = cache();
        poison(&cache.actions);
        poison(&cache.workflows);
        let view = cache.for_file(Path::new("/project/.github/workflows/ci.yaml"));
        assert_eq!(view.action("./greet"), Lookup::None);
        assert_eq!(
            view.reusable_workflow("./.github/workflows/reusable.yaml"),
            Lookup::None
        );
        let record = view.into_record();
        assert!(record.is_poisoned());
    }

    #[test]
    fn a_view_that_finds_nothing_poisoned_leaves_its_record_clean() {
        let cache = cache();
        let view = cache.for_file(Path::new("/project/.github/workflows/ci.yaml"));
        assert_eq!(view.action("owner/repo@v1"), Lookup::None);
        assert!(!view.into_record().is_poisoned());
    }

    #[test]
    fn commit_fails_when_the_committed_state_is_poisoned() {
        let cache = cache();
        poison(&cache.committed);
        assert!(matches!(
            cache.commit(FileRecord::default()),
            Err(CachePoisoned)
        ));
    }

    #[test]
    fn commit_fails_when_a_replayed_lookup_finds_the_store_poisoned() {
        let cache = cache();
        poison(&cache.actions);
        let record = FileRecord {
            events: vec![Event::Action {
                spec: "./greet".to_owned(),
                answer: Lookup::None,
            }],
            poisoned: false,
        };
        assert!(matches!(cache.commit(record), Err(CachePoisoned)));
        // The lookup that fails claims nothing.
        let committed = cache.committed.lock().ok();
        assert!(committed.is_some_and(|state| state.actions.is_empty()));
    }
}
