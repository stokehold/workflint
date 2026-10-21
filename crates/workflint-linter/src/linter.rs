//! The parallel driver: lints the inputs of a run with the caches of their projects and returns
//! each file's diagnostics in input order (ADR 0013).
//!
//! The driver logs through `tracing`. Each file's events are in a span named [`FILE_SPAN`], so
//! that a subscriber can hold them until the file is done and print them together. `INFO`
//! events are the lines of `-verbose`, and `DEBUG` events those of `-debug`.

use std::any::Any;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, ErrorKind, Read};
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use rayon::prelude::*;
use workflint_diagnostics::quote::quote;
use workflint_diagnostics::{Diagnostic, FileId, SourceFile, Span, sort_and_dedup};
use workflint_rules::local::{LocalMetadata, NoProject};
use workflint_rules::tools::ExternalTools;
use workflint_yaml::rules::YAML_SYNTAX_ERROR;

use crate::cache::{CachePoisoned, FileMetadata, FileRecord, LocalCache, Prior};
use crate::discover::{self, DiscoveryError};
use crate::ignore::IgnorePattern;
use crate::path;
use crate::project::ProjectRoots;
use crate::rules::INTERNAL_ERROR;
use crate::tools::Tools;

/// What lints one file: parses it, runs the rules with the project's local metadata and the
/// run's external tools, and returns the diagnostics. [`Linter::new`] uses
/// [`workflint_rules::check_file`].
pub type Check = fn(&SourceFile, &dyn LocalMetadata, &dyn ExternalTools) -> Vec<Diagnostic>;

/// One input of a run: a file to read, or a workflow read from stdin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    path: PathBuf,
    /// The text read from stdin, or `None` for a file.
    stdin: Option<Vec<u8>>,
}

/// The name of a workflow read from stdin when `-stdin-filename` gives none. A workflow with
/// this name has no project.
pub const STDIN: &str = "<stdin>";

impl Input {
    /// The file at `path`, relative to the working directory of the [`Linter`] if it is
    /// relative.
    #[must_use]
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            stdin: None,
        }
    }

    /// Reads a workflow from `reader`, stdin, to the end, and names it `name`, or [`STDIN`] if
    /// `name` is empty (`linter.go:177`, `linter.go:474`).
    ///
    /// The diagnostics print `name` as given. If a file exists at `name`, the workflow gets the
    /// project of that file, with its local actions and reusable workflows (`linter.go:487`).
    ///
    /// # Errors
    ///
    /// Fails if `reader` fails.
    pub fn stdin(name: &str, mut reader: impl Read) -> Result<Self, StdinError> {
        tracing::info!("Reading the input from stdin");
        let mut text = Vec::new();
        reader.read_to_end(&mut text).map_err(StdinError)?;
        let name = if name.is_empty() { STDIN } else { name };
        Ok(Self {
            path: PathBuf::from(name),
            stdin: Some(text),
        })
    }

    /// The path of the file, or the name of the workflow read from stdin, as given.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Stdin could not be read. As in the reference, it stops the run.
#[derive(Debug, thiserror::Error)]
#[error("could not read stdin: {0}")]
pub struct StdinError(io::Error);

/// A linted file: its text and its sorted diagnostics.
#[derive(Debug)]
pub struct Linted {
    /// The file, named by its path from the working directory when it has one.
    pub file: SourceFile,
    /// The diagnostics, sorted and without duplicates (ADR 0007).
    pub diagnostics: Vec<Diagnostic>,
}

impl Linted {
    /// Whether the file has an [`INTERNAL_ERROR`] diagnostic, so that its other diagnostics may
    /// be missing.
    #[must_use]
    pub fn has_internal_error(&self) -> bool {
        has_internal_error(&self.diagnostics)
    }
}

/// Whether `diagnostics` has an [`INTERNAL_ERROR`] diagnostic.
#[must_use]
pub fn has_internal_error(diagnostics: &[Diagnostic]) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.rule.id == INTERNAL_ERROR.id)
}

/// A file of the run could not be read. As in the reference, it stops the run before anything
/// is printed.
#[derive(Debug, thiserror::Error)]
#[error("could not read {}: {message}", quote(&.path.to_string_lossy()))]
pub struct ReadError {
    /// The path as given.
    path: PathBuf,
    message: String,
}

/// The name of the span that holds the log events of one file.
pub const FILE_SPAN: &str = "file";

/// Lints the inputs of a run in parallel, with one [`LocalCache`] per project.
#[derive(Debug, Clone)]
pub struct Linter {
    cwd: PathBuf,
    check: Check,
    ignore: Arc<[IgnorePattern]>,
    tools: Arc<Tools>,
}

