//! The external tools of a run, which check the scripts that rules collect (ADR 0012).
//!
//! [`Tools`] implements [`ExternalTools`] for the rules. It runs shellcheck through a
//! [`ToolPool`], turns its JSON output into findings, and turns a run that gives no findings into
//! the message of a `shellcheck-failed` diagnostic.
//!
//! Each script runs once per run with its shell and [`ShellcheckOptions`], the settings of its
//! project. Every script that has no outcome yet goes into one queue. A thread that waits for
//! outcomes takes scripts that share a shell and options from the queue, up to [`MAX_BATCH`],
//! and checks them in one shellcheck process, each script in a file of a temporary directory.
//! shellcheck's JSON output names the file of each finding, and its findings are those of each
//! script alone, because shellcheck resolves `source` from the working directory for a file as
//! for stdin. When a batch fails, each of its scripts runs alone on stdin, so a failure has the
//! message it would have alone. The driver queues the scripts of every file before it lints any
//! ([`Tools::prefetch`]), so that a run with many files starts few processes.
//!
//! With the cache of the options on, a script whose findings are in the [`DiskCache`] does not
//! run, and the findings of each script that runs are stored there.

mod cache;

use std::collections::hash_map::Entry;
use std::collections::{HashMap, VecDeque};
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread;
use std::time::Duration;

use serde::Deserialize;
use workflint_diagnostics::quote::quote;
use workflint_rules::tools::{
    ExternalTools, ScriptShell, ShellScript, ShellcheckFinding, ShellcheckOutcome,
};

pub use cache::default_cache_dir;

use crate::tool::{RunOptions, ToolCommand, ToolOutcome, ToolPool};
use cache::{DiskCache, KeyParts};

/// The most scripts that one shellcheck process checks.
pub const MAX_BATCH: usize = 64;

/// How shellcheck runs on the scripts of one project: the `shellcheck` keys and `tool-timeout`
/// of its config file, and the flags. [`ShellcheckOptions::default`] is the reference's: no
/// rcfile, no extra argument, no cache, and the timeout of the [`Tools`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct ShellcheckOptions {
    rcfile: Option<Rcfile>,
    args: Vec<String>,
    cache: bool,
    timeout: Option<Duration>,
}

/// An rcfile: its path and its text, which the cache key includes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Rcfile {
    path: PathBuf,
    text: Vec<u8>,
}

impl ShellcheckOptions {
    /// Options with the rcfile at `path`, whose text is `text`, in place of `--norc`.
    #[must_use]
    pub fn with_rcfile(self, path: PathBuf, text: Vec<u8>) -> Self {
        Self {
            rcfile: Some(Rcfile { path, text }),
            ..self
        }
    }

    /// Options with `args` after workflint's arguments.
    #[must_use]
    pub fn with_args(self, args: Vec<String>) -> Self {
        Self { args, ..self }
    }

    /// Options that use the on-disk cache, if `cache` is set.
    #[must_use]
    pub fn with_cache(self, cache: bool) -> Self {
        Self { cache, ..self }
    }

    /// Options with `timeout` for each process, in place of the timeout of the [`Tools`].
    #[must_use]
    pub fn with_timeout(self, timeout: Option<Duration>) -> Self {
        Self { timeout, ..self }
    }

    /// The arguments of shellcheck for a script of `shell`, before the script: `--norc` or
    /// `--rcfile PATH`, the reference's arguments, and the extra arguments.
    fn arguments(&self, shell: ScriptShell) -> Vec<OsString> {
        let mut args: Vec<OsString> = match &self.rcfile {
            Some(rcfile) => vec!["--rcfile".into(), rcfile.path.clone().into_os_string()],
            None => vec!["--norc".into()],
        };
        args.extend(shell.shellcheck_args().into_iter().map(OsString::from));
        args.extend(self.args.iter().map(OsString::from));
        args
    }
}

/// The external tools of a run. [`Tools::none`] runs none.
#[derive(Debug, Default)]
pub struct Tools {
    shellcheck: Option<Shellcheck>,
}

