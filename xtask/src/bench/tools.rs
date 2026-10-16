//! Finds hyperfine and shellcheck and checks their versions.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// The hyperfine major version whose JSON export (schema version 2) the
/// command reads. CI installs [`HYPERFINE_VERSION`].
pub(crate) const HYPERFINE_MAJOR: u64 = 2;

/// The hyperfine version that CI installs.
pub(crate) const HYPERFINE_VERSION: &str = "2.0.0";

/// The shellcheck version that every benchmark job uses (ADR 0019). It is the
/// version the research measured the reference with.
pub(crate) const SHELLCHECK_VERSION: &str = "0.11.0";

/// Finds `name` in the directories of `PATH`, with `.exe` on Windows.
pub(crate) fn find_in_path(name: &str) -> Option<PathBuf> {
    let file = format!("{name}{}", env::consts::EXE_SUFFIX);
    env::split_paths(&env::var_os("PATH")?)
        .map(|directory| directory.join(&file))
        .find(|candidate| candidate.is_file())
}

/// Runs `program` with `argument` and returns its standard output.
fn version_output(program: &Path, argument: &str) -> Result<String> {
    let output = Command::new(program)
        .arg(argument)
        .output()
        .with_context(|| format!("cannot run {}", program.display()))?;
    if !output.status.success() {
        bail!(
            "{} {argument} failed with {}: {}",
            program.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Parses the output of `hyperfine --version`, for example `hyperfine 2.0.0`.
pub(crate) fn parse_hyperfine_version(output: &str) -> Option<String> {
    let version = output.trim().strip_prefix("hyperfine ")?;
    major(version)?;
    Some(version.to_owned())
}

/// Parses the output of `shellcheck --version`, which has a line such as
/// `version: 0.11.0`.
pub(crate) fn parse_shellcheck_version(output: &str) -> Option<String> {
    output
        .lines()
        .find_map(|line| line.trim().strip_prefix("version: "))
        .map(|version| version.trim().to_owned())
}

fn major(version: &str) -> Option<u64> {
    version.split('.').next()?.parse().ok()
}

/// The hyperfine that the command runs.
#[derive(Debug)]
pub(crate) struct Hyperfine {
    pub(crate) path: PathBuf,
    pub(crate) version: String,
}

/// Finds hyperfine and checks that its major version is
/// [`HYPERFINE_MAJOR`].
pub(crate) fn hyperfine() -> Result<Hyperfine> {
    let Some(path) = find_in_path("hyperfine") else {
        bail!(
            "hyperfine is not installed. Install hyperfine {HYPERFINE_VERSION}, for example \
             with `cargo install --locked hyperfine@{HYPERFINE_VERSION}`"
        );
    };
    let output = version_output(&path, "--version")?;
    let Some(version) = parse_hyperfine_version(&output) else {
        bail!(
            "cannot read the version of {} from {:?}",
            path.display(),
            output.trim()
        );
    };
    if major(&version) != Some(HYPERFINE_MAJOR) {
        bail!(
            "{} is hyperfine {version}, but the command reads the JSON export of hyperfine \
             {HYPERFINE_MAJOR}. Install hyperfine {HYPERFINE_VERSION}",
            path.display()
        );
    }
    Ok(Hyperfine { path, version })
}

/// What the command found when it looked for shellcheck.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Shellcheck {
    /// shellcheck [`SHELLCHECK_VERSION`] at this path.
    Pinned(PathBuf),
    /// shellcheck at this path, with another version.
    OtherVersion(PathBuf, String),
    /// No shellcheck in `PATH`.
    Missing,
}

impl Shellcheck {
    /// The path to pass to `-shellcheck=`, if the shellcheck scenario runs.
    pub(crate) fn pinned(&self) -> Option<&Path> {
        match self {
            Self::Pinned(path) => Some(path),
            Self::OtherVersion(..) | Self::Missing => None,
        }
    }

    /// The version to record in the results.
    pub(crate) fn version(&self) -> Option<&str> {
        match self {
            Self::Pinned(_) => Some(SHELLCHECK_VERSION),
            Self::OtherVersion(_, version) => Some(version),
            Self::Missing => None,
        }
    }

    /// Why the shellcheck scenario does not run, or `None` if it runs.
    pub(crate) fn skip_reason(&self) -> Option<String> {
        match self {
            Self::Pinned(_) => None,
            Self::OtherVersion(path, version) => Some(format!(
                "{} is shellcheck {version}, but the benchmarks use shellcheck \
                 {SHELLCHECK_VERSION}",
                path.display()
            )),
            Self::Missing => Some(format!(
                "shellcheck is not installed. Install shellcheck {SHELLCHECK_VERSION} to run \
                 this scenario"
            )),
        }
    }
}

/// Looks for shellcheck in `PATH` and reads its version.
pub(crate) fn shellcheck() -> Result<Shellcheck> {
    let Some(path) = find_in_path("shellcheck") else {
        return Ok(Shellcheck::Missing);
    };
    shellcheck_at(path)
}

fn shellcheck_at(path: PathBuf) -> Result<Shellcheck> {
    let output = version_output(&path, "--version")?;
    let Some(version) = parse_shellcheck_version(&output) else {
        bail!(
            "cannot read the version of {} from {:?}",
            path.display(),
            output.trim()
        );
    };
    Ok(if version == SHELLCHECK_VERSION {
        Shellcheck::Pinned(path)
    } else {
        Shellcheck::OtherVersion(path, version)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hyperfine_version_is_read_from_its_output() {
        assert_eq!(
            parse_hyperfine_version("hyperfine 2.0.0\n").as_deref(),
            Some("2.0.0")
        );
        assert_eq!(parse_hyperfine_version("hyperfine\n"), None);
        assert_eq!(parse_hyperfine_version("something 2.0.0"), None);
    }

    #[test]
    fn shellcheck_version_is_read_from_its_output() {
        let output = "ShellCheck - shell script analysis tool\nversion: 0.11.0\n\
                      license: GNU General Public License, version 3\n";
        assert_eq!(parse_shellcheck_version(output).as_deref(), Some("0.11.0"));
        assert_eq!(parse_shellcheck_version("ShellCheck\n"), None);
    }

    #[test]
    fn only_the_pinned_shellcheck_runs_the_scenario() {
        let path = PathBuf::from("/usr/bin/shellcheck");
        let pinned = Shellcheck::Pinned(path.clone());
        assert_eq!(pinned.pinned(), Some(path.as_path()));
        assert_eq!(pinned.skip_reason(), None);

        let other = Shellcheck::OtherVersion(path, "0.10.0".to_owned());
        assert_eq!(other.pinned(), None);
        assert_eq!(other.version(), Some("0.10.0"));
        assert!(other.skip_reason().unwrap().contains("0.11.0"));

        assert_eq!(Shellcheck::Missing.pinned(), None);
        assert!(
            Shellcheck::Missing
                .skip_reason()
                .unwrap()
                .contains("not installed")
        );
    }

    #[cfg(unix)]
    #[test]
    fn shellcheck_with_another_version_is_reported_as_such() {
        let directory = tempfile::tempdir().unwrap();
        let fake = directory.path().join("shellcheck");
        crate::bench::test_support::write_script(&fake, "echo 'version: 0.10.0'");
        let found = shellcheck_at(fake.clone()).unwrap();
        assert_eq!(found, Shellcheck::OtherVersion(fake, "0.10.0".to_owned()));
    }
}
