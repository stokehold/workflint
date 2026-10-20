//! External tool processes, run through `workflint-test-tool`, a program built from this
//! repository, so the tests behave the same on every OS.
//!
//! The expected behaviour is the reference's `cmdExecution.run`, `concurrentProcess`, and
//! `resolveExternalCommand` (`process.go`), with the differences in `docs/differences.md`
//! (`#external-process-hangs`, `#tool-failures`).

#![allow(clippy::unwrap_used, clippy::panic)]

use std::ffi::OsString;
use std::num::NonZeroUsize;
use std::path::Path;
use std::time::{Duration, Instant};
use std::{env, fs, thread};

use tempfile::TempDir;
use workflint_linter::tool::{ResolveError, RunOptions, ToolCommand, ToolOutcome, ToolPool};

const TEST_TOOL: &str = env!("CARGO_BIN_EXE_workflint-test-tool");

fn test_tool() -> ToolCommand {
    ToolCommand::resolve(TEST_TOOL).unwrap()
}

fn pool(limit: usize) -> ToolPool {
    ToolPool::new(NonZeroUsize::new(limit).unwrap()).unwrap()
}

fn run(args: &[&str], stdin: &[u8], options: &RunOptions) -> ToolOutcome {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    pool(1).run(&test_tool(), &args, stdin, options)
}

fn exited(outcome: ToolOutcome) -> (i32, Vec<u8>, Vec<u8>) {
    match outcome {
        ToolOutcome::Exited {
            code,
            stdout,
            stderr,
        } => (code, stdout, stderr),
        other => panic!("expected the tool to exit, got {other:?}"),
    }
}

/// 200 KiB of shell script, all different lines, so a lost or reordered chunk shows.
fn large_script() -> Vec<u8> {
    let mut script = Vec::new();
    let mut i = 0;
    while script.len() < 200 * 1024 {
        script.extend_from_slice(format!("echo \"line {i}\" | tr a-z A-Z\n").as_bytes());
        i += 1;
    }
    script.truncate(200 * 1024);
    script
}

#[test]
fn a_200_kib_script_goes_through_stdin_and_comes_back_on_stdout_and_stderr() {
    let script = large_script();
    let (code, stdout, stderr) = exited(run(&["echo"], &script, &RunOptions::default()));
    assert_eq!(code, 0);
    assert!(stdout == script, "stdout differs from stdin");
    assert!(stderr == script, "stderr differs from stdin");
}

#[test]
fn combined_output_keeps_the_order_the_tool_wrote_in() {
    let options = RunOptions {
        combine_output: true,
        ..RunOptions::default()
    };
    let (code, stdout, stderr) = exited(run(&["interleave"], b"", &options));
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8(stdout).unwrap(), "out 1\nerr\nout 2\n");
    assert_eq!(stderr, b"");
}

#[test]
fn a_tool_that_does_not_read_stdin_still_finishes() {
    let script = large_script();
    let (code, stdout, _) = exited(run(&["args", "x"], &script, &RunOptions::default()));
    assert_eq!(code, 0);
    assert_eq!(stdout, b"x\n");
}

#[test]
fn a_process_that_never_exits_is_killed_after_the_timeout() {
    let timeout = Duration::from_millis(300);
    let start = Instant::now();
    let outcome = run(
        &["sleep"],
        b"",
        &RunOptions {
            timeout,
            ..RunOptions::default()
        },
    );
    let elapsed = start.elapsed();
    match outcome {
        ToolOutcome::TimedOut { timeout: reported } => assert_eq!(reported, timeout),
        other => panic!("expected a timeout, got {other:?}"),
    }
    assert!(elapsed >= timeout, "returned after {elapsed:?}");
    assert!(
        elapsed < Duration::from_secs(30),
        "returned after {elapsed:?}"
    );
}

