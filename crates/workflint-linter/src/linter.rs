//! The parallel driver: lints the inputs of a run with the caches of their projects and returns
//! each file's diagnostics in input order (ADR 0013).
//!
//! The driver logs through `tracing`. Each file's events are in a span named [`FILE_SPAN`], so
//! that a subscriber can hold them until the file is done and print them together. `INFO`
//! events are the lines of `-verbose`, and `DEBUG` events those of `-debug`.

use std::any::Any;
use std::collections::BTreeMap;
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use rayon::prelude::*;
use workflint_diagnostics::quote::quote;
use workflint_diagnostics::{Diagnostic, FileId, SourceFile, Span, sort_and_dedup};
use workflint_rules::RuleConfig;
use workflint_rules::local::{LocalMetadata, NoProject};
use workflint_yaml::rules::YAML_SYNTAX_ERROR;

use crate::cache::{CachePoisoned, FileMetadata, FileRecord, LocalCache, Prior};
use crate::config::{Config, ConfigError};
use crate::ignore::IgnorePattern;
use crate::path;
use crate::project::ProjectRoots;
use crate::rules::INTERNAL_ERROR;
use crate::settings::{Flags, Settings};

/// What lints one file: parses it, runs the rules with the project's local metadata and the
/// settings of the config file, and returns the diagnostics. [`Linter::new`] uses
/// [`workflint_rules::check_file`].
pub type Check = fn(&SourceFile, &dyn LocalMetadata, &RuleConfig) -> Vec<Diagnostic>;

/// One input of a run.
///
/// Only files named on the command line exist so far. M5.1 adds discovered files and stdin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    path: PathBuf,
}

impl Input {
    /// The file at `path`, relative to the working directory of the [`Linter`] if it is
    /// relative.
    #[must_use]
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The path of the file, as given.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

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

/// An error that stops the run before anything is printed.
#[derive(Debug, thiserror::Error)]
pub enum LintError {
    /// A file of the run could not be read.
    #[error(transparent)]
    Read(#[from] ReadError),
    /// The config file of a project could not be read.
    #[error(transparent)]
    Config(#[from] ConfigError),
}

/// The name of the span that holds the log events of one file.
pub const FILE_SPAN: &str = "file";

/// Lints the inputs of a run in parallel, with one [`LocalCache`] and one config file per
/// project.
#[derive(Debug, Clone)]
pub struct Linter {
    cwd: PathBuf,
    check: Check,
    flags: Arc<Flags>,
}

impl Linter {
    /// Creates a linter that resolves relative paths from `cwd`, an absolute path, and names
    /// files by their path from it.
    #[must_use]
    pub fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            check: workflint_rules::check_file,
            flags: Arc::default(),
        }
    }

    /// Returns the linter with `patterns` from `-ignore`: it drops every diagnostic whose
    /// message one of them matches, except an [`INTERNAL_ERROR`], which reports a bug.
    #[must_use]
    pub fn with_ignore(self, patterns: Vec<IgnorePattern>) -> Self {
        let mut flags = Flags::clone(&self.flags);
        flags.ignore = patterns;
        Self {
            flags: Arc::new(flags),
            ..self
        }
    }

