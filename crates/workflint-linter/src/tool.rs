//! Running external tools such as shellcheck and pyflakes (ADR 0012).
//!
//! [`ToolCommand::resolve`] finds the executable as the reference does, and [`ToolPool::run`]
//! runs it on a script. A run cannot hang: stdin is written from its own thread while other
//! threads read stdout and stderr, and the process is killed when its timeout passes. A pool
//! runs at most its limit of processes at once, separately from the threads that lint files
//! (ADR 0013). The first pool installs a handler that kills every running tool process and exits
//! with [`INTERRUPTED_EXIT_CODE`] on SIGINT or SIGTERM.
//!
//! The child gets the parent's environment, so `SHELLCHECK_OPTS` works, and only the three
//! standard streams. The standard library opens every other file and pipe without inheritance.

mod interrupt;
mod resolve;

use std::ffi::{OsStr, OsString};
use std::io::{self, PipeReader, Read, Write};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub use interrupt::{INTERRUPTED_EXIT_CODE, InterruptHandlerError};
pub use resolve::ResolveError;

use interrupt::lock;

/// How long a tool process may run unless the caller sets another timeout.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// A resolved external tool: the executable and the arguments that come before those of each run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCommand {
    program: PathBuf,
    args: Vec<OsString>,
}

impl ToolCommand {
    /// Resolves the value of a flag such as `-shellcheck` as the reference's
    /// `resolveExternalCommand` does, with the `PATH` of the environment.
    ///
    /// # Errors
    ///
    /// [`ResolveError::NotFound`] when no executable matches.
    pub fn resolve(spec: &str) -> Result<Self, ResolveError> {
        Self::resolve_with_path(spec, std::env::var_os("PATH").as_deref())
    }

    /// Resolves `spec` with `path_var` as the value of `PATH`.
    ///
    /// A value with a path separator names the executable directly. A bare name is looked up in
    /// the absolute directories of `path_var`, never in the current directory. If neither finds
    /// an executable, the value is split as a command line, the first word is looked up the same
    /// way, and the other words come before the arguments of each run.
    ///
    /// # Errors
    ///
    /// [`ResolveError::NotFound`] when no executable matches.
    pub fn resolve_with_path(spec: &str, path_var: Option<&OsStr>) -> Result<Self, ResolveError> {
        resolve::resolve(spec, path_var)
    }

    /// The path of the executable.
    #[must_use]
    pub fn program(&self) -> &Path {
        &self.program
    }

    /// The arguments split off the resolved value, given before the arguments of each run.
    #[must_use]
    pub fn args(&self) -> &[OsString] {
        &self.args
    }
}

/// Options of one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOptions {
    /// How long the process may run before it is killed.
    pub timeout: Duration,
    /// Give stdout and stderr to the process as one pipe, so that the output keeps the order the
    /// process wrote in, as Go's `CombinedOutput` does. The output is then in
    /// [`ToolOutcome::Exited::stdout`] and stderr is empty.
    pub combine_output: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            combine_output: false,
        }
    }
}

/// How a run ended. The caller turns each case into findings or a diagnostic.
#[derive(Debug)]
pub enum ToolOutcome {
    /// The process exited with status 0, or with another status and some output on stdout. The
    /// reference treats both as the tool's findings.
    Exited {
        /// The exit status.
        code: i32,
        /// Everything the process wrote to stdout.
        stdout: Vec<u8>,
        /// Everything the process wrote to stderr.
        stderr: Vec<u8>,
    },
    /// The process exited with a status other than 0 and wrote nothing to stdout.
    FailedWithoutOutput {
        /// The exit status.
        code: i32,
        /// Everything the process wrote to stderr.
        stderr: Vec<u8>,
    },
    /// A signal killed the process before it exited.
    Signaled {
        /// The signal number, when the OS reports one.
        signal: Option<i32>,
        /// Everything the process wrote to stderr.
        stderr: Vec<u8>,
    },
    /// The process ran longer than the timeout and was killed.
    TimedOut {
        /// The timeout that passed.
        timeout: Duration,
    },
    /// The process could not be started.
    SpawnFailed(io::Error),
    /// Writing stdin or reading the output failed in a way other than the process exiting, and
    /// the process was killed.
    IoFailed(io::Error),
}

