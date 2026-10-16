//! Reads the pinned Go version and checks the installed toolchain against it.

use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

/// A Go version as `go version` reports it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum GoVersion {
    /// A release or release candidate, for example `go1.26.4`. `GOEXPERIMENT`
    /// settings that `go version` prints as `X:name` are not part of it.
    Release(String),
    /// A toolchain built from source, with the text between `devel` and the
    /// platform.
    Devel(String),
}

/// Parses the output of `go version`, for example
/// `go version go1.26.4 darwin/arm64`.
pub(crate) fn parse_go_version(output: &str) -> Option<GoVersion> {
    let rest = output.trim().strip_prefix("go version ")?;
    if let Some(devel) = rest.strip_prefix("devel ") {
        // The last word is the platform, for example `linux/amd64`.
        let (description, _platform) = devel.rsplit_once(' ')?;
        return Some(GoVersion::Devel(description.to_owned()));
    }
    let version = rest.split(' ').next()?;
    let number = version.strip_prefix("go")?;
    if !number.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    Some(GoVersion::Release(version.to_owned()))
}

/// Reads the pinned version from the Go version file, which holds a version
/// such as `1.26.4` for `actions/setup-go`, and returns it as `go1.26.4`.
pub(crate) fn read_pinned_version(path: &Path) -> Result<String> {
    let text =
        fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let version = text.trim();
    let valid = !version.is_empty()
        && version
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()));
    if !valid {
        bail!(
            "{} must hold one Go version such as 1.26.4, but it holds {version:?}",
            path.display()
        );
    }
    Ok(format!("go{version}"))
}

/// Runs `go version` in `dir` and fails unless it reports `pinned`.
///
/// Running it in the checkout applies the same toolchain selection as the
/// build, so a `GOTOOLCHAIN` setting counts.
pub(crate) fn require_version(go: &OsString, dir: &Path, pinned: &str) -> Result<()> {
    let how = format!(
        "Install {pinned} from https://go.dev/dl/, or set GOTOOLCHAIN={pinned} \
         to make an installed Go 1.21 or later download it."
    );
    let output = match Command::new(go).arg("version").current_dir(dir).output() {
        Ok(output) => output,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            bail!(
                "building the reference needs {pinned}, but `{}` was not found on PATH. {how}",
                go.display()
            );
        }
        Err(error) => {
            return Err(error).with_context(|| format!("cannot run `{} version`", go.display()));
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        bail!(
            "`{} version` failed: {}",
            go.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let found = match parse_go_version(&stdout) {
        Some(GoVersion::Release(version)) if version == pinned => return Ok(()),
        Some(GoVersion::Release(version)) => version,
        Some(GoVersion::Devel(description)) => format!("the development build {description}"),
        None => format!("{:?}", stdout.trim()),
    };
    bail!("building the reference needs {pinned}, but `go version` reports {found}. {how}")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn release(version: &str) -> GoVersion {
        GoVersion::Release(version.to_owned())
    }

    #[test]
    fn release_build_reports_its_version() {
        assert_eq!(
            parse_go_version("go version go1.26.4 darwin/arm64\n"),
            Some(release("go1.26.4")),
        );
    }

    #[test]
    fn release_candidate_reports_its_version() {
        assert_eq!(
            parse_go_version("go version go1.27rc1 linux/amd64\n"),
            Some(release("go1.27rc1")),
        );
    }

    #[test]
    fn experiments_are_not_part_of_the_version() {
        assert_eq!(
            parse_go_version("go version go1.26.4 X:boringcrypto,nodwarf5 linux/amd64\n"),
            Some(release("go1.26.4")),
        );
    }

    #[test]
    fn devel_build_keeps_its_description() {
        assert_eq!(
            parse_go_version(
                "go version devel go1.27-2a3d4e5f6a Tue Oct 13 18:00:00 2026 +0000 linux/amd64\n"
            ),
            Some(GoVersion::Devel(
                "go1.27-2a3d4e5f6a Tue Oct 13 18:00:00 2026 +0000".to_owned()
            )),
        );
    }

    #[test]
    fn old_devel_build_without_a_version_is_devel() {
        assert_eq!(
            parse_go_version(
                "go version devel +b7a85e0003 Fri Oct 4 10:00:00 2019 +0000 linux/amd64"
            ),
            Some(GoVersion::Devel(
                "+b7a85e0003 Fri Oct 4 10:00:00 2019 +0000".to_owned()
            )),
        );
    }

    #[test]
    fn unrelated_output_is_not_a_version() {
        assert_eq!(parse_go_version(""), None);
        assert_eq!(parse_go_version("go: unknown command\n"), None);
        assert_eq!(parse_go_version("go version 1.26.4 linux/amd64"), None);
    }

    #[test]
    fn pinned_version_file_gains_the_go_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".go-version");
        fs::write(&path, "1.26.4\n").unwrap();
        assert_eq!(read_pinned_version(&path).unwrap(), "go1.26.4");
    }

    #[test]
    fn pinned_version_file_must_hold_one_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".go-version");
        for contents in ["", "\n", "1.26.4\n1.25.0\n", "latest\n", "go1.26.4\n"] {
            fs::write(&path, contents).unwrap();
            assert!(read_pinned_version(&path).is_err(), "accepted {contents:?}");
        }
    }

    #[test]
    fn missing_go_names_the_required_version() {
        let dir = tempfile::tempdir().unwrap();
        let go = OsString::from("workflint-test-no-such-go");
        let error = require_version(&go, dir.path(), "go1.26.4").unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("go1.26.4"), "{message}");
        assert!(message.contains("not found"), "{message}");
    }

    #[cfg(unix)]
    fn fake_go(dir: &Path, output: &str) -> OsString {
        use std::os::unix::fs::PermissionsExt;

        let path = dir.join("go");
        fs::write(&path, format!("#!/bin/sh\necho '{output}'\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path.into_os_string()
    }

    #[cfg(unix)]
    #[test]
    fn different_go_version_names_both_versions() {
        let dir = tempfile::tempdir().unwrap();
        let go = fake_go(dir.path(), "go version go1.25.0 linux/amd64");
        let error = require_version(&go, dir.path(), "go1.26.4").unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("go1.26.4"), "{message}");
        assert!(message.contains("go1.25.0"), "{message}");
        assert!(message.contains("GOTOOLCHAIN=go1.26.4"), "{message}");
    }

    #[cfg(unix)]
    #[test]
    fn devel_go_is_not_the_pinned_version() {
        let dir = tempfile::tempdir().unwrap();
        let go = fake_go(
            dir.path(),
            "go version devel go1.27-2a3d4e5f6a Tue Oct 13 18:00:00 2026 +0000 linux/amd64",
        );
        let error = require_version(&go, dir.path(), "go1.26.4").unwrap_err();
        assert!(format!("{error:#}").contains("go1.26.4"));
    }

    #[cfg(unix)]
    #[test]
    fn pinned_go_version_passes() {
        let dir = tempfile::tempdir().unwrap();
        let go = fake_go(dir.path(), "go version go1.26.4 X:nodwarf5 linux/amd64");
        require_version(&go, dir.path(), "go1.26.4").unwrap();
    }
}
