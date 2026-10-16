//! Builds and runs the go-yaml dump program in `xtask/goyaml-dump`.

use std::ffi::OsString;
use std::fs;
use std::io::Write as _;
use std::path::{self, Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

use anyhow::{Context, Result, bail};

use crate::reference::go;

/// Builds the program in `source` into `binary` with `go`, after checking
/// that `go` is the version `go_version`.
///
/// Go's build cache makes a build with no changes fast, so this always runs
/// `go build`.
pub(crate) fn build(go: &OsString, source: &Path, binary: &Path, go_version: &str) -> Result<()> {
    go::require_version(go, source, go_version, "building the go-yaml dump")?;
    let output =
        path::absolute(binary).with_context(|| format!("cannot resolve {}", binary.display()))?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    let status = Command::new(go)
        .args(["build", "-trimpath", "-o"])
        .arg(&output)
        .arg(".")
        .current_dir(source)
        .env("CGO_ENABLED", "0")
        .status()
        .with_context(|| format!("cannot run `{} build`", go.display()))?;
    if !status.success() {
        bail!("`go build` of the go-yaml dump failed with {status}");
    }
    Ok(())
}

/// Runs `binary` on `files` and returns the dump of each file, in order.
pub(crate) fn run(binary: &Path, files: &[PathBuf]) -> Result<Vec<String>> {
    let paths = files
        .iter()
        .map(|file| {
            let absolute = path::absolute(file)
                .with_context(|| format!("cannot resolve {}", file.display()))?;
            match absolute.into_os_string().into_string() {
                Ok(path) if !path.contains(['\n', '\r']) => Ok(path),
                _ => bail!("{} is not a path the go-yaml dump can read", file.display()),
            }
        })
        .collect::<Result<Vec<_>>>()?;

    let mut child = Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("cannot run {}", binary.display()))?;
    let mut stdin = child.stdin.take().context("no standard input")?;
    let mut input = paths.join("\n");
    input.push('\n');
    // Write from another thread, so that a full output pipe cannot block the
    // program while this thread still writes its input.
    let writer = thread::spawn(move || stdin.write_all(input.as_bytes()));
    let output = child
        .wait_with_output()
        .with_context(|| format!("cannot run {}", binary.display()))?;
    match writer.join() {
        Ok(result) => result.context("cannot write the paths to the go-yaml dump")?,
        Err(_) => bail!("the thread that writes the paths panicked"),
    }
    if !output.status.success() {
        bail!("{} failed with {}", binary.display(), output.status);
    }
    let stdout = String::from_utf8(output.stdout).context("the go-yaml dump is not UTF-8")?;
    split_output(&stdout, &paths)
}

/// Splits the output of the program into the dump of each file, and checks
/// that it has one dump for each of `paths`, in order.
fn split_output(output: &str, paths: &[String]) -> Result<Vec<String>> {
    let mut dumps: Vec<(&str, String)> = Vec::new();
    for line in output.split_inclusive('\n') {
        if let Some(path) = line.strip_prefix("=== ") {
            dumps.push((path.trim_end_matches(['\n', '\r']), String::new()));
        } else if let Some((_, dump)) = dumps.last_mut() {
            dump.push_str(line);
        } else {
            bail!("the go-yaml dump starts without a file header: {line:?}");
        }
    }
    for (index, expected) in paths.iter().enumerate() {
        match dumps.get(index) {
            Some((path, _)) if path == expected => {}
            Some((path, _)) => {
                bail!("the go-yaml dump has {path} where {expected} was expected");
            }
            None => bail!("the go-yaml dump has no output for {expected}"),
        }
    }
    if let Some((path, _)) = dumps.get(paths.len()) {
        bail!("the go-yaml dump has output for {path}, which was not requested");
    }
    Ok(dumps.into_iter().map(|(_, dump)| dump).collect())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn paths(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|path| (*path).to_owned()).collect()
    }

    #[test]
    fn output_splits_into_the_dump_of_each_file() {
        let output =
            "=== a.yaml\nmapping 1:1\n  scalar 1:1 \"a\"\n  scalar 1:4 \"1\"\n=== b.yaml\nempty\n";
        assert_eq!(
            split_output(output, &paths(&["a.yaml", "b.yaml"])).unwrap(),
            [
                "mapping 1:1\n  scalar 1:1 \"a\"\n  scalar 1:4 \"1\"\n",
                "empty\n",
            ],
        );
    }

    #[test]
    fn output_for_other_files_is_an_error() {
        let output = "=== b.yaml\nempty\n=== a.yaml\nempty\n";
        let error = split_output(output, &paths(&["a.yaml", "b.yaml"])).unwrap_err();
        assert!(format!("{error:#}").contains("a.yaml"), "{error:#}");

        let error = split_output("=== a.yaml\nempty\n", &paths(&["a.yaml", "b.yaml"])).unwrap_err();
        assert!(format!("{error:#}").contains("b.yaml"), "{error:#}");

        assert!(split_output("empty\n", &paths(&[])).is_err());
    }

    fn program_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("goyaml-dump")
    }

    /// The go-yaml module and version in the reference's `go.mod`.
    const GO_YAML_REQUIREMENT: &str = "go.yaml.in/yaml/v4 v4.0.0-rc.3";

    #[test]
    fn program_requires_the_go_yaml_version_of_the_reference() {
        let go_mod = fs::read_to_string(program_dir().join("go.mod")).unwrap();
        let required = go_mod
            .lines()
            .filter_map(|line| line.trim().strip_prefix("require "))
            .collect::<Vec<_>>();
        assert_eq!(required, [GO_YAML_REQUIREMENT]);
    }

    #[test]
    fn missing_go_names_the_task_and_the_version() {
        let dir = tempfile::tempdir().unwrap();
        let go = OsString::from("workflint-test-no-such-go");
        let error = build(&go, &program_dir(), &dir.path().join("dump"), "go1.26.4").unwrap_err();
        let message = format!("{error:#}");
        assert!(
            message.contains("building the go-yaml dump needs go1.26.4"),
            "{message}"
        );
    }
}