impl Tools {
    /// No external tool.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// Runs shellcheck with `command` in `pool`, with `options` for each process unless the
    /// [`ShellcheckOptions`] of a project set the timeout.
    #[must_use]
    pub fn with_shellcheck(
        self,
        command: ToolCommand,
        pool: Arc<ToolPool>,
        options: RunOptions,
    ) -> Self {
        Self {
            shellcheck: Some(Shellcheck {
                command,
                pool,
                options,
                cache: None,
                env_options: std::env::var_os("SHELLCHECK_OPTS"),
                version: OnceLock::new(),
                queue: Mutex::default(),
                changed: Condvar::new(),
            }),
        }
    }

    /// Keeps the cache of shellcheck in `dir`/`shellcheck`, for the projects whose options turn
    /// it on. `dir` is usually [`default_cache_dir`]. Without a directory, no project uses the
    /// cache.
    #[must_use]
    pub fn with_cache_dir(mut self, dir: PathBuf) -> Self {
        if let Some(shellcheck) = &mut self.shellcheck {
            shellcheck.cache = Some(DiskCache::new(dir.join("shellcheck")));
        }
        self
    }

    /// Whether shellcheck runs.
    #[must_use]
    pub fn has_shellcheck(&self) -> bool {
        self.shellcheck.is_some()
    }

    /// The tools for the files of a project with `options`.
    pub(crate) fn for_project<'t>(
        &'t self,
        options: &'t Arc<ShellcheckOptions>,
    ) -> ProjectTools<'t> {
        ProjectTools {
            tools: self,
            options,
        }
    }

    /// Queues `scripts`, of a file of a project with `options`, without waiting for them. The
    /// threads that wait for outcomes check them.
    pub(crate) fn prefetch(&self, options: &Arc<ShellcheckOptions>, scripts: &[ShellScript]) {
        if let Some(shellcheck) = &self.shellcheck {
            shellcheck.enqueue(options, scripts);
        }
    }
}

/// The tools with the defaults of [`ShellcheckOptions`], for a caller without a project.
impl ExternalTools for Tools {
    fn has_shellcheck(&self) -> bool {
        self.shellcheck.is_some()
    }

    fn shellcheck(&self, scripts: &[ShellScript]) -> Vec<Arc<ShellcheckOutcome>> {
        let options = Arc::new(ShellcheckOptions::default());
        self.for_project(&options).shellcheck(scripts)
    }
}

/// The [`Tools`] of a run with the [`ShellcheckOptions`] of one project.
#[derive(Debug)]
pub(crate) struct ProjectTools<'t> {
    tools: &'t Tools,
    options: &'t Arc<ShellcheckOptions>,
}

impl ExternalTools for ProjectTools<'_> {
    fn has_shellcheck(&self) -> bool {
        self.tools.shellcheck.is_some()
    }

    fn shellcheck(&self, scripts: &[ShellScript]) -> Vec<Arc<ShellcheckOutcome>> {
        self.tools
            .shellcheck
            .as_ref()
            .map_or_else(Vec::new, |shellcheck| {
                shellcheck.check(self.options, scripts)
            })
    }
}

/// The outcome of one script, which the thread that checks it fills.
type Cell = Arc<OnceLock<Arc<ShellcheckOutcome>>>;

/// What identifies a script in a run.
#[derive(PartialEq, Eq, Hash)]
struct Key {
    shell: ScriptShell,
    options: Arc<ShellcheckOptions>,
    text: String,
}

/// A script that waits to be checked.
struct Job {
    shell: ScriptShell,
    options: Arc<ShellcheckOptions>,
    text: String,
    cell: Cell,
    /// The key of its cache entry, if its options use the cache.
    cache_key: Option<String>,
}

impl Job {
    /// Whether `other` can run in the same process.
    fn batches_with(&self, other: &Self) -> bool {
        self.shell == other.shell && self.options == other.options
    }
}

/// The outcome of every script of the run, and the scripts that wait.
#[derive(Default)]
struct Queue {
    outcomes: HashMap<Key, Cell>,
    waiting: VecDeque<Job>,
}