impl Linter {
    /// Creates a linter that resolves relative paths from `cwd`, an absolute path, and names
    /// files by their path from it.
    #[must_use]
    pub fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            check: workflint_rules::check_file,
            ignore: Arc::new([]),
            tools: Arc::new(Tools::none()),
        }
    }

    /// Returns the linter with `tools` as the external tools that check the scripts of the
    /// files. [`Linter::new`] runs none.
    #[must_use]
    pub fn with_tools(self, tools: Tools) -> Self {
        Self {
            tools: Arc::new(tools),
            ..self
        }
    }

    /// Returns the linter with `patterns` from `-ignore`: it drops every diagnostic whose
    /// message one of them matches, except an [`INTERNAL_ERROR`], which reports a bug.
    #[must_use]
    pub fn with_ignore(self, patterns: Vec<IgnorePattern>) -> Self {
        Self {
            ignore: patterns.into(),
            ..self
        }
    }

    /// Returns the workflow files that a run without file arguments lints: the files under
    /// `.github/workflows` of the project of the working directory whose names end in `.yml` or
    /// `.yaml`, sorted by path (`linter.go:264-311`).
    ///
    /// # Errors
    ///
    /// Fails if the working directory has no project, if a directory under `.github/workflows`
    /// cannot be read, or if no file is found.
    pub fn discover(&self) -> Result<Vec<Input>, DiscoveryError> {
        Ok(discover::workflow_files(&self.cwd)?
            .into_iter()
            .map(Input::file)
            .collect())
    }

    /// Returns the linter with `check` in place of [`workflint_rules::check_file`], which the
    /// tests of the driver use to make the lint of a file panic.
    #[must_use]
    pub fn with_check(self, check: Check) -> Self {
        Self { check, ..self }
    }

    /// Lints `inputs` and returns one [`Linted`] per input, in the order of `inputs`.
    ///
    /// # Errors
    ///
    /// Fails with the first input, in the order of `inputs`, that cannot be read.
    pub fn lint(&self, inputs: &[Input]) -> Result<Vec<Linted>, ReadError> {
        self.lint_with(inputs, |file, diagnostics| Linted { file, diagnostics })
    }

    /// Lints `inputs`, calls `finish` with each file and its sorted diagnostics as soon as they
    /// are final, and returns what `finish` returns, in the order of `inputs`.
    ///
    /// `finish` runs in parallel. A caller that renders in it frees each file's text before the
    /// other files are done.
    ///
    /// Each file is linted as if no other file had used the cache of its project. The uses of
    /// each cache are then replayed in the order of the files' paths, and a file whose answers
    /// the files before it change is linted again ([`LocalCache`]). So the errors of a shared
    /// local action or reusable workflow go to its first use by path, whatever the order of
    /// `inputs` and the number of threads.
    ///
    /// A panic in the lint of a file becomes one [`INTERNAL_ERROR`] diagnostic on that file,
    /// and the other files are linted as usual.
    ///
    /// # Errors
    ///
    /// Fails with the first input, in the order of `inputs`, that cannot be read.
    pub fn lint_with<T: Send>(
        &self,
        inputs: &[Input],
        finish: impl Fn(SourceFile, Vec<Diagnostic>) -> T + Sync,
    ) -> Result<Vec<T>, ReadError> {
        if inputs.len() > 1 {
            tracing::info!("Linting {} files", inputs.len());
        }
        let finish = |source: SourceFile, mut diagnostics: Vec<Diagnostic>, started: Instant| {
            self.ignore(&mut diagnostics);
            tracing::info!(
                "Found total {} errors in {} ms for {}",
                diagnostics.len(),
                started.elapsed().as_millis(),
                source.path()
            );
            finish(source, diagnostics)
        };
        let mut roots = ProjectRoots::default();
        let mut caches = BTreeMap::new();
        let files: Vec<File> = inputs
            .iter()
            .map(|input| {
                let root = if input.stdin.is_some() {
                    stdin_root(&mut roots, &input.path, &self.cwd)
                } else {
                    roots.find(&input.path, &self.cwd)
                };
                if let Some(root) = &root {
                    caches
                        .entry(root.clone())
                        .or_insert_with(|| LocalCache::new(root.clone()));
                }
                File {
                    input,
                    absolute: path::absolute(&self.cwd, &input.path),
                    root,
                }
            })
            .collect();
        let cache_of = |file: &File<'_>| file.root.as_ref().and_then(|root| caches.get(root));

        let first: Vec<Result<First<'_, T>, ReadError>> = files
            .par_iter()
            .enumerate()
            .map(|(index, file)| self.first(index, file, cache_of(file), &finish))
            .collect();
        let mut first = first.into_iter().collect::<Result<Vec<_>, _>>()?;

        // Replays the uses of the caches in the order of the paths. The sort is stable, so a
        // file given twice keeps the order of `inputs`.
        let mut by_path: Vec<(usize, &File<'_>)> = files.iter().enumerate().collect();
        by_path.sort_by_key(|(_, file)| file.absolute.as_path());
        let mut again: Vec<Option<Again<'_, T>>> = first.iter().map(|_| None).collect();
        for (index, file) in by_path {
            let Some(slot) = first.get_mut(index) else {
                continue;
            };
            let next = match std::mem::replace(slot, First::Taken) {
                First::Pending {
                    source,
                    mut diagnostics,
                    cache,
                    record,
                } => match commit(cache, record, &source) {
                    Ok(Some(prior)) => {
                        Again::Lint(source, cache.for_file_after(&file.absolute, prior))
                    }
                    Ok(None) => Again::Finish(source, diagnostics),
                    Err(diagnostic) => {
                        diagnostics.push(diagnostic);
                        sort_and_dedup(&source, &mut diagnostics);
                        Again::Finish(source, diagnostics)
                    }
                },
                First::Done(done) => Again::Done(done),
                First::Taken => continue,
            };
            if let Some(slot) = again.get_mut(index) {
                *slot = Some(next);
            }
        }

        Ok(again
            .into_par_iter()
            .flatten()
            .map(|again| self.again(again, &finish))
            .collect())
    }

    /// Reads and lints `file`, the input at `index`, with `cache`, the cache of its project.
    fn first<'c, T>(
        &self,
        index: usize,
        file: &File<'_>,
        cache: Option<&'c LocalCache>,
        finish: &impl Fn(SourceFile, Vec<Diagnostic>, Instant) -> T,
    ) -> Result<First<'c, T>, ReadError> {
        let started = Instant::now();
        let (source, decode_error) = read(index, file.input, &self.cwd)?;
        let _span = tracing::info_span!(FILE_SPAN).entered();
        tracing::info!("Linting {}", source.path());
        if let Some(root) = &file.root {
            tracing::info!("Using project at {}", root.display());
        }
        if let Some(diagnostic) = decode_error {
            return Ok(First::Done(finish(source, vec![diagnostic], started)));
        }
        let Some(cache) = cache else {
            let diagnostics = self.check(&source, &NoProject);
            return Ok(First::Done(finish(source, diagnostics, started)));
        };
        let local = cache.for_file(&file.absolute);
        let mut diagnostics = self.check(&source, &local);
        let record = local.into_record();
        if record.is_poisoned() {
            diagnostics.push(poisoned(&source));
            sort_and_dedup(&source, &mut diagnostics);
        }
        Ok(if record.is_empty() {
            First::Done(finish(source, diagnostics, started))
        } else {
            First::Pending {
                source,
                diagnostics,
                cache,
                record,
            }
        })
    }

    /// Does what is left of a file after the uses of the caches are replayed.
    fn again<T>(
        &self,
        again: Again<'_, T>,
        finish: &impl Fn(SourceFile, Vec<Diagnostic>, Instant) -> T,
    ) -> T {
        let started = Instant::now();
        match again {
            Again::Lint(source, local) => {
                let _span = tracing::info_span!(FILE_SPAN).entered();
                tracing::info!(
                    "Linting {} again, because the files before it changed what it reads",
                    source.path()
                );
                let mut diagnostics = self.check(&source, &local);
                if local.into_record().is_poisoned() {
                    diagnostics.push(poisoned(&source));
                    sort_and_dedup(&source, &mut diagnostics);
                }
                finish(source, diagnostics, started)
            }
            Again::Finish(source, diagnostics) => {
                let _span = tracing::info_span!(FILE_SPAN).entered();
                finish(source, diagnostics, started)
            }
            Again::Done(done) => done,
        }
    }

    /// Lints `source` with the project's local metadata in `local` and returns the sorted
    /// diagnostics, or one [`INTERNAL_ERROR`] diagnostic if the lint panics.
    fn check(&self, source: &SourceFile, local: &dyn LocalMetadata) -> Vec<Diagnostic> {
        // The view of the cache may hold a half-done lookup after a panic. The file's record
        // still lists only the lookups that finished, so the replay stays right.
        let checked = panic::catch_unwind(AssertUnwindSafe(|| {
            let mut diagnostics = (self.check)(source, local, self.tools.as_ref());
            sort_and_dedup(source, &mut diagnostics);
            diagnostics
        }));
        let diagnostics =
            checked.unwrap_or_else(|payload| vec![panicked(source, payload.as_ref())]);
        if tracing::enabled!(tracing::Level::DEBUG) {
            let mut counts = BTreeMap::new();
            for diagnostic in &diagnostics {
                *counts.entry(diagnostic.rule.id).or_insert(0_usize) += 1;
            }
            for (rule, count) in counts {
                tracing::debug!("{rule} found {count} errors");
            }
        }
        diagnostics
    }

    /// Drops the diagnostics that an ignore pattern matches, as the reference's `filterErrors`
    /// does, and logs each one.
    fn ignore(&self, diagnostics: &mut Vec<Diagnostic>) {
        if self.ignore.is_empty() {
            return;
        }
        let before = diagnostics.len();
        diagnostics.retain(|diagnostic| {
            let ignored = diagnostic.rule.id != INTERNAL_ERROR.id
                && self
                    .ignore
                    .iter()
                    .any(|pattern| pattern.is_match(&diagnostic.message));
            if ignored {
                tracing::debug!(
                    "Error {} is ignored due to -ignore command line option",
                    quote(&diagnostic.message)
                );
            }
            !ignored
        });
        if diagnostics.len() != before {
            tracing::info!(
                "Filtered {} error(s) due to \"-ignore\" command line option and \"ignore\" \
                 configuration",
                before - diagnostics.len()
            );
        }
    }
}

