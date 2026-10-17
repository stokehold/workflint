//! Builds and runs `xtask/expr-check`, the Go program around the reference's
//! expression checker.
//!
//! The program imports the `actionlint` package, so it builds inside the
//! reference checkout, with the module and the dependencies that `cargo xtask
//! reference` uses. `go build -overlay` adds the program to the checkout as
//! [`PACKAGE`] for that one build, so the checkout keeps no new file and no
//! other `go.mod` or `go.sum` has to agree with the reference's.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Serialize;
use xtask::process::{self, Finished};

use super::compare::Case;
use super::outcome::Outcome;
use super::setups::Setup;
use crate::reference::go;

/// How long the program may take for every case together. It needs about
/// ten seconds for the committed inputs.
const TIME_LIMIT: Duration = Duration::from_mins(5);

/// The package that the overlay adds to the checkout.
const PACKAGE: &str = "cmd/workflint-expr-check";

/// Builds the program in `source` against the reference `checkout` into
/// `binary`, after checking that `go` is the version `go_version`. `work` is
/// a directory for the overlay file.
///
/// Go's build cache makes a build with no changes fast, so this always runs
/// `go build`.
pub(crate) fn build(
    go: &OsString,
    checkout: &Path,
    source: &Path,
    work: &Path,
    binary: &Path,
    go_version: &str,
) -> Result<()> {
    // Go reads the overlay with the paths it computes from its working
    // directory, which has no symbolic links once canonical.
    let checkout = fs::canonicalize(checkout)
        .with_context(|| format!("cannot resolve {}", checkout.display()))?;
    let main = fs::canonicalize(source.join("main.go"))
        .with_context(|| format!("cannot resolve {}", source.join("main.go").display()))?;
    let package = checkout.join(PACKAGE);
    if package.exists() {
        bail!(
            "{} exists, so the overlay cannot add the program there",
            package.display()
        );
    }
    go::require_version(
        go,
        &checkout,
        go_version,
        "building the reference's expression checker",
    )?;

    fs::create_dir_all(work).with_context(|| format!("cannot create {}", work.display()))?;
    let work =
        fs::canonicalize(work).with_context(|| format!("cannot resolve {}", work.display()))?;
    let overlay = work.join("overlay.json");
    let mut replace = serde_json::Map::new();
    replace.insert(
        path_text(&package.join("main.go"))?,
        path_text(&main)?.into(),
    );
    let json = serde_json::json!({ "Replace": replace });
    fs::write(&overlay, json.to_string())
        .with_context(|| format!("cannot write {}", overlay.display()))?;

    let output = std::path::absolute(binary)
        .with_context(|| format!("cannot resolve {}", binary.display()))?;
    let status = Command::new(go)
        .args(["build", "-trimpath", "-overlay"])
        .arg(&overlay)
        .arg("-o")
        .arg(&output)
        .arg(format!("./{PACKAGE}"))
        .current_dir(&checkout)
        .env("PWD", &checkout)
        .env("CGO_ENABLED", "0")
        .status()
        .with_context(|| format!("cannot run `{} build`", go.display()))?;
    if !status.success() {
        bail!("`go build` of the reference's expression checker failed with {status}");
    }
    Ok(())
}

fn path_text(path: &Path) -> Result<String> {
    match path.to_str() {
        Some(text) => Ok(text.to_owned()),
        None => bail!("{} is not valid UTF-8", path.display()),
    }
}

#[derive(Serialize)]
struct Input<'a> {
    setups: &'a [Setup],
    cases: &'a [Case],
}

/// Runs `binary` on `cases` and returns the outcome of each, in order.
/// `work` is a directory for the input file.
pub(crate) fn run(
    binary: &Path,
    work: &Path,
    setups: &[Setup],
    cases: &[Case],
) -> Result<Vec<Outcome>> {
    let input = work.join("input.json");
    let json = serde_json::to_vec(&Input { setups, cases })?;
    fs::write(&input, json).with_context(|| format!("cannot write {}", input.display()))?;
    let output = process::output(Command::new(binary).arg(&input), TIME_LIMIT)?;
    match output.finished {
        Finished::Exited(status) if status.success() => {}
        Finished::Exited(status) => bail!(
            "{} failed with {status}: {}",
            binary.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ),
        Finished::TimedOut => bail!(
            "{} did not finish within {} seconds",
            binary.display(),
            TIME_LIMIT.as_secs()
        ),
    }
    let stdout = String::from_utf8(output.stdout)
        .context("the output of the reference's expression checker is not UTF-8")?;
    parse_output(&stdout, cases.len())
}

/// Parses one outcome per line, and checks that there are `expected` of them.
fn parse_output(stdout: &str, expected: usize) -> Result<Vec<Outcome>> {
    let outcomes = stdout
        .lines()
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str(line).with_context(|| {
                format!(
                    "line {} of the reference's expression checker: {line}",
                    index + 1
                )
            })
        })
        .collect::<Result<Vec<Outcome>>>()?;
    if outcomes.len() != expected {
        bail!(
            "the reference's expression checker printed {} outcomes for {expected} expressions",
            outcomes.len()
        );
    }
    Ok(outcomes)
}

/// The path of the program's binary in `work`.
pub(crate) fn binary_path(work: &Path) -> PathBuf {
    work.join(format!("expr-check{}", std::env::consts::EXE_SUFFIX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_has_one_outcome_per_case() {
        let stdout = "{\"type\":\"number\",\"constant\":true}\n\
                      {\"parse_error\":{\"line\":1,\"column\":2,\"message\":\"m\"}}\n";
        assert_eq!(parse_output(stdout, 2).unwrap().len(), 2);
        let error = parse_output(stdout, 3).unwrap_err();
        assert!(
            format!("{error:#}").contains("2 outcomes for 3"),
            "{error:#}"
        );
        let error = parse_output("{\"type\":1}\n", 1).unwrap_err();
        assert!(format!("{error:#}").contains("line 1"), "{error:#}");
    }

    #[test]
    fn missing_go_names_the_task_and_the_version() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("main.go"), "package main\n").unwrap();
        let go = OsString::from("workflint-test-no-such-go");
        let error = build(
            &go,
            dir.path(),
            dir.path(),
            &dir.path().join("work"),
            &dir.path().join("bin"),
            "go1.26.4",
        )
        .unwrap_err();
        let message = format!("{error:#}");
        assert!(
            message.contains("building the reference's expression checker needs go1.26.4"),
            "{message}"
        );
    }

    #[test]
    fn the_program_imports_only_the_reference_and_the_standard_library() {
        let main = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("expr-check")
                .join("main.go"),
        )
        .unwrap();
        let start = main.find("import (").unwrap();
        let end = start + main[start..].find(')').unwrap();
        let imports: Vec<&str> = main[start + "import (".len()..end]
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        assert_eq!(
            imports,
            [
                "\"bufio\"",
                "\"encoding/json\"",
                "\"fmt\"",
                "\"os\"",
                "\"github.com/rhysd/actionlint\"",
            ]
        );
    }
}