impl Queue {
    /// Takes the first waiting script and up to [`MAX_BATCH`] in all that batch with it, in
    /// queue order. It takes a share of those scripts, so that `limit` processes split them.
    fn take_batch(&mut self, limit: usize) -> Vec<Job> {
        let Some(first) = self.waiting.pop_front() else {
            return Vec::new();
        };
        let alike = self
            .waiting
            .iter()
            .filter(|job| job.batches_with(&first))
            .count();
        let size = (alike + 1).div_ceil(limit.max(1)).clamp(1, MAX_BATCH);
        let mut batch = Vec::with_capacity(size);
        let mut rest = VecDeque::with_capacity(self.waiting.len());
        for job in self.waiting.drain(..) {
            if batch.len() + 1 < size && job.batches_with(&first) {
                batch.push(job);
            } else {
                rest.push_back(job);
            }
        }
        self.waiting = rest;
        batch.insert(0, first);
        batch
    }
}

/// shellcheck, and the outcome of each script it has been given in the run.
struct Shellcheck {
    command: ToolCommand,
    pool: Arc<ToolPool>,
    options: RunOptions,
    cache: Option<DiskCache>,
    /// `SHELLCHECK_OPTS`, which shellcheck reads, so that the cache key includes it.
    env_options: Option<OsString>,
    /// The output of `shellcheck --version`, asked the first time the cache is used, or `None`
    /// if it failed.
    version: OnceLock<Option<String>>,
    queue: Mutex<Queue>,
    /// Notified when a script is queued or an outcome is filled.
    changed: Condvar,
}

impl fmt::Debug for Shellcheck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Shellcheck")
            .field("command", &self.command)
            .field("options", &self.options)
            .field("cache", &self.cache)
            .finish_non_exhaustive()
    }
}

impl Shellcheck {
    fn lock(&self) -> MutexGuard<'_, Queue> {
        // A panic while the lock is held leaves the queue as it was, so it stays usable.
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Checks each of `scripts` that has no outcome yet, and returns the outcome of each. The
    /// calling thread and up to one helper thread per slot of the pool check waiting scripts,
    /// these and any others, until these have outcomes.
    fn check(
        &self,
        options: &Arc<ShellcheckOptions>,
        scripts: &[ShellScript],
    ) -> Vec<Arc<ShellcheckOutcome>> {
        let cells = self.enqueue(options, scripts);
        let open = cells.iter().filter(|cell| cell.get().is_none()).count();
        let workers = open.min(self.pool.limit().get());
        thread::scope(|scope| {
            for _ in 1..workers {
                // Without another thread, this one does all the work.
                let spawned = thread::Builder::new()
                    .name("workflint-shellcheck".to_owned())
                    .spawn_scoped(scope, || self.work_until(&cells));
                if spawned.is_err() {
                    break;
                }
            }
            self.work_until(&cells);
        });
        cells
            .iter()
            .map(|cell| {
                cell.get().cloned().unwrap_or_else(|| {
                    Arc::new(ShellcheckOutcome::Failed(
                        "shellcheck was not run on this script".to_owned(),
                    ))
                })
            })
            .collect()
    }

    /// Returns the cell of each of `scripts`, and queues each script that has none yet, unless
    /// the cache has its findings.
    fn enqueue(&self, options: &Arc<ShellcheckOptions>, scripts: &[ShellScript]) -> Vec<Cell> {
        let mut cells = Vec::with_capacity(scripts.len());
        let mut new = Vec::new();
        {
            let mut queue = self.lock();
            for script in scripts {
                let key = Key {
                    shell: script.shell,
                    options: Arc::clone(options),
                    text: script.text.clone(),
                };
                match queue.outcomes.entry(key) {
                    Entry::Occupied(entry) => cells.push(Arc::clone(entry.get())),
                    Entry::Vacant(entry) => {
                        let cell = Cell::default();
                        entry.insert(Arc::clone(&cell));
                        cells.push(Arc::clone(&cell));
                        new.push(Job {
                            shell: script.shell,
                            options: Arc::clone(options),
                            text: script.text.clone(),
                            cell,
                            cache_key: None,
                        });
                    }
                }
            }
        }
        if new.is_empty() {
            return cells;
        }
        let mut misses = Vec::with_capacity(new.len());
        for mut job in new {
            if let Some((cache, key)) = self.cache_key(&job) {
                if let Some(findings) = cache.read(&key) {
                    let _ = job
                        .cell
                        .set(Arc::new(ShellcheckOutcome::Findings(findings)));
                    continue;
                }
                job.cache_key = Some(key);
            }
            misses.push(job);
        }
        self.lock().waiting.extend(misses);
        self.changed.notify_all();
        cells
    }