#[test]
fn no_more_than_the_limit_of_processes_run_at_once() {
    let dir = TempDir::new().unwrap();
    let pool = pool(2);
    let command = test_tool();
    let args = [OsString::from("overlap"), dir.path().as_os_str().to_owned()];
    let counts: Vec<usize> = thread::scope(|scope| {
        let handles: Vec<_> = (0..6)
            .map(|_| scope.spawn(|| pool.run(&command, &args, b"", &RunOptions::default())))
            .collect();
        handles
            .into_iter()
            .map(|handle| {
                let (code, stdout, _) = exited(handle.join().unwrap());
                assert_eq!(code, 0);
                String::from_utf8(stdout).unwrap().parse().unwrap()
            })
            .collect()
    });
    assert_eq!(
        counts.iter().max(),
        Some(&2),
        "processes at once: {counts:?}"
    );
}

#[test]
fn the_default_limit_is_the_available_parallelism() {
    let expected = thread::available_parallelism().map_or(1, NonZeroUsize::get);
    assert_eq!(
        ToolPool::with_available_parallelism()
            .unwrap()
            .limit()
            .get(),
        expected
    );
}

#[test]
fn a_non_zero_exit_with_output_is_an_exit() {
    let (code, stdout, stderr) = exited(run(&["exit", "1", "[]"], b"", &RunOptions::default()));
    assert_eq!(code, 1);
    assert_eq!(stdout, b"[]");
    assert_eq!(stderr, b"failed");
}

#[test]
fn a_non_zero_exit_with_empty_stdout_is_a_failure() {
    match run(&["exit", "2"], b"", &RunOptions::default()) {
        ToolOutcome::FailedWithoutOutput { code, stderr } => {
            assert_eq!(code, 2);
            assert_eq!(stderr, b"failed");
        }
        other => panic!("expected a failure without output, got {other:?}"),
    }
}

#[test]
fn a_zero_exit_with_empty_stdout_is_an_exit() {
    let (code, stdout, _) = exited(run(&["exit", "0"], b"", &RunOptions::default()));
    assert_eq!(code, 0);
    assert_eq!(stdout, b"");
}

#[cfg(unix)]
#[test]
fn a_process_killed_by_a_signal_reports_the_signal() {
    match run(&["kill-self"], b"", &RunOptions::default()) {
        ToolOutcome::Signaled { signal, .. } => assert_eq!(signal, Some(9)),
        other => panic!("expected a signal, got {other:?}"),
    }
}

#[test]
fn a_program_that_cannot_start_is_a_spawn_failure() {
    let dir = TempDir::new().unwrap();
    let program = dir.path().join(exe_name("workflint-test-tool"));
    fs::copy(TEST_TOOL, &program).unwrap();
    let command = ToolCommand::resolve(program.to_str().unwrap()).unwrap();
    fs::remove_file(&program).unwrap();
    let outcome = pool(1).run(&command, &[] as &[OsString], b"", &RunOptions::default());
    assert!(
        matches!(outcome, ToolOutcome::SpawnFailed(_)),
        "expected a spawn failure, got {outcome:?}"
    );
}

#[test]
fn the_environment_is_inherited() {
    // Cargo and nextest both set CARGO_MANIFEST_DIR in the environment of the test.
    let expected = env::var("CARGO_MANIFEST_DIR").unwrap();
    let (_, stdout, _) = exited(run(
        &["env", "CARGO_MANIFEST_DIR"],
        b"",
        &RunOptions::default(),
    ));
    assert_eq!(String::from_utf8(stdout).unwrap(), format!("{expected}\n"));
}

fn exe_name(name: &str) -> String {
    format!("{name}{}", env::consts::EXE_SUFFIX)
}

fn test_tool_dir() -> &'static Path {
    Path::new(TEST_TOOL).parent().unwrap()
}

#[test]
fn a_bare_name_is_looked_up_on_path() {
    let path = env::join_paths([test_tool_dir()]).unwrap();
    let command = ToolCommand::resolve_with_path("workflint-test-tool", Some(&path)).unwrap();
    assert_eq!(command.program(), Path::new(TEST_TOOL));
    assert_eq!(command.args(), &[] as &[OsString]);
}