/// Runs external tool processes, at most `limit` at once across all threads that share the pool.
#[derive(Debug)]
pub struct ToolPool {
    limit: NonZeroUsize,
    running: Mutex<usize>,
    freed: Condvar,
}

impl ToolPool {
    /// A pool that runs at most `limit` processes at once. It installs the handler for SIGINT and
    /// SIGTERM if no pool has yet.
    ///
    /// # Errors
    ///
    /// [`InterruptHandlerError`] when another handler for the signals is installed.
    pub fn new(limit: NonZeroUsize) -> Result<Self, InterruptHandlerError> {
        interrupt::install()?;
        Ok(Self {
            limit,
            running: Mutex::new(0),
            freed: Condvar::new(),
        })
    }

    /// A pool whose limit is [`thread::available_parallelism`], or 1 when that is unknown.
    ///
    /// # Errors
    ///
    /// As [`ToolPool::new`].
    pub fn with_available_parallelism() -> Result<Self, InterruptHandlerError> {
        Self::new(thread::available_parallelism().unwrap_or(NonZeroUsize::MIN))
    }

    /// The most processes that run at once.
    #[must_use]
    pub fn limit(&self) -> NonZeroUsize {
        self.limit
    }

    /// Runs `command` with its own arguments, then `args`, and `stdin` as the standard input. It
    /// waits for a free slot first, and returns when the process has exited or been killed.
    pub fn run<S: AsRef<OsStr>>(
        &self,
        command: &ToolCommand,
        args: &[S],
        stdin: &[u8],
        options: &RunOptions,
    ) -> ToolOutcome {
        let _slot = self.acquire();
        run(command, args, stdin, options)
    }

    fn acquire(&self) -> Slot<'_> {
        let mut running = lock(&self.running);
        while *running >= self.limit.get() {
            running = self
                .freed
                .wait(running)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        *running += 1;
        Slot(self)
    }
}

/// A running process's place in a pool, given back on drop.
struct Slot<'a>(&'a ToolPool);

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        *lock(&self.0.running) -= 1;
        self.0.freed.notify_one();
    }
}

/// Which pipe a reader thread read.
#[derive(Clone, Copy)]
enum Stream {
    Stdout,
    Stderr,
}

type ReadResult = (Stream, io::Result<Vec<u8>>);

