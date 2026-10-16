//! Step 2: the reference binary built from the checkout.

use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs;
use std::io::ErrorKind;
use std::path::{self, Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use super::{Outcome, go};

/// The `go build` flags. The stamp records them, so changing them here builds
/// the binary again.
const BUILD_FLAGS: [&str; 1] = ["-trimpath"];

/// The environment that `go build` runs with on top of the caller's.
const BUILD_ENVIRONMENT: [(&str, &str); 1] = [("CGO_ENABLED", "0")];

/// The environment variables from the caller's environment that change what
/// `go build` produces. Settings written with `go env -w` are not read, so
/// that an up-to-date check needs no Go toolchain.
const BUILD_VARIABLES: [&str; 9] = [
    "GOFLAGS",
    "GOOS",
    "GOARCH",
    "GOAMD64",
    "GOARM",
    "GOARM64",
    "GO386",
    "GOEXPERIMENT",
    "GOWORK",
];

/// The [`BUILD_VARIABLES`] that are set and not empty in the environment of
/// this process, in that order.
pub(crate) fn build_variables() -> Vec<(String, String)> {
    BUILD_VARIABLES
        .iter()
        .filter_map(|name| {
            let value = std::env::var(name).ok().filter(|value| !value.is_empty())?;
            Some(((*name).to_owned(), value))
        })
        .collect()
}

/// What a build of the reference binary came from: the commit, the Go
/// version, the build command, and the build variables. The build writes it
/// next to the binary. A later run builds again unless the file holds exactly
/// what the current pins and environment render to.
#[derive(Debug)]
struct Stamp<'a> {
    commit: &'a str,
    go: &'a str,
    variables: &'a [(String, String)],
}

impl Stamp<'_> {
    fn render(&self) -> String {
        let environment: Vec<String> = BUILD_ENVIRONMENT
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect();
        let mut text = format!(
            "commit = {}\ngo = {}\nbuild = {} go build {}\n",
            self.commit,
            self.go,
            environment.join(" "),
            BUILD_FLAGS.join(" ")
        );
        for (name, value) in self.variables {
            let _ = writeln!(text, "env {name} = {value}");
        }
        text
    }
}

/// The stamp file for `binary`, for example `.reference/bin/actionlint.stamp`.
pub(crate) fn stamp_path(binary: &Path) -> PathBuf {
    let mut name = binary.file_name().unwrap_or_default().to_os_string();
    name.push(".stamp");
    binary.with_file_name(name)
}