    /// The cache and the key of the entry of `job`, if its options use the cache and the
    /// version of shellcheck is known.
    fn cache_key(&self, job: &Job) -> Option<(&DiskCache, String)> {
        if !job.options.cache {
            return None;
        }
        let cache = self.cache.as_ref()?;
        let version = self.version()?;
        let args = job.options.arguments(job.shell);
        let all: Vec<&OsStr> = self
            .command
            .args()
            .iter()
            .chain(&args)
            .map(OsString::as_os_str)
            .collect();
        let key = KeyParts {
            version,
            args: &all,
            rcfile: job
                .options
                .rcfile
                .as_ref()
                .map_or(&[][..], |rcfile| &rcfile.text),
            env_options: self.env_options.as_deref(),
            script: &job.text,
        }
        .key();
        Some((cache, key))
    }

    /// The output of `shellcheck --version`, asked once per run.
    fn version(&self) -> Option<&str> {
        self.version
            .get_or_init(|| {
                match self
                    .pool
                    .run(&self.command, &["--version"], b"", &self.options)
                {
                    ToolOutcome::Exited {
                        code: 0, stdout, ..
                    } => String::from_utf8(stdout).ok(),
                    outcome => {
                        tracing::info!(
                            "The shellcheck cache is not used, because shellcheck --version                              failed: {outcome:?}"
                        );
                        None
                    }
                }
            })
            .as_deref()
    }

    /// Checks waiting scripts until each of `cells` has an outcome.
    fn work_until(&self, cells: &[Cell]) {
        loop {
            let batch = {
                let mut queue = self.lock();
                loop {
                    if cells.iter().all(|cell| cell.get().is_some()) {
                        return;
                    }
                    let batch = queue.take_batch(self.pool.limit().get());
                    if !batch.is_empty() {
                        break batch;
                    }
                    queue = self
                        .changed
                        .wait(queue)
                        .unwrap_or_else(PoisonError::into_inner);
                }
            };
            let ran = panic::catch_unwind(AssertUnwindSafe(|| self.run_batch(&batch)));
            // A panic leaves no script of the batch without an outcome, so no thread waits for
            // it forever. The driver reports the panic itself.
            for job in &batch {
                let _ = job.cell.set(Arc::new(ShellcheckOutcome::Failed(
                    "shellcheck was not run on this script".to_owned(),
                )));
            }
            drop(self.lock());
            self.changed.notify_all();
            if let Err(payload) = ran {
                panic::resume_unwind(payload);
            }
        }
    }

    /// Checks the scripts of `batch`, which share a shell and options, in one process, or each
    /// alone if there is one or the process fails.
    fn run_batch(&self, batch: &[Job]) {
        if let [job] = batch {
            self.run_alone(job);
            return;
        }
        match self.run_together(batch) {
            Some(findings) => {
                for (job, findings) in batch.iter().zip(findings) {
                    self.finish(job, ShellcheckOutcome::Findings(findings));
                }
            }
            None => {
                for job in batch {
                    self.run_alone(job);
                }
            }
        }
    }

    /// The options of a process for scripts with `options`.
    fn run_options(&self, options: &ShellcheckOptions) -> RunOptions {
        RunOptions {
            timeout: options.timeout.unwrap_or(self.options.timeout),
            ..self.options.clone()
        }
    }

    /// Runs shellcheck on `job` alone, with the script on stdin.
    fn run_alone(&self, job: &Job) {
        let mut args = job.options.arguments(job.shell);
        args.push("-".into());
        tracing::debug!(
            "Running {} {} on a {} script",
            self.command.program().display(),
            join(&args),
            job.shell.name()
        );
        let ran = self.pool.run(
            &self.command,
            &args,
            job.text.as_bytes(),
            &self.run_options(&job.options),
        );
        self.finish(job, outcome(ran));
    }

