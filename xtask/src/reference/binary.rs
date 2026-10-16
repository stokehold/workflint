//! Step 2: the reference binary built from the checkout.

use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{self, Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use super::{Outcome, go};

/// What a build of the reference binary came from. The build writes it next to
/// the binary, and a later run compares it with the pins to decide whether to
/// build again.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Stamp {
    pub(crate) commit: String,
    pub(crate) go: String,
}

impl Stamp {
    fn parse(text: &str) -> Option<Self> {
        let mut commit = None;
        let mut go = None;
        for line in text.lines() {
            match line.split_once(" = ")? {
                ("commit", value) => commit = Some(value.to_owned()),
                ("go", value) => go = Some(value.to_owned()),
                _ => {}
            }
        }
        Some(Self {
            commit: commit?,
            go: go?,
        })
    }

    fn render(&self) -> String {
        format!("commit = {}\ngo = {}\n", self.commit, self.go)
    }
}

/// The stamp file for `binary`, for example `.reference/bin/actionlint.stamp`.
pub(crate) fn stamp_path(binary: &Path) -> PathBuf {
    let mut name = binary.file_name().unwrap_or_default().to_os_string();
    name.push(".stamp");
    binary.with_file_name(name)
}

/// Builds `./cmd/actionlint` from `checkout` into `binary` with Go `go_version`,
/// unless the stamp next to `binary` already records `commit` and
/// `go_version`. `go` is the Go command to run.
///
/// An up-to-date binary needs no Go toolchain. Otherwise the installed Go must
/// report `go_version` before the build starts.
pub(crate) fn ensure(
    checkout: &Path,
    binary: &Path,
    commit: &str,
    go: &OsString,
    go_version: &str,
) -> Result<Outcome> {
    let expected = Stamp {
        commit: commit.to_owned(),
        go: go_version.to_owned(),
    };
    let stamp = stamp_path(binary);
    let existed = binary.is_file();
    let recorded = fs::read_to_string(&stamp)
        .ok()
        .and_then(|text| Stamp::parse(&text));
    if existed && recorded.as_ref() == Some(&expected) {
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
        .args(["build", "-trimpath", "-o"])
        .arg(&output)
        .arg("./cmd/actionlint")
        .current_dir(checkout)
        .env("CGO_ENABLED", "0")
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

    fs::write(&stamp, expected.render())
        .with_context(|| format!("cannot write {}", stamp.display()))?;
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

    fn stamp(commit: &str, go: &str) -> Stamp {
        Stamp {
            commit: commit.to_owned(),
            go: go.to_owned(),
        }
    }

    fn no_go() -> OsString {
        OsString::from("workflint-test-no-such-go")
    }

    #[test]
    fn stamp_round_trips() {
        let original = stamp(COMMIT, "go1.26.4");
        assert_eq!(Stamp::parse(&original.render()), Some(original));
    }

    #[test]
    fn stamp_without_both_fields_does_not_parse() {
        assert_eq!(Stamp::parse(""), None);
        assert_eq!(Stamp::parse(&format!("commit = {COMMIT}\n")), None);
        assert_eq!(Stamp::parse("go = go1.26.4\n"), None);
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
        fs::write(stamp_path(&binary), stamp(COMMIT, "go1.26.4").render()).unwrap();

        let outcome = ensure(dir.path(), &binary, COMMIT, &no_go(), "go1.26.4").unwrap();

        assert_eq!(outcome, Outcome::UpToDate);
    }

    #[test]
    fn stamp_for_another_go_version_needs_a_build() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("actionlint");
        fs::write(&binary, "binary").unwrap();
        fs::write(stamp_path(&binary), stamp(COMMIT, "go1.25.0").render()).unwrap();

        let error = ensure(dir.path(), &binary, COMMIT, &no_go(), "go1.26.4").unwrap_err();

        assert!(format!("{error:#}").contains("go1.26.4"), "{error:#}");
        assert_eq!(fs::read_to_string(&binary).unwrap(), "binary");
    }

    #[test]
    fn stamp_for_another_commit_needs_a_build() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("actionlint");
        fs::write(&binary, "binary").unwrap();
        fs::write(stamp_path(&binary), stamp("other", "go1.26.4").render()).unwrap();

        assert!(ensure(dir.path(), &binary, COMMIT, &no_go(), "go1.26.4").is_err());
    }

    #[test]
    fn stamp_without_a_binary_needs_a_build() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("actionlint");
        fs::write(stamp_path(&binary), stamp(COMMIT, "go1.26.4").render()).unwrap();

        assert!(ensure(dir.path(), &binary, COMMIT, &no_go(), "go1.26.4").is_err());
    }

    #[test]
    fn missing_go_fails_before_building() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("bin").join("actionlint");

        let error = ensure(dir.path(), &binary, COMMIT, &no_go(), "go1.26.4").unwrap_err();

        assert!(format!("{error:#}").contains("go1.26.4"), "{error:#}");
        assert!(!binary.exists());
        assert!(!stamp_path(&binary).exists());
    }
}