fn run<S: AsRef<OsStr>>(
    command: &ToolCommand,
    args: &[S],
    stdin: &[u8],
    options: &RunOptions,
) -> ToolOutcome {
    let (process, combined) = match build(command, args, options.combine_output) {
        Ok(built) => built,
        Err(error) => return ToolOutcome::SpawnFailed(error),
    };
    let spawned = match interrupt::spawn(process) {
        Ok(spawned) => spawned,
        Err(error) => return ToolOutcome::SpawnFailed(error),
    };
    // A timeout too long for an `Instant` never passes.
    let deadline = Instant::now().checked_add(options.timeout);
    let remaining = || {
        deadline.map_or(Duration::MAX, |d| {
            d.saturating_duration_since(Instant::now())
        })
    };
    let child = &spawned.registration.child;
    let kill = |outcome: ToolOutcome| {
        let mut child = lock(child);
        // Either call fails only when the process has already exited and been waited for.
        let _ = child.kill();
        let _ = child.wait();
        outcome
    };

    let (sender, receiver) = mpsc::channel::<ReadResult>();
    let mut readers = 0;
    let start = || -> io::Result<()> {
        if let Some(mut pipe) = spawned.stdin {
            let input = stdin.to_vec();
            // A write error means the process closed stdin or exited without reading all of it.
            // Its exit status and output tell what happened.
            spawn_thread(move || {
                let _ = pipe.write_all(&input);
            })?;
        }
        let pipes: [(Stream, Option<Box<dyn Read + Send>>); 2] = match combined {
            Some(reader) => [
                (Stream::Stdout, Some(Box::new(reader))),
                (Stream::Stderr, None),
            ],
            None => [
                (Stream::Stdout, spawned.stdout.map(|p| Box::new(p) as _)),
                (Stream::Stderr, spawned.stderr.map(|p| Box::new(p) as _)),
            ],
        };
        for (stream, pipe) in pipes {
            let Some(mut pipe) = pipe else { continue };
            let sender = sender.clone();
            spawn_thread(move || {
                let mut output = Vec::new();
                let result = pipe.read_to_end(&mut output).map(|_| output);
                // The receiver is gone only when the run has already ended.
                let _ = sender.send((stream, result));
            })?;
            readers += 1;
        }
        Ok(())
    };
    if let Err(error) = start() {
        return kill(ToolOutcome::IoFailed(error));
    }
    drop(sender);

    let timed_out = || ToolOutcome::TimedOut {
        timeout: options.timeout,
    };
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    for _ in 0..readers {
        let remaining = remaining();
        match receiver.recv_timeout(remaining) {
            Ok((stream, Ok(output))) => match stream {
                Stream::Stdout => stdout = output,
                Stream::Stderr => stderr = output,
            },
            Ok((_, Err(error))) => return kill(ToolOutcome::IoFailed(error)),
            Err(RecvTimeoutError::Timeout) => return kill(timed_out()),
            Err(RecvTimeoutError::Disconnected) => {
                return kill(ToolOutcome::IoFailed(io::Error::other(
                    "a thread reading the output of the process stopped",
                )));
            }
        }
    }

    // The process closed its output, so it has exited or is about to.
    let mut pause = Duration::from_millis(1);
    let status = loop {
        match lock(child).try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => return kill(ToolOutcome::IoFailed(error)),
        }
        let remaining = remaining();
        if remaining.is_zero() {
            return kill(timed_out());
        }
        thread::sleep(pause.min(remaining));
        pause = (pause * 2).min(Duration::from_millis(20));
    };
    outcome(status, stdout, stderr)
}

/// The process to spawn, and the read end of the pipe for both stdout and stderr if `combine`.
fn build<S: AsRef<OsStr>>(
    command: &ToolCommand,
    args: &[S],
    combine: bool,
) -> io::Result<(Command, Option<PipeReader>)> {
    let mut process = Command::new(&command.program);
    process.args(&command.args).args(args).stdin(Stdio::piped());
    if combine {
        let (reader, writer) = io::pipe()?;
        process.stdout(writer.try_clone()?).stderr(writer);
        Ok((process, Some(reader)))
    } else {
        process.stdout(Stdio::piped()).stderr(Stdio::piped());
        Ok((process, None))
    }
}

fn spawn_thread(f: impl FnOnce() + Send + 'static) -> io::Result<()> {
    thread::Builder::new()
        .name("workflint-tool-io".to_owned())
        .spawn(f)
        .map(drop)
}

fn outcome(status: ExitStatus, stdout: Vec<u8>, stderr: Vec<u8>) -> ToolOutcome {
    match status.code() {
        Some(code) if code != 0 && stdout.is_empty() => {
            ToolOutcome::FailedWithoutOutput { code, stderr }
        }
        Some(code) => ToolOutcome::Exited {
            code,
            stdout,
            stderr,
        },
        None => ToolOutcome::Signaled {
            signal: signal(status),
            stderr,
        },
    }
}

#[cfg(unix)]
fn signal(status: ExitStatus) -> Option<i32> {
    std::os::unix::process::ExitStatusExt::signal(&status)
}

#[cfg(not(unix))]
fn signal(_: ExitStatus) -> Option<i32> {
    None
}