    /// Runs shellcheck once on the scripts of `batch`, each in a file, and returns the findings
    /// of each. Returns `None` if the files cannot be written, the process fails, or its output
    /// is not JSON whose findings name the files.
    fn run_together(&self, batch: &[Job]) -> Option<Vec<Vec<ShellcheckFinding>>> {
        let first = batch.first()?;
        let dir = match tempfile::Builder::new().prefix("workflint-").tempdir() {
            Ok(dir) => dir,
            Err(err) => {
                tracing::debug!("Could not create a directory for a shellcheck batch: {err}");
                return None;
            }
        };
        let mut files = HashMap::with_capacity(batch.len());
        let mut args = first.options.arguments(first.shell);
        for (index, job) in batch.iter().enumerate() {
            let path = dir.path().join(index.to_string());
            if let Err(err) = fs::write(&path, &job.text) {
                tracing::debug!("Could not write a script for a shellcheck batch: {err}");
                return None;
            }
            // shellcheck names each file as its argument is written.
            files.insert(path.to_str()?.to_owned(), index);
            args.push(path.into_os_string());
        }
        tracing::debug!(
            "Running {} {} on {} {} scripts",
            self.command.program().display(),
            join(&args[..args.len() - batch.len()]),
            batch.len(),
            first.shell.name()
        );
        let ran = self
            .pool
            .run(&self.command, &args, b"", &self.run_options(&first.options));
        let stdout = match ran {
            // shellcheck exits with 1 when it reports findings, and with another status when it
            // could not check a file.
            ToolOutcome::Exited {
                code: 0 | 1,
                stdout,
                ..
            } => stdout,
            ran => {
                tracing::debug!(
                    "A shellcheck batch failed, so each of its scripts runs alone: {ran:?}"
                );
                return None;
            }
        };
        let parsed = match serde_json::from_slice::<Option<Vec<JsonFinding>>>(&stdout) {
            Ok(parsed) => parsed.unwrap_or_default(),
            Err(err) => {
                tracing::debug!(
                    "The output of a shellcheck batch is not its JSON, so each of its scripts                      runs alone: {err}"
                );
                return None;
            }
        };
        let mut findings = vec![Vec::new(); batch.len()];
        for finding in parsed {
            let index = *files.get(&finding.file)?;
            findings.get_mut(index)?.push(finding.into_finding());
        }
        Some(findings)
    }

    /// Fills the outcome of `job`, and stores its findings in the cache if it uses it.
    fn finish(&self, job: &Job, outcome: ShellcheckOutcome) {
        if let (Some(cache), Some(key), ShellcheckOutcome::Findings(findings)) =
            (&self.cache, &job.cache_key, &outcome)
        {
            cache.write(key, findings);
        }
        let _ = job.cell.set(Arc::new(outcome));
    }
}

/// `args` joined with spaces, for the log.
fn join(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ")
}

/// One finding in shellcheck's JSON output. A missing field is zero or empty, as Go's
/// `json.Unmarshal` leaves it in the reference.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonFinding {
    /// The file, `-` for stdin.
    #[serde(default)]
    file: String,
    #[serde(default)]
    line: i64,
    #[serde(default)]
    column: i64,
    end_line: Option<i64>,
    end_column: Option<i64>,
    #[serde(default)]
    level: String,
    #[serde(default)]
    code: i64,
    #[serde(default)]
    message: String,
}

impl JsonFinding {
    fn into_finding(self) -> ShellcheckFinding {
        ShellcheckFinding {
            line: self.line,
            column: self.column,
            end_line: self.end_line,
            end_column: self.end_column,
            level: self.level,
            code: self.code,
            message: self.message,
        }
    }
}

