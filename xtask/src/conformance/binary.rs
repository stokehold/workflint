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
/// Returns a message if the program cannot start, exits with a status other
/// than 0 (no errors) or 1 (errors found), or prints output that is not UTF-8.
pub fn run(program: &Path, invocation: &Invocation) -> Result<String, String> {
    let output = Command::new(program)
        .args(&invocation.args)
        .current_dir(&invocation.cwd)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("running {}: {error}", program.display()))?;
    match output.status.code() {
        Some(0 | 1) => String::from_utf8(output.stdout).map_err(|error| {
            format!(
                "{} printed output that is not UTF-8: {error}",
                program.display()
            )
        }),
        _ => Err(format!(
            "{} exited with {}\nstderr:\n{}",
            program.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )),
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
    fn messages_without_the_artifact_give_no_executable() {
        assert_eq!(
            executable(r#"{"reason":"build-finished","success":false}"#),
            None
        );
    }
}
