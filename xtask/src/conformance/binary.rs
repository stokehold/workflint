//! Builds the `workflint` binary from the workspace and runs it on a fixture.

use std::fs::File;
use std::hash::{DefaultHasher, Hasher};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

use serde::Deserialize;

use super::fixture::Invocation;
use crate::process::{self, Finished};

/// The path of a copy of the `workflint` binary, built from the workspace.
///
/// `cargo test` does not build the binaries of other packages, so the runner
/// runs `cargo build -p workflint` itself, once per process. Under nextest,
/// which runs each test in its own process, every test runs it. When the
/// binary is up to date, that takes about 50 milliseconds.
///
/// Every `cargo build` deletes `target/debug/workflint` and hard-links it
/// again, even when it rebuilds nothing. A test that starts that path while
/// another process builds can fail with "No such file or directory". So the
/// runner copies the binary to `target/conformance/workflint-<hash>`, where
/// `<hash>` is a hash of its content, and returns the path of the copy. No
/// build deletes the copy. A change to `crates/workflint` gives a new hash
/// and a new copy, and the runner then deletes the older copies.
///
/// # Panics
///
/// Panics if the build or the copy fails.
pub fn workflint() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| build().unwrap_or_else(|error| panic!("{error}")))
}

#[derive(Deserialize)]
struct Message {
    reason: String,
    target: Option<Target>,
    executable: Option<PathBuf>,
}

#[derive(Deserialize)]
struct Target {
    name: String,
}

fn build() -> Result<PathBuf, String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(&cargo)
        .args([
            "build",
            "--package",
            "workflint",
            "--bin",
            "workflint",
            "--message-format=json-render-diagnostics",
        ])
        .current_dir(super::repository_root())
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("running {}: {error}", cargo.to_string_lossy()))?;
    if !output.status.success() {
        return Err(format!(
            "cargo build --package workflint failed: {}",
            output.status
        ));
    }
    let built = executable(&String::from_utf8_lossy(&output.stdout)).ok_or_else(|| {
        "cargo build --package workflint reported no workflint executable".to_owned()
    })?;
    // `built` is `<target>/<profile>/workflint`.
    let target = built
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| format!("{} has no target directory", built.display()))?;
    stable_copy(&built, &target.join("conformance"))
}