#[test]
fn a_bare_name_is_not_looked_up_in_a_relative_directory_on_path() {
    // The test runs in the crate directory, so `tests/support` holds the source of the tool, not
    // the tool. A copy of the tool in a relative `PATH` entry must not be found.
    let dir = TempDir::new_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    fs::copy(TEST_TOOL, dir.path().join(exe_name("workflint-test-tool"))).unwrap();
    let relative = dir.path().strip_prefix(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = env::join_paths([relative, Path::new(".")]).unwrap();
    assert!(matches!(
        ToolCommand::resolve_with_path("workflint-test-tool", Some(&path)),
        Err(ResolveError::NotFound { .. })
    ));
}

#[test]
fn a_command_line_is_split_and_its_words_prepended_to_the_arguments() {
    let path = env::join_paths([test_tool_dir()]).unwrap();
    let command =
        ToolCommand::resolve_with_path("workflint-test-tool args 'a b' \"c\\\"d\"", Some(&path))
            .unwrap();
    assert_eq!(command.program(), Path::new(TEST_TOOL));
    let extra = [OsString::from("e")];
    let (_, stdout, _) = exited(pool(1).run(&command, &extra, b"", &RunOptions::default()));
    assert_eq!(String::from_utf8(stdout).unwrap(), "a b\nc\"d\ne\n");
}

#[test]
fn an_unknown_program_is_not_found() {
    let dir = TempDir::new().unwrap();
    let path = env::join_paths([dir.path()]).unwrap();
    let error = ToolCommand::resolve_with_path("no-such-tool --flag", Some(&path)).unwrap_err();
    assert!(
        matches!(&error, ResolveError::NotFound { name } if name == "no-such-tool --flag"),
        "{error:?}"
    );
}

#[test]
fn a_path_with_a_separator_is_used_as_given() {
    let resolved = ToolCommand::resolve_with_path(TEST_TOOL, None).unwrap();
    assert_eq!(resolved.program(), Path::new(TEST_TOOL));
}

#[cfg(unix)]
#[test]
fn sigterm_kills_the_running_children() {
    use std::path::PathBuf;
    use std::process::{Command, Stdio};

    use workflint_linter::tool::INTERRUPTED_EXIT_CODE;

    const CHILDREN: usize = 3;
    let dir = TempDir::new().unwrap();
    let mut parent = Command::new(TEST_TOOL)
        .args(["pool".as_ref(), dir.path().as_os_str(), "3".as_ref()])
        .stdin(Stdio::null())
        .spawn()
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(30);
    let pids: Vec<PathBuf> = loop {
        let pids: Vec<PathBuf> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        if pids.len() == CHILDREN {
            break pids;
        }
        assert!(Instant::now() < deadline, "the children did not start");
        thread::sleep(Duration::from_millis(20));
    };
    let pids: Vec<String> = pids
        .iter()
        .map(|pid| pid.file_name().unwrap().to_str().unwrap().to_owned())
        .collect();

    let kill = |args: &[&str]| {
        Command::new("kill")
            .args(args)
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    };
    assert!(kill(&["-TERM", &parent.id().to_string()]));
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = parent.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            parent.kill().unwrap();
            for pid in &pids {
                kill(&["-KILL", pid]);
            }
            panic!("the parent did not exit after SIGTERM");
        }
        thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(status.code(), Some(INTERRUPTED_EXIT_CODE), "{status:?}");

    // The parent waits for the children it kills, so none is left as a zombie.
    for pid in &pids {
        let alive = kill(&["-0", pid]);
        if alive {
            kill(&["-KILL", pid]);
        }
        assert!(!alive, "child {pid} survived SIGTERM of its parent");
    }
}

#[test]
fn a_timeout_too_long_for_the_clock_never_passes() {
    let options = RunOptions {
        timeout: Duration::MAX,
        ..RunOptions::default()
    };
    let (code, _, _) = exited(run(&["exit", "0"], b"", &options));
    assert_eq!(code, 0);
}
