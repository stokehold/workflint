//! Runs a process with a time limit.

use std::io::Read;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

/// How a process run with [`run`] or [`output`] ended.
#[derive(Debug)]
pub enum Finished {
    /// The process exited on its own.
    Exited(ExitStatus),
    /// The process ran past the time limit and was killed.
    TimedOut,
}

/// What a process run with [`output`] printed, and how it ended.
#[derive(Debug)]
pub struct Captured {
    /// How the process ended.
    pub finished: Finished,
    /// Its standard output, up to the moment it ended.
    pub stdout: Vec<u8>,
    /// Its standard error, up to the moment it ended.
    pub stderr: Vec<u8>,
}

/// How often [`run`] checks whether the process has exited.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Runs `command` and waits at most `limit` for it to exit.
///
/// On Unix the process starts in a new process group, and at the limit the
/// whole group gets `SIGKILL`. That stops the children it started too, such as
/// the shellcheck processes of the reference. Elsewhere only the process
/// itself is killed.
///
/// # Errors
///
/// Fails if the process cannot start, or if waiting for it fails.
pub fn run(command: &mut Command, limit: Duration) -> Result<Finished> {
    let mut child = spawn(command)?;
    wait(&mut child, command, limit)
}

/// Runs `command` with empty standard input, as [`run`] does, and returns
/// what it printed. A process that runs past `limit` is killed, and the
/// result holds what it printed until then.
///
/// # Errors
///
/// Fails if the process cannot start, or if waiting for it or reading its
/// output fails.
pub fn output(command: &mut Command, limit: Duration) -> Result<Captured> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = spawn(command)?;
    // Read both pipes while the process runs, so that a process that fills a
    // pipe does not block forever.
    let stdout = child.stdout.take().map(read_in_background);
    let stderr = child.stderr.take().map(read_in_background);
    let finished = wait(&mut child, command, limit)?;
    let program = command.get_program().display().to_string();
    let collect = |reader: Option<thread::JoinHandle<std::io::Result<Vec<u8>>>>| {
        reader.map_or_else(
            || Ok(Vec::new()),
            |reader| {
                reader
                    .join()
                    .map_err(|_| anyhow::anyhow!("the thread that reads {program} panicked"))?
                    .with_context(|| format!("cannot read the output of {program}"))
            },
        )
    };
    Ok(Captured {
        finished,
        stdout: collect(stdout)?,
        stderr: collect(stderr)?,
    })
}

fn read_in_background(
    mut pipe: impl Read + Send + 'static,
) -> thread::JoinHandle<std::io::Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        pipe.read_to_end(&mut bytes).map(|_| bytes)
    })
}

fn spawn(command: &mut Command) -> Result<Child> {
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(command, 0);
    command
        .spawn()
        .with_context(|| format!("cannot run {}", command.get_program().display()))
}

fn wait(child: &mut Child, command: &Command, limit: Duration) -> Result<Finished> {
    let start = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("cannot wait for {}", command.get_program().display()))?
        {
            return Ok(Finished::Exited(status));
        }
        if start.elapsed() >= limit {
            kill(child);
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

    #[test]
    fn output_captures_both_streams_and_the_exit_code() {
        let captured = output(
            Command::new("sh").args(["-c", "echo out; echo err >&2; exit 2"]),
            Duration::from_secs(10),
        )
        .unwrap();
        assert!(
            matches!(captured.finished, Finished::Exited(status) if status.code() == Some(2)),
            "{captured:?}"
        );
        assert_eq!(captured.stdout, b"out\n");
        assert_eq!(captured.stderr, b"err\n");
    }

    #[test]
    fn output_reads_more_than_a_pipe_holds() {
        let captured = output(
            Command::new("sh").args(["-c", "head -c 1000000 /dev/zero"]),
            Duration::from_secs(10),
        )
        .unwrap();
        assert!(matches!(captured.finished, Finished::Exited(_)));
        assert_eq!(captured.stdout.len(), 1_000_000);
    }

    #[test]
    fn output_of_a_hanging_process_times_out_with_what_it_printed() {
        let start = Instant::now();
        let captured = output(
            Command::new("sh").args(["-c", "echo started; sleep 30"]),
            Duration::from_millis(500),
        )
        .unwrap();
        assert!(
            matches!(captured.finished, Finished::TimedOut),
            "{captured:?}"
        );
        assert_eq!(captured.stdout, b"started\n");
        assert!(start.elapsed() < Duration::from_secs(10));
    }
}