/// Replays the `record` of `source` in `cache` ([`LocalCache::commit`]). Fails with the
/// diagnostic to report on `source` if that panics or finds the cache poisoned.
fn commit(
    cache: &LocalCache,
    record: FileRecord,
    source: &SourceFile,
) -> Result<Option<Prior>, Diagnostic> {
    match panic::catch_unwind(AssertUnwindSafe(|| cache.commit(record))) {
        Ok(Ok(prior)) => Ok(prior),
        Ok(Err(CachePoisoned)) => Err(poisoned(source)),
        Err(_) => Err(commit_panicked(source)),
    }
}

/// An input with its project root.
struct File<'i> {
    input: &'i Input,
    /// The path made absolute and cleaned.
    absolute: PathBuf,
    root: Option<PathBuf>,
}

/// A file after its first lint.
enum First<'c, T> {
    /// The file did not use a cache, so `finish` has its final diagnostics.
    Done(T),
    /// The file used `cache`, so its diagnostics may depend on the files before it.
    Pending {
        source: SourceFile,
        /// The sorted diagnostics.
        diagnostics: Vec<Diagnostic>,
        cache: &'c LocalCache,
        record: FileRecord,
    },
    /// The replay has moved the file on.
    Taken,
}

/// What is left to do for a file after the uses of the caches are replayed.
enum Again<'c, T> {
    /// Lint the file again with this view of its cache.
    Lint(SourceFile, FileMetadata<'c>),
    /// Finish the file with the diagnostics of the first lint.
    Finish(SourceFile, Vec<Diagnostic>),
    Done(T),
}

