//! The running tool processes, and the handler that kills them on SIGINT or SIGTERM.
//!
//! Every process is spawned and registered while the registry is locked, and the handler kills
//! the registered processes under the same lock, so no process starts between the two.

use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::process::{self, Child, ChildStderr, ChildStdin, ChildStdout, Command};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

/// The exit code after SIGINT or SIGTERM: 128 plus the number of SIGINT, as shells report it.
pub const INTERRUPTED_EXIT_CODE: i32 = 130;

/// The handler for SIGINT and SIGTERM could not be installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterruptHandlerError(String);

impl fmt::Display for InterruptHandlerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "could not install the handler for SIGINT and SIGTERM: {}",
            self.0
        )
    }
}

impl std::error::Error for InterruptHandlerError {}

/// A process that may be killed by the handler. Its standard streams are taken out before it is
/// shared.
pub(super) type SharedChild = Arc<Mutex<Child>>;

struct Registry {
    exiting: bool,
    next_id: u64,
    children: BTreeMap<u64, SharedChild>,
}

static REGISTRY: Mutex<Registry> = Mutex::new(Registry {
    exiting: false,
    next_id: 0,
    children: BTreeMap::new(),
});

/// Locks a mutex whose holder panicked too. The data behind these locks stays consistent at
/// every point a panic could happen.
pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Installs the handler once per process.
pub(super) fn install() -> Result<(), InterruptHandlerError> {
    static INSTALLED: OnceLock<Result<(), InterruptHandlerError>> = OnceLock::new();
    INSTALLED
        .get_or_init(|| {
            ctrlc::try_set_handler(kill_children_and_exit)
                .map_err(|error| InterruptHandlerError(error.to_string()))
        })
        .clone()
}

fn kill_children_and_exit() {
    let mut registry = lock(&REGISTRY);
    registry.exiting = true;
    for child in registry.children.values() {
        let mut child = lock(child);
        // Either call fails only when the process has already exited and been waited for, which
        // is the state this wants.
        let _ = child.kill();
        let _ = child.wait();
    }
    process::exit(INTERRUPTED_EXIT_CODE);
}

/// A registered process. Dropping it unregisters the process.
pub(super) struct Registration {
    id: u64,
    pub(super) child: SharedChild,
}

impl Drop for Registration {
    fn drop(&mut self) {
        lock(&REGISTRY).children.remove(&self.id);
    }
}

/// A spawned and registered process, with the standard streams that were piped.
pub(super) struct Spawned {
    pub(super) registration: Registration,
    pub(super) stdin: Option<ChildStdin>,
    pub(super) stdout: Option<ChildStdout>,
    pub(super) stderr: Option<ChildStderr>,
}

/// Spawns `command` and registers the process, unless the handler is already killing processes.
/// `command` is dropped before this returns, which closes the parent's copies of the pipe ends
/// given to the child.
pub(super) fn spawn(mut command: Command) -> io::Result<Spawned> {
    let mut registry = lock(&REGISTRY);
    if registry.exiting {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "workflint is exiting after SIGINT or SIGTERM",
        ));
    }
    let mut child = command.spawn()?;
    drop(command);
    let (stdin, stdout, stderr) = (child.stdin.take(), child.stdout.take(), child.stderr.take());
    let id = registry.next_id;
    registry.next_id += 1;
    let child = Arc::new(Mutex::new(child));
    registry.children.insert(id, Arc::clone(&child));
    Ok(Spawned {
        registration: Registration { id, child },
        stdin,
        stdout,
        stderr,
    })
}