/// Builds `./cmd/actionlint` from `checkout` into `binary` with Go `go_version`,
/// unless the stamp next to `binary` already records `commit`, `go_version`,
/// the build command, and `variables`. `go` is the Go command to run.
/// `variables` are the [`build_variables`] of the caller's environment, which
/// the build inherits.
///
/// An up-to-date binary needs no Go toolchain. Otherwise the installed Go must
/// report `go_version` before the build starts.
pub(crate) fn ensure(
    checkout: &Path,
    binary: &Path,
    commit: &str,
    go: &OsString,
    go_version: &str,
    variables: &[(String, String)],
) -> Result<Outcome> {
    let expected = Stamp {
        commit,
        go: go_version,
        variables,
    }
    .render();
    let stamp = stamp_path(binary);
    let existed = binary.is_file();
    let recorded = fs::read_to_string(&stamp).ok();
    if existed && recorded.as_deref() == Some(expected.as_str()) {
        return Ok(Outcome::UpToDate);
    }

    go::require_version(go, checkout, go_version, "building the reference")?;

    // Without a stamp, a run that stops during the build builds again next time.
    match fs::remove_file(&stamp) {
        Err(error) if error.kind() != ErrorKind::NotFound => {
            return Err(error).with_context(|| format!("cannot remove {}", stamp.display()));
        }
        _ => {}
    }
    let output =
        path::absolute(binary).with_context(|| format!("cannot resolve {}", binary.display()))?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    let status = Command::new(go)
        .arg("build")
        .args(BUILD_FLAGS)
        .arg("-o")
        .arg(&output)
        .arg("./cmd/actionlint")
        .current_dir(checkout)
        .envs(BUILD_ENVIRONMENT)
        .status()
        .with_context(|| format!("cannot run `{} build`", go.display()))?;
    if !status.success() {
        bail!("`go build` of the reference failed with {status}");
    }

    let version = Command::new(&output)
        .arg("-version")
        .output()
        .with_context(|| format!("cannot run {}", output.display()))?;
    if !version.status.success() {
        bail!(
            "{} -version failed with {}: {}",
            output.display(),
            version.status,
            String::from_utf8_lossy(&version.stderr).trim()
        );
    }

    fs::write(&stamp, expected).with_context(|| format!("cannot write {}", stamp.display()))?;
    Ok(if existed {
        Outcome::Updated
    } else {
        Outcome::Created
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const COMMIT: &str = "011a6d15e749bb3f2d771eed9c7aa0e7e3e10ee7";

    fn stamp(commit: &str, go: &str) -> String {
        Stamp {
            commit,
            go,
            variables: &[],
        }
        .render()
    }

    fn no_go() -> OsString {
        OsString::from("workflint-test-no-such-go")
    }

    fn goflags(value: &str) -> Vec<(String, String)> {
        vec![("GOFLAGS".to_owned(), value.to_owned())]
    }

    #[test]
    fn stamp_records_the_build_command_and_variables() {
        let variables = goflags("-mod=mod");
        let text = Stamp {
            commit: COMMIT,
            go: "go1.26.4",
            variables: &variables,
        }
        .render();
        assert_eq!(
            text,
            format!(
                "commit = {COMMIT}\ngo = go1.26.4\nbuild = CGO_ENABLED=0 go build -trimpath\n\
                 env GOFLAGS = -mod=mod\n"
            )
        );
    }

    #[test]
    fn binary_with_a_matching_stamp_and_goflags_is_up_to_date_without_go() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("actionlint");
        fs::write(&binary, "binary").unwrap();
        let variables = goflags("-mod=mod");
        let text = Stamp {
            commit: COMMIT,
            go: "go1.26.4",
            variables: &variables,
        }
        .render();
        fs::write(stamp_path(&binary), text).unwrap();

        let outcome = ensure(
            dir.path(),
            &binary,
            COMMIT,
            &no_go(),
            "go1.26.4",
            &variables,
        )
        .unwrap();

        assert_eq!(outcome, Outcome::UpToDate);
    }

    #[test]
    fn binary_built_without_goflags_needs_a_build_when_goflags_is_set() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("actionlint");
        fs::write(&binary, "binary").unwrap();
        fs::write(stamp_path(&binary), stamp(COMMIT, "go1.26.4")).unwrap();

        let error = ensure(
            dir.path(),
            &binary,
            COMMIT,
            &no_go(),
            "go1.26.4",
            &goflags("-race"),
        )
        .unwrap_err();

        assert!(format!("{error:#}").contains("go1.26.4"), "{error:#}");
        assert_eq!(fs::read_to_string(&binary).unwrap(), "binary");
    }

    #[test]
    fn stamp_from_an_older_version_of_the_command_needs_a_build() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("actionlint");
        fs::write(&binary, "binary").unwrap();
        fs::write(
            stamp_path(&binary),
            format!("commit = {COMMIT}\ngo = go1.26.4\n"),
        )
        .unwrap();

        assert!(ensure(dir.path(), &binary, COMMIT, &no_go(), "go1.26.4", &[]).is_err());
    }

    #[test]
    fn stamp_sits_next_to_the_binary() {
        assert_eq!(
            stamp_path(Path::new("ref/bin/actionlint")),
            Path::new("ref/bin/actionlint.stamp"),
        );
        assert_eq!(
            stamp_path(Path::new("ref/bin/actionlint.exe")),
            Path::new("ref/bin/actionlint.exe.stamp"),
        );
    }

    #[test]
    fn binary_with_a_matching_stamp_is_up_to_date_without_go() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("actionlint");
        fs::write(&binary, "binary").unwrap();
        fs::write(stamp_path(&binary), stamp(COMMIT, "go1.26.4")).unwrap();

        let outcome = ensure(dir.path(), &binary, COMMIT, &no_go(), "go1.26.4", &[]).unwrap();

        assert_eq!(outcome, Outcome::UpToDate);
    }

    #[test]
    fn stamp_for_another_go_version_needs_a_build() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("actionlint");
        fs::write(&binary, "binary").unwrap();
        fs::write(stamp_path(&binary), stamp(COMMIT, "go1.25.0")).unwrap();

        let error = ensure(dir.path(), &binary, COMMIT, &no_go(), "go1.26.4", &[]).unwrap_err();

        assert!(format!("{error:#}").contains("go1.26.4"), "{error:#}");
        assert_eq!(fs::read_to_string(&binary).unwrap(), "binary");
    }

    #[test]
    fn stamp_for_another_commit_needs_a_build() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("actionlint");
        fs::write(&binary, "binary").unwrap();
        fs::write(stamp_path(&binary), stamp("other", "go1.26.4")).unwrap();

        assert!(ensure(dir.path(), &binary, COMMIT, &no_go(), "go1.26.4", &[]).is_err());
    }

    #[test]
    fn stamp_without_a_binary_needs_a_build() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("actionlint");
        fs::write(stamp_path(&binary), stamp(COMMIT, "go1.26.4")).unwrap();

        assert!(ensure(dir.path(), &binary, COMMIT, &no_go(), "go1.26.4", &[]).is_err());
    }

    #[test]
    fn missing_go_fails_before_building() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("bin").join("actionlint");

        let error = ensure(dir.path(), &binary, COMMIT, &no_go(), "go1.26.4", &[]).unwrap_err();

        assert!(format!("{error:#}").contains("go1.26.4"), "{error:#}");
        assert!(!binary.exists());
        assert!(!stamp_path(&binary).exists());
    }
}