/// Where the report of a bug goes.
const REPORT: &str = "This is a bug in workflint. Please report it at \
                      https://github.com/stokehold/workflint/issues";

/// The [`INTERNAL_ERROR`] diagnostic of a panic with `payload` in the lint of `source`.
fn panicked(source: &SourceFile, payload: &(dyn Any + Send)) -> Diagnostic {
    let message = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str));
    let message = match message {
        // The one-line format prints a diagnostic on one line.
        Some(message) => format!(
            "workflint panicked while linting this file: {}. {REPORT}",
            message.replace(['\r', '\n'], " ")
        ),
        None => format!("workflint panicked while linting this file. {REPORT}"),
    };
    internal_error(source, message)
}

/// The [`INTERNAL_ERROR`] diagnostic of a panic while replaying the uses of the cache of
/// `source`'s project.
fn commit_panicked(source: &SourceFile) -> Diagnostic {
    internal_error(
        source,
        format!(
            "workflint panicked while checking this file against the cache of local actions and \
             reusable workflows. {REPORT}"
        ),
    )
}

/// The [`INTERNAL_ERROR`] diagnostic of a file whose cache a panic left unusable.
fn poisoned(source: &SourceFile) -> Diagnostic {
    internal_error(source, format!("{CachePoisoned}. {REPORT}"))
}

