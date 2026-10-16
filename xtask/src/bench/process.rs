//! Runs a process with a time limit.

use std::process::{Child, Command, ExitStatus};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

/// How a process run with [`run`] ended.
#[derive(Debug)]
pub(crate) enum Finished {
    /// The process exited on its own.
    Exited(ExitStatus),
    /// The process ran past the time limit, and [`run`] killed it.
    TimedOut,
}

/// How often [`run`] checks whether the process has exited.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Runs `command` and waits at most `limit` for it to exit.
///
/// On Unix the process starts in a new process group, and at the limit the
/// whole group gets `SIGKILL`. That stops the children it started too, such as
/// the shellcheck processes of the reference. Elsewhere only the process
/// itself is killed.
pub(crate) fn run(command: &mut Command, limit: Duration) -> Result<Finished> {
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(command, 0);
    let program = command.get_program().to_owned();
    let start = Instant::now();
    let mut child = command
        .spawn()
        .with_context(|| format!("cannot run {}", program.display()))?;
    loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("cannot wait for {}", program.display()))?
        {
            return Ok(Finished::Exited(status));
        }
        if start.elapsed() >= limit {
            kill(&mut child);
            return Ok(Finished::TimedOut);
        }
        thread::sleep(POLL_INTERVAL);
    }
}

/// Kills `child` and, on Unix, its process group, then reaps it. Errors are
/// ignored: the process may have exited in the meantime.
fn kill(child: &mut Child) {
    #[cfg(unix)]
    {
        let group = format!("-{}", child.id());
        let _ = Command::new("kill").args(["-KILL", "--", &group]).status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(all(test, unix))]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn process_that_exits_in_time_reports_its_status() {
        let finished = run(
            Command::new("sh").args(["-c", "exit 3"]),
            Duration::from_secs(10),
        )
        .unwrap();
        let Finished::Exited(status) = finished else {
            panic!("expected an exit, got {finished:?}");
        };
        assert_eq!(status.code(), Some(3));
    }

    #[test]
    fn process_past_the_limit_times_out_without_waiting_for_it() {
        let start = Instant::now();
        let finished = run(Command::new("sleep").arg("30"), Duration::from_millis(200)).unwrap();
        assert!(matches!(finished, Finished::TimedOut), "{finished:?}");
        assert!(start.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn timeout_kills_the_children_of_the_process() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("pid");
        let script = format!("sleep 30 & echo $! > '{}'; wait", pid_file.display());
        let finished = run(
            Command::new("sh").args(["-c", &script]),
            Duration::from_millis(500),
        )
        .unwrap();
        assert!(matches!(finished, Finished::TimedOut), "{finished:?}");

        let pid = fs::read_to_string(&pid_file).unwrap();
        // The killed child may stay a zombie until init reaps it, so check
        // that it is no longer running rather than that it is gone.
        let state = Command::new("ps")
            .args(["-o", "stat=", "-p", pid.trim()])
            .output()
            .unwrap();
        let state = String::from_utf8_lossy(&state.stdout);
        assert!(
            state.trim().is_empty() || state.trim().starts_with('Z'),
            "child {} still runs with state {state:?}",
            pid.trim()
        );
    }

    #[test]
    fn missing_program_is_an_error() {
        let error = run(
            &mut Command::new("workflint-test-no-such-program"),
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert!(error.to_string().contains("cannot run"), "{error:#}");
    }
}
