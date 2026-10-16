//! Builds the `workflint` binary from the workspace and runs it on a fixture.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use serde::Deserialize;

use super::fixture::Invocation;

/// The path of the `workflint` binary, built from the workspace.
///
/// `cargo test` does not build the binaries of other packages, so the runner
/// runs `cargo build -p workflint` itself, once per process. Under nextest,
/// which runs each test in its own process, every test runs it. When the
/// binary is up to date, that takes about 50 milliseconds.
///
/// # Panics
///
/// Panics if the build fails.
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
    executable(&String::from_utf8_lossy(&output.stdout)).ok_or_else(|| {
        "cargo build --package workflint reported no workflint executable".to_owned()
    })
}

/// Finds the `workflint` executable in the JSON messages of `cargo build`.
fn executable(messages: &str) -> Option<PathBuf> {
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

/// Runs `program` as `invocation` describes and returns its standard output.
///
/// # Errors
///
/// Returns a message if the program cannot start, or if [`check`] rejects
/// what it did.
pub fn run(program: &Path, invocation: &Invocation) -> Result<String, String> {
    let output = Command::new(program)
        .args(&invocation.args)
        .current_dir(&invocation.cwd)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("running {}: {error}", program.display()))?;
    check(
        &program.display().to_string(),
        output.status.code(),
        output.stdout,
        &output.stderr,
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