fn internal_error(source: &SourceFile, message: String) -> Diagnostic {
    Diagnostic::new(&INTERNAL_ERROR, Span::new(source.id(), 0, 0), message)
}

/// The project root of a workflow read from stdin and named `name`: that of the file at `name`,
/// relative to `cwd` if it is relative, if one exists there (`linter.go:487`).
///
/// As in the reference, a path that names a directory is searched from that directory, and an
/// error other than a missing file still leads to a search.
fn stdin_root(roots: &mut ProjectRoots, name: &Path, cwd: &Path) -> Option<PathBuf> {
    if name == Path::new(STDIN) {
        return None;
    }
    match fs::metadata(cwd.join(name)) {
        Err(err) if err.kind() == ErrorKind::NotFound => None,
        Ok(meta) if meta.is_dir() => roots.find_dir(name, cwd),
        _ => roots.find(name, cwd),
    }
}

/// Reads and decodes `input`, a file relative to `cwd` if it is relative, and returns it with
/// the YAML syntax error of bytes that do not decode.
///
/// A file is named by its path relative to `cwd` when there is one, and a workflow read from
/// stdin by its name as given.
fn read(
    index: usize,
    input: &Input,
    cwd: &Path,
) -> Result<(SourceFile, Option<Diagnostic>), ReadError> {
    let path = &input.path;
    let fail = |message: String| ReadError {
        path: path.clone(),
        message,
    };
    let (bytes, display) = match &input.stdin {
        Some(text) => (
            Cow::Borrowed(text.as_slice()),
            Cow::Borrowed(path.as_path()),
        ),
        None => (
            Cow::Owned(fs::read(cwd.join(path)).map_err(|err| fail(err.to_string()))?),
            Cow::Owned(path::display_path(cwd, path)),
        ),
    };
    let display = display.to_string_lossy();
    let id = FileId::new(u32::try_from(index).map_err(|err| fail(err.to_string()))?);
    let (text, decode_error) = match workflint_yaml::decode(&bytes) {
        Ok(text) => (text.into_owned(), None),
        Err(err) => (decoded_prefix(&bytes, err.offset()), Some(err)),
    };
    let file = SourceFile::new(id, display.as_ref(), text).map_err(|err| fail(err.to_string()))?;
    // As in the reference, bytes that do not decode are a YAML syntax error. It points at the
    // end of the text that does decode.
    let diagnostic = decode_error.map(|err| {
        let end = u32::try_from(file.text().len()).unwrap_or(u32::MAX);
        Diagnostic::new(
            &YAML_SYNTAX_ERROR,
            Span::new(id, end, end),
            format!("could not parse as YAML: {}", err.problem()),
        )
    });
    Ok((file, diagnostic))
}

/// Decodes the longest prefix of `bytes` that ends at or before `end` and decodes without error.
fn decoded_prefix(bytes: &[u8], mut end: usize) -> String {
    loop {
        match bytes.get(..end).map(workflint_yaml::decode) {
            Some(Ok(text)) => return text.into_owned(),
            Some(Err(err)) if err.offset() < end => end = err.offset(),
            _ => return String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn decoded_prefix_stops_before_an_invalid_utf8_sequence() {
        // 0xC3 starts a two-byte sequence, which `x` does not continue.
        let bytes = b"a: b\xC3x";
        let err = workflint_yaml::decode(bytes).unwrap_err();
        assert_eq!(decoded_prefix(bytes, err.offset()), "a: b");
    }

    #[test]
    fn decoded_prefix_stops_before_a_lone_utf16_surrogate() {
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend("a: ".encode_utf16().flat_map(u16::to_le_bytes));
        bytes.extend(0xD800_u16.to_le_bytes());
        bytes.extend(u16::from(b'x').to_le_bytes());
        let err = workflint_yaml::decode(&bytes).unwrap_err();
        assert_eq!(decoded_prefix(&bytes, err.offset()), "a: ");
    }

    fn source() -> SourceFile {
        SourceFile::new(FileId::new(0), "a.yaml", "on: push\n").unwrap()
    }

    #[test]
    fn a_panic_payload_that_is_not_a_string_still_gives_a_message() {
        let diagnostic = panicked(&source(), &42_u32);
        assert_eq!(
            diagnostic.message,
            format!("workflint panicked while linting this file. {REPORT}")
        );
    }

    #[test]
    fn the_line_breaks_of_a_panic_message_become_spaces() {
        let diagnostic = panicked(&source(), &String::from("two\nlines"));
        assert_eq!(
            diagnostic.message,
            format!("workflint panicked while linting this file: two lines. {REPORT}")
        );
    }
}