/// Copies `source` into `directory` under a name that depends on its content
/// and returns the path of the copy.
///
/// Another process's `cargo build` can delete `source` and hard-link it again
/// at any time, so this function retries opening it while it is missing. It
/// writes the copy to a temporary file and then renames that file, so no
/// process sees a partial copy. If a copy with the same content exists, it
/// returns that copy.
fn stable_copy(source: &Path, directory: &Path) -> Result<PathBuf, String> {
    let mut file = open_relinked(source)?;
    let mut content = Vec::new();
    file.read_to_end(&mut content)
        .map_err(|error| format!("reading {}: {error}", source.display()))?;
    let permissions = file
        .metadata()
        .map_err(|error| format!("reading the metadata of {}: {error}", source.display()))?
        .permissions();

    let mut hasher = DefaultHasher::new();
    hasher.write(&content);
    let mut name = format!("workflint-{:016x}", hasher.finish());
    if let Some(extension) = source.extension() {
        name.push('.');
        name.push_str(&extension.to_string_lossy());
    }
    let copy = directory.join(name);
    if copy.is_file() {
        return Ok(copy);
    }

    std::fs::create_dir_all(directory)
        .map_err(|error| format!("creating {}: {error}", directory.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(directory)
        .map_err(|error| format!("creating a file in {}: {error}", directory.display()))?;
    temporary
        .write_all(&content)
        .and_then(|()| temporary.as_file().set_permissions(permissions))
        .map_err(|error| format!("writing {}: {error}", temporary.path().display()))?;
    match temporary.persist_noclobber(&copy) {
        Ok(_) => {
            prune(directory, &copy);
            Ok(copy)
        }
        // Another test process made the same copy first.
        Err(error) if error.error.kind() == ErrorKind::AlreadyExists => Ok(copy),
        Err(error) => Err(format!(
            "moving a copy to {}: {}",
            copy.display(),
            error.error
        )),
    }
}

/// Deletes the copies in `directory` other than `keep`, so that each build
/// of a changed binary does not leave another copy behind. A copy that
/// cannot be deleted, such as one that runs on Windows, stays.
fn prune(directory: &Path, keep: &Path) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_copy = entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with("workflint-"));
        if is_copy && path != keep {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Opens `path`, which `cargo build` may be deleting and linking again.
///
/// Cargo links the file again right after it deletes it, so this function
/// retries every millisecond, for at most 100 milliseconds, while the file is
/// missing.
fn open_relinked(path: &Path) -> Result<File, String> {
    let mut attempts = 0;
    loop {
        match File::open(path) {
            Err(error) if error.kind() == ErrorKind::NotFound && attempts < 100 => {
                attempts += 1;
                std::thread::sleep(Duration::from_millis(1));
            }
            result => {
                return result.map_err(|error| format!("opening {}: {error}", path.display()));
            }
        }
    }
}

/// Finds the `workflint` executable in the JSON messages of `cargo build
/// --message-format=json`.
#[must_use]
pub fn executable(messages: &str) -> Option<PathBuf> {
    messages
        .lines()
        .filter_map(|line| serde_json::from_str::<Message>(line).ok())
        .filter(|message| message.reason == "compiler-artifact")
        .filter(|message| {
            message
                .target
                .as_ref()
                .is_some_and(|t| t.name == "workflint")
        })
        .find_map(|message| message.executable)
}

/// How long one run of a binary on one fixture may take. A debug build of
/// workflint lints a fixture in milliseconds, and the reference in less.
pub const TIME_LIMIT: Duration = Duration::from_secs(60);

/// Runs `program` as `invocation` describes and returns its standard output.
///
/// # Errors
///
/// Returns a message if the program cannot start, if it runs longer than
/// [`TIME_LIMIT`], or if [`check`] rejects what it did.
pub fn run(program: &Path, invocation: &Invocation) -> Result<String, String> {
    run_with_limit(program, invocation, TIME_LIMIT)
}

fn run_with_limit(
    program: &Path,
    invocation: &Invocation,
    limit: Duration,
) -> Result<String, String> {
    let captured = process::output(
        Command::new(program)
            .args(&invocation.args)
            .current_dir(&invocation.cwd),
        limit,
    )
    .map_err(|error| format!("{error:#}"))?;
    let code = match captured.finished {
        Finished::Exited(status) => status.code(),
        Finished::TimedOut => {
            return Err(format!(
                "{} did not finish within {} s and was killed",
                program.display(),
                limit.as_secs()
            ));
        }
    };
    check(
        &program.display().to_string(),
        code,
        captured.stdout,
        &captured.stderr,
    )
}

/// Checks the exit code and the output of a run of `program` and returns its
/// standard output.
///
/// `code` is `None` when a signal ended the process. Exit code 0 means no
/// errors and needs empty standard output. Exit code 1 means errors were
/// found and needs some. Standard error must be empty.
///
/// # Errors
///
/// Returns a message that names each cause: another exit code, exit code 0
/// with output, exit code 1 without output, output on standard error, or
/// output that is not UTF-8.
pub fn check(
    program: &str,
    code: Option<i32>,
    stdout: Vec<u8>,
    stderr: &[u8],
) -> Result<String, String> {
    let mut causes = Vec::new();
    match code {
        Some(0) if !stdout.is_empty() => causes.push(format!(
            "{program} exited with code 0, which means no errors, but printed output"
        )),
        Some(1) if stdout.is_empty() => causes.push(format!(
            "{program} exited with code 1, which means errors were found, but printed none"
        )),
        Some(0 | 1) => {}
        Some(code) => causes.push(format!(
            "{program} exited with code {code}; only 0 and 1 are normal"
        )),
        None => causes.push(format!("{program} was stopped by a signal")),
    }
    if !stderr.is_empty() {
        causes.push(format!(
            "{program} printed to standard error:\n{}",
            String::from_utf8_lossy(stderr).trim_end()
        ));
    }
    let stdout = String::from_utf8(stdout)
        .map_err(|error| format!("{program} printed output that is not UTF-8: {error}"));
    match (causes.is_empty(), stdout) {
        (true, Ok(stdout)) => Ok(stdout),
        (true, Err(error)) => Err(error),
        (false, stdout) => {
            if let Ok(stdout) = stdout.as_deref()
                && !stdout.is_empty()
            {
                causes.push(format!("standard output:\n{}", stdout.trim_end()));
            }
            Err(causes.join("\n"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_executable_comes_from_the_workflint_artifact_message() {
        let messages = r#"{"reason":"compiler-artifact","target":{"name":"workflint-diagnostics"},"executable":null}
{"reason":"compiler-artifact","target":{"name":"workflint"},"executable":"C:\\target\\debug\\workflint.exe"}
{"reason":"build-finished","success":true}"#;
        assert_eq!(
            executable(messages),
            Some(PathBuf::from(r"C:\target\debug\workflint.exe"))
        );
    }

    /// A fresh `cargo build` deletes the binary it reports and hard-links it
    /// again from `deps/`. The copy that the runner starts must survive that.
    #[cfg(unix)]
    #[test]
    fn the_copy_starts_while_cargo_relinks_the_binary() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let temp = tempfile::tempdir().unwrap();
        let deps = temp.path().join("deps");
        std::fs::create_dir(&deps).unwrap();
        let original = deps.join("workflint-0123456789abcdef");
        std::fs::copy("/usr/bin/true", &original).unwrap();
        let uplifted = temp.path().join("workflint");
        std::fs::hard_link(&original, &uplifted).unwrap();
        let copies = temp.path().join("conformance");

        let done = AtomicBool::new(false);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                while !done.load(Ordering::Relaxed) {
                    std::fs::remove_file(&uplifted).unwrap();
                    std::fs::hard_link(&original, &uplifted).unwrap();
                }
            });
            let result = (0..50).try_for_each(|_| {
                let program = stable_copy(&uplifted, &copies)?;
                let status = Command::new(&program)
                    .status()
                    .map_err(|error| format!("running {}: {error}", program.display()))?;
                status
                    .success()
                    .then_some(())
                    .ok_or_else(|| format!("{} exited with {status}", program.display()))
            });
            done.store(true, Ordering::Relaxed);
            result.unwrap();
        });
    }

    #[cfg(unix)]
    #[test]
    fn a_new_copy_deletes_the_older_copies() {
        let temp = tempfile::tempdir().unwrap();
        let copies = temp.path().join("conformance");
        let first = temp.path().join("first");
        std::fs::write(&first, "#!/bin/sh\nexit 0\n").unwrap();
        let old = stable_copy(&first, &copies).unwrap();
        let second = temp.path().join("second");
        std::fs::write(&second, "#!/bin/sh\nexit 1\n").unwrap();
        let new = stable_copy(&second, &copies).unwrap();
        assert_ne!(old, new);
        assert!(!old.exists(), "{} is still there", old.display());
        assert!(new.is_file());
        assert_eq!(stable_copy(&second, &copies).unwrap(), new);
    }

    #[cfg(unix)]
    #[test]
    fn a_binary_that_hangs_is_killed_at_the_time_limit() {
        use std::os::unix::fs::PermissionsExt as _;

        use crate::conformance::fixture::{Fixture, Origin, Tools};

        let temp = tempfile::tempdir().unwrap();
        let script = temp.path().join("hang");
        std::fs::write(&script, "#!/bin/sh\nsleep 30\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let ok = Origin::Actionlint.root().join("ok/minimal.yaml");
        let invocation = Fixture::new(Origin::Actionlint, &ok)
            .prepare(&Tools::default())
            .unwrap();
        let start = std::time::Instant::now();
        let error = run_with_limit(&script, &invocation, Duration::from_millis(500)).unwrap_err();
        assert!(error.contains("did not finish within"), "{error}");
        assert!(start.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn exit_code_0_without_output_passes() {
        assert_eq!(check("wl", Some(0), Vec::new(), b""), Ok(String::new()));
    }

    #[test]
    fn exit_code_1_with_output_passes() {
        let stdout = b"test.yaml:1:1: a [x]\n".to_vec();
        assert_eq!(
            check("wl", Some(1), stdout, b""),
            Ok("test.yaml:1:1: a [x]\n".to_owned())
        );
    }

    #[test]
    fn exit_code_0_with_output_fails() {
        let error = check("wl", Some(0), b"Hello, world!\n".to_vec(), b"").unwrap_err();
        assert!(
            error.contains("exited with code 0, which means no errors, but printed output"),
            "{error}"
        );
        assert!(error.contains("Hello, world!"), "{error}");
    }

    #[test]
    fn exit_code_1_without_output_fails() {
        let error = check("wl", Some(1), Vec::new(), b"").unwrap_err();
        assert!(
            error.contains("exited with code 1, which means errors were found, but printed none"),
            "{error}"
        );
    }

    #[test]
    fn another_exit_code_fails() {
        let error = check("wl", Some(2), b"a\n".to_vec(), b"").unwrap_err();
        assert!(error.contains("exited with code 2"), "{error}");
        assert!(
            check("wl", None, Vec::new(), b"")
                .unwrap_err()
                .contains("signal")
        );
    }

    #[test]
    fn output_on_standard_error_fails() {
        let error = check("wl", Some(0), Vec::new(), b"warning: x\n").unwrap_err();
        assert!(
            error.contains("wl printed to standard error:\nwarning: x"),
            "{error}"
        );
    }

    #[test]
    fn every_cause_is_named() {
        let error = check("wl", Some(1), Vec::new(), b"oops").unwrap_err();
        assert!(error.contains("printed none"), "{error}");
        assert!(error.contains("printed to standard error"), "{error}");
    }

    #[test]
    fn output_that_is_not_utf8_fails() {
        let error = check("wl", Some(1), vec![0xff], b"").unwrap_err();
        assert!(error.contains("not UTF-8"), "{error}");
    }

    #[test]
    fn messages_without_the_artifact_give_no_executable() {
        assert_eq!(
            executable(r#"{"reason":"build-finished","success":false}"#),
            None
        );
    }
}