/// Turns the outcome of a shellcheck process into findings or the message of a failure.
pub(crate) fn outcome(outcome: ToolOutcome) -> ShellcheckOutcome {
    let failed = |detail: String| {
        ShellcheckOutcome::Failed(format!(
            "shellcheck did not run successfully while checking this script: {detail}"
        ))
    };
    let stderr = |stderr: &[u8]| quote(&String::from_utf8_lossy(stderr));
    match outcome {
        ToolOutcome::Exited { stdout, .. } => {
            // The reference's `json.Unmarshal` reads `null` as no findings.
            match serde_json::from_slice::<Option<Vec<JsonFinding>>>(&stdout) {
                Ok(findings) => ShellcheckOutcome::Findings(
                    findings
                        .unwrap_or_default()
                        .into_iter()
                        .map(JsonFinding::into_finding)
                        .collect(),
                ),
                Err(err) => ShellcheckOutcome::Failed(format!(
                    "could not parse JSON output from shellcheck: {err}: stdout={}",
                    quote(&String::from_utf8_lossy(&stdout))
                )),
            }
        }
        ToolOutcome::FailedWithoutOutput { code, stderr: err } => failed(format!(
            "shellcheck exited with status {code} but stdout was empty. stderr: {}",
            stderr(&err)
        )),
        ToolOutcome::Signaled {
            signal: Some(signal),
            stderr: err,
        } => failed(format!(
            "shellcheck was terminated by signal {signal}. stderr: {}",
            stderr(&err)
        )),
        ToolOutcome::Signaled {
            signal: None,
            stderr: err,
        } => failed(format!(
            "shellcheck was terminated. stderr: {}",
            stderr(&err)
        )),
        ToolOutcome::TimedOut { timeout } => failed(format!(
            "shellcheck did not finish in {} seconds and was killed",
            timeout.as_secs_f64()
        )),
        ToolOutcome::SpawnFailed(err) => failed(format!("shellcheck could not be started: {err}")),
        ToolOutcome::IoFailed(err) => failed(format!(
            "reading the output of shellcheck or writing the script to it failed: {err}"
        )),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use std::io;
    use std::time::Duration;

    use super::*;

    fn exited(stdout: &str) -> ToolOutcome {
        ToolOutcome::Exited {
            code: 1,
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        }
    }

    fn failure(outcome: ToolOutcome) -> String {
        match super::outcome(outcome) {
            ShellcheckOutcome::Failed(message) => message,
            ShellcheckOutcome::Findings(findings) => panic!("findings: {findings:?}"),
        }
    }

    fn job(shell: ScriptShell, options: &Arc<ShellcheckOptions>, text: &str) -> Job {
        Job {
            shell,
            options: Arc::clone(options),
            text: text.to_owned(),
            cell: Cell::default(),
            cache_key: None,
        }
    }

    /// The sizes of the batches that `limit` processes take from `queue` until it is empty, with
    /// the first script of each.
    fn batches(mut queue: Queue, limit: usize) -> Vec<(String, usize)> {
        let mut out = Vec::new();
        loop {
            let batch = queue.take_batch(limit);
            let Some(first) = batch.first() else {
                return out;
            };
            out.push((first.text.clone(), batch.len()));
        }
    }

    #[test]
    fn a_batch_takes_a_share_of_the_alike_scripts_and_at_most_max_batch() {
        let options = Arc::new(ShellcheckOptions::default());
        let queue = |count: usize| Queue {
            outcomes: HashMap::new(),
            waiting: (0..count)
                .map(|i| job(ScriptShell::Bash, &options, &i.to_string()))
                .collect(),
        };
        let sizes = |queue, limit| -> Vec<usize> {
            batches(queue, limit)
                .into_iter()
                .map(|(_, size)| size)
                .collect()
        };
        assert_eq!(sizes(queue(10), 4), [3, 2, 2, 1, 1, 1]);
        assert_eq!(sizes(queue(10), 1), [10]);
        assert_eq!(sizes(queue(200), 1), [64, 64, 64, 8]);
        assert_eq!(sizes(queue(0), 1), Vec::<usize>::new());
    }

    #[test]
    fn a_batch_holds_one_shell_and_one_set_of_options_in_queue_order() {
        let plain = Arc::new(ShellcheckOptions::default());
        let other = Arc::new(ShellcheckOptions::default().with_args(vec!["-o".to_owned()]));
        let same = Arc::new(ShellcheckOptions::default());
        let queue = Queue {
            outcomes: HashMap::new(),
            waiting: [
                job(ScriptShell::Bash, &plain, "a"),
                job(ScriptShell::Sh, &plain, "b"),
                job(ScriptShell::Bash, &other, "c"),
                job(ScriptShell::Bash, &same, "d"),
                job(ScriptShell::Sh, &plain, "e"),
            ]
            .into(),
        };
        assert_eq!(
            batches(queue, 1),
            [
                ("a".to_owned(), 2),
                ("b".to_owned(), 2),
                ("c".to_owned(), 1)
            ]
        );
    }

    #[test]
    fn the_options_give_norc_or_the_rcfile_then_the_reference_arguments_then_the_extra_ones() {
        let strings = |args: Vec<OsString>| -> Vec<String> {
            args.into_iter()
                .map(|arg| arg.into_string().unwrap())
                .collect()
        };
        let excluded = "SC1091,SC2194,SC2050,SC2153,SC2154,SC2157,SC2043";
        assert_eq!(
            strings(ShellcheckOptions::default().arguments(ScriptShell::Sh)),
            [
                "--norc", "-f", "json", "-x", "--shell", "sh", "-e", excluded
            ]
        );
        let options = ShellcheckOptions::default()
            .with_rcfile(PathBuf::from("/r/.shellcheckrc"), Vec::new())
            .with_args(vec!["-S".to_owned(), "warning".to_owned()]);
        assert_eq!(
            strings(options.arguments(ScriptShell::Bash)),
            [
                "--rcfile",
                "/r/.shellcheckrc",
                "-f",
                "json",
                "-x",
                "--shell",
                "bash",
                "-e",
                excluded,
                "-S",
                "warning"
            ]
        );
    }

    #[test]
    fn the_json_output_becomes_findings() {
        let json = r#"[{"file":"-","line":2,"endLine":2,"column":6,"endColumn":10,"level":"info","code":2086,"message":"Double quote.","fix":null}]"#;
        assert_eq!(
            super::outcome(exited(json)),
            ShellcheckOutcome::Findings(vec![ShellcheckFinding {
                line: 2,
                column: 6,
                end_line: Some(2),
                end_column: Some(10),
                level: "info".to_owned(),
                code: 2086,
                message: "Double quote.".to_owned(),
            }])
        );
    }

    #[test]
    fn an_empty_array_or_null_is_no_finding() {
        for json in ["[]", "null", " [ ]\n"] {
            assert_eq!(
                super::outcome(exited(json)),
                ShellcheckOutcome::Findings(Vec::new()),
                "{json}"
            );
        }
    }

    #[test]
    fn missing_fields_are_zero_or_empty() {
        assert_eq!(
            super::outcome(exited("[{}]")),
            ShellcheckOutcome::Findings(vec![ShellcheckFinding {
                line: 0,
                column: 0,
                end_line: None,
                end_column: None,
                level: String::new(),
                code: 0,
                message: String::new(),
            }])
        );
    }

    #[test]
    fn output_that_is_not_json_is_a_failure_with_the_output() {
        assert_eq!(
            failure(exited("oops\n")),
            "could not parse JSON output from shellcheck: expected value at line 1 column 1: \
             stdout=\"oops\\n\""
        );
        assert!(failure(exited("")).starts_with("could not parse JSON output from shellcheck: "));
    }

    #[test]
    fn each_failure_of_the_process_has_its_message() {
        let prefix = "shellcheck did not run successfully while checking this script: ";
        let cases = [
            (
                ToolOutcome::FailedWithoutOutput {
                    code: 2,
                    stderr: b"bad\n".to_vec(),
                },
                "shellcheck exited with status 2 but stdout was empty. stderr: \"bad\\n\"",
            ),
            (
                ToolOutcome::Signaled {
                    signal: Some(9),
                    stderr: Vec::new(),
                },
                "shellcheck was terminated by signal 9. stderr: \"\"",
            ),
            (
                ToolOutcome::Signaled {
                    signal: None,
                    stderr: Vec::new(),
                },
                "shellcheck was terminated. stderr: \"\"",
            ),
            (
                ToolOutcome::TimedOut {
                    timeout: Duration::from_millis(1500),
                },
                "shellcheck did not finish in 1.5 seconds and was killed",
            ),
            (
                ToolOutcome::SpawnFailed(io::Error::other("no such file")),
                "shellcheck could not be started: no such file",
            ),
            (
                ToolOutcome::IoFailed(io::Error::other("broken pipe")),
                "reading the output of shellcheck or writing the script to it failed: broken pipe",
            ),
        ];
        for (outcome, detail) in cases {
            assert_eq!(failure(outcome), format!("{prefix}{detail}"));
        }
    }
}