    /// Returns the linter with `config` from `-config-file`, which every file of a run uses in
    /// place of the config file of its project.
    #[must_use]
    pub fn with_config_file(self, config: Config) -> Self {
        let mut flags = Flags::clone(&self.flags);
        flags.config_file = Some(Arc::new(config));
        Self {
            flags: Arc::new(flags),
            ..self
        }
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
    /// Fails as [`lint_with`](Self::lint_with) does.
    pub fn lint(&self, inputs: &[Input]) -> Result<Vec<Linted>, LintError> {
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
    /// Each file is linted with the [`Settings`] of its project: the defaults, then the config
    /// file that [`Config::find`] finds in the project root, or the one of
    /// [`with_config_file`](Self::with_config_file), then the flags. A project's config file is
    /// read once per run. The globs of `paths` match the path of the file from its project
    /// root, or from the working directory for a file in no project.
    ///
    /// # Errors
    ///
    /// Fails with the first input, in the order of `inputs`, whose project's config file cannot
    /// be read, and then with the first input that cannot be read.
    pub fn lint_with<T: Send>(
        &self,
        inputs: &[Input],
        finish: impl Fn(SourceFile, Vec<Diagnostic>) -> T + Sync,
    ) -> Result<Vec<T>, LintError> {
        if inputs.len() > 1 {
            tracing::info!("Linting {} files", inputs.len());
        }
        let finish = |file: &File<'_>,
                      source: SourceFile,
                      mut diagnostics: Vec<Diagnostic>,
                      started: Instant| {
            Self::ignore(file, &mut diagnostics);
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
        let mut settings = ProjectSettings::new(&self.flags);
        let mut files = Vec::with_capacity(inputs.len());
        for input in inputs {
            let root = roots.find(&input.path, &self.cwd);
            if let Some(root) = &root {
                caches
                    .entry(root.clone())
                    .or_insert_with(|| LocalCache::new(root.clone()));
            }
            let absolute = path::absolute(&self.cwd, &input.path);
            let relative = slash_path(root.as_deref().unwrap_or(&self.cwd), &absolute);
            files.push(File {
                given: &input.path,
                settings: settings.of(root.as_deref())?,
                absolute,
                relative,
                root,
            });
        }
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
        let mut again: Vec<Option<Again<'_, '_, T>>> = first.iter().map(|_| None).collect();
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
                        Again::Lint(file, source, cache.for_file_after(&file.absolute, prior))
                    }
                    Ok(None) => Again::Finish(file, source, diagnostics),
                    Err(diagnostic) => {
                        diagnostics.push(diagnostic);
                        sort_and_dedup(&source, &mut diagnostics);
                        Again::Finish(file, source, diagnostics)
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
        finish: &impl Fn(&File<'_>, SourceFile, Vec<Diagnostic>, Instant) -> T,
    ) -> Result<First<'c, T>, ReadError> {
        let started = Instant::now();
        let (source, decode_error) = read(index, file.given, &self.cwd)?;
        let _span = tracing::info_span!(FILE_SPAN).entered();
        tracing::info!("Linting {}", source.path());
        if let Some(root) = &file.root {
            tracing::info!("Using project at {}", root.display());
        }
        if let Some(path) = file.settings.config_path() {
            tracing::debug!("Config: {}", path.display());
        } else {
            tracing::debug!("No config was found");
        }
        if let Some(diagnostic) = decode_error {
            return Ok(First::Done(finish(file, source, vec![diagnostic], started)));
        }
        let config = file.settings.rules();
        let Some(cache) = cache else {
            let diagnostics = self.check(&source, &NoProject, config);
            return Ok(First::Done(finish(file, source, diagnostics, started)));
        };
        let local = cache.for_file(&file.absolute);
        let mut diagnostics = self.check(&source, &local, config);
        let record = local.into_record();
        if record.is_poisoned() {
            diagnostics.push(poisoned(&source));
            sort_and_dedup(&source, &mut diagnostics);
        }
        Ok(if record.is_empty() {
            First::Done(finish(file, source, diagnostics, started))
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
        again: Again<'_, '_, T>,
        finish: &impl Fn(&File<'_>, SourceFile, Vec<Diagnostic>, Instant) -> T,
    ) -> T {
        let started = Instant::now();
        match again {
            Again::Lint(file, source, local) => {
                let _span = tracing::info_span!(FILE_SPAN).entered();
                tracing::info!(
                    "Linting {} again, because the files before it changed what it reads",
                    source.path()
                );
                let mut diagnostics = self.check(&source, &local, file.settings.rules());
                if local.into_record().is_poisoned() {
                    diagnostics.push(poisoned(&source));
                    sort_and_dedup(&source, &mut diagnostics);
                }
                finish(file, source, diagnostics, started)
            }
            Again::Finish(file, source, diagnostics) => {
                let _span = tracing::info_span!(FILE_SPAN).entered();
                finish(file, source, diagnostics, started)
            }
            Again::Done(done) => done,
        }
    }

    /// Lints `source` with the project's local metadata in `local` and the settings of the
    /// config file in `config`, and returns the sorted diagnostics, or one [`INTERNAL_ERROR`]
    /// diagnostic if the lint panics.
    fn check(
        &self,
        source: &SourceFile,
        local: &dyn LocalMetadata,
        config: &RuleConfig,
    ) -> Vec<Diagnostic> {
        // The view of the cache may hold a half-done lookup after a panic. The file's record
        // still lists only the lookups that finished, so the replay stays right.
        let checked = panic::catch_unwind(AssertUnwindSafe(|| {
            let mut diagnostics = (self.check)(source, local, config);
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

    /// Drops the diagnostics of `file` that a pattern of `-ignore` or of a matching `paths`
    /// entry of the config matches, as the reference's `filterErrors` does, and logs each one.
    /// An [`INTERNAL_ERROR`] is never dropped.
    fn ignore(file: &File<'_>, diagnostics: &mut Vec<Diagnostic>) {
        let ignores = file.settings.ignores_for(&file.relative);
        if ignores.is_empty() {
            return;
        }
        let before = diagnostics.len();
        diagnostics.retain(|diagnostic| {
            if diagnostic.rule.id == INTERNAL_ERROR.id {
                return true;
            }
            let message = &diagnostic.message;
            if ignores
                .flags
                .iter()
                .any(|pattern| pattern.is_match(message))
            {
                tracing::debug!(
                    "Error {} is ignored due to -ignore command line option",
                    quote(message)
                );
                return false;
            }
            if ignores
                .paths
                .iter()
                .any(|pattern| pattern.is_match(message))
            {
                tracing::debug!(
                    "Error {} is ignored due to the \"ignore\" config in the config file",
                    quote(message)
                );
                return false;
            }
            true
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

/// An input with its project root and settings.
struct File<'i> {
    /// The path as given.
    given: &'i Path,
    /// The path made absolute and cleaned.
    absolute: PathBuf,
    /// The path from the project root, or from the working directory for a file in no project,
    /// with `/` separators. The globs of `paths` match it.
    relative: String,
    root: Option<PathBuf>,
    settings: Arc<Settings>,
}

/// The settings of each project of a run, each made once.
struct ProjectSettings<'f> {
    flags: &'f Flags,
    /// The settings of every file when `-config-file` is given.
    explicit: Option<Arc<Settings>>,
    /// The settings of a file in no project.
    default: Arc<Settings>,
    /// The settings of each project root found so far.
    projects: BTreeMap<PathBuf, Arc<Settings>>,
}

impl<'f> ProjectSettings<'f> {
    fn new(flags: &'f Flags) -> Self {
        let explicit = flags.config_file.as_deref().map(|config| {
            warn(config);
            Arc::new(Settings::default().with_config(config).with_flags(flags))
        });
        Self {
            flags,
            explicit,
            default: Arc::new(Settings::default().with_flags(flags)),
            projects: BTreeMap::new(),
        }
    }

    /// Returns the settings of a file in the project at `root`, reading the project's config
    /// file the first time.
    fn of(&mut self, root: Option<&Path>) -> Result<Arc<Settings>, ConfigError> {
        if let Some(explicit) = &self.explicit {
            return Ok(Arc::clone(explicit));
        }
        let Some(root) = root else {
            return Ok(Arc::clone(&self.default));
        };
        if let Some(settings) = self.projects.get(root) {
            return Ok(Arc::clone(settings));
        }
        let settings = match Config::find(root)? {
            Some(config) => {
                warn(&config);
                Arc::new(
                    Settings::default()
                        .with_config(&config)
                        .with_flags(self.flags),
                )
            }
            None => Arc::clone(&self.default),
        };
        self.projects
            .insert(root.to_path_buf(), Arc::clone(&settings));
        Ok(settings)
    }
}

/// Logs the warnings of `config`.
fn warn(config: &Config) {
    for warning in config.warnings() {
        tracing::warn!("{warning}");
    }
}

/// Returns `path` relative to `base`, both absolute and cleaned, with `/` separators. A path
/// outside `base` starts with `..`.
fn slash_path(base: &Path, path: &Path) -> String {
    let relative = path::relative(base, path).unwrap_or_else(|| path.to_path_buf());
    let parts: Vec<_> = relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect();
    parts.join("/")
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
enum Again<'f, 'c, T> {
    /// Lint the file again with this view of its cache.
    Lint(&'f File<'f>, SourceFile, FileMetadata<'c>),
    /// Finish the file with the diagnostics of the first lint.
    Finish(&'f File<'f>, SourceFile, Vec<Diagnostic>),
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

/// Reads and decodes the file at `path`, relative to `cwd` if it is relative, and returns it
/// with the YAML syntax error of bytes that do not decode.
///
/// The file is named by its path relative to `cwd` when there is one.
fn read(
    index: usize,
    path: &Path,
    cwd: &Path,
) -> Result<(SourceFile, Option<Diagnostic>), ReadError> {
    let fail = |message: String| ReadError {
        path: path.to_path_buf(),
        message,
    };
    let bytes = fs::read(cwd.join(path)).map_err(|err| fail(err.to_string()))?;
    let display = path::display_path(cwd, path);
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
