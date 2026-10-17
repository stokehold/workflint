//! Writes `tests/conformance/go/time-zones.txt`: the time zone names in the
//! tzdata that the reference binary embeds, for the `workflint-data` test that
//! compares its table with them.
//!
//! `cmd/actionlint/main.go` imports Go's `time/tzdata`, so the reference's
//! `time.LoadLocation` accepts every name in `lib/time/zoneinfo.zip` of the Go
//! that built it. Go reads the host's zoneinfo first, so on some hosts the
//! reference accepts more names (`docs/differences.md#host-time-zones`).

use std::ffi::OsString;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use zip::ZipArchive;

use crate::reference::{self, Layout, Outcome, go};

/// The file the command writes, relative to the repository root.
pub(crate) const DESTINATION: &str = "tests/conformance/go/time-zones.txt";

/// The archive of zone files in a Go installation, relative to `GOROOT`.
const ZONEINFO_ZIP: &str = "lib/time/zoneinfo.zip";

/// The script that builds [`ZONEINFO_ZIP`] and names its tzdata version,
/// relative to `GOROOT`.
const UPDATE_BASH: &str = "lib/time/update.bash";

/// Lists the zones of the pinned Go and writes [`DESTINATION`] if it differs,
/// then prints one line about it.
pub(crate) fn run(layout: &Layout) -> Result<()> {
    let pinned = reference::pinned_go_version(layout)?;
    let go = OsString::from("go");
    go::require_version(&go, layout.root(), &pinned, "listing Go's time zones")?;
    let goroot = goroot(&go, layout.root())?;
    let names = zip_names(&goroot.join(ZONEINFO_ZIP))?;
    let script = goroot.join(UPDATE_BASH);
    let script =
        fs::read_to_string(&script).with_context(|| format!("cannot read {}", script.display()))?;
    let version = tzdata_version(&script)?;
    let destination = layout.root().join(DESTINATION);
    let outcome = write_if_changed(&destination, &render(&pinned, &version, &names))?;
    let path = layout.display(&destination);
    match outcome {
        Outcome::UpToDate => println!(
            "vendor-testdata: {path} is up to date with {} time zones",
            names.len()
        ),
        Outcome::Created | Outcome::Updated => println!(
            "vendor-testdata: wrote {} time zones from tzdata {version} into {path}",
            names.len()
        ),
    }
    Ok(())
}

/// Runs `go env GOROOT` in `dir`.
fn goroot(go: &OsString, dir: &Path) -> Result<PathBuf> {
    let output = Command::new(go)
        .args(["env", "GOROOT"])
        .current_dir(dir)
        .output()
        .with_context(|| format!("cannot run `{} env GOROOT`", go.display()))?;
    if !output.status.success() {
        bail!(
            "`{} env GOROOT` failed: {}",
            go.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let root = String::from_utf8(output.stdout).context("GOROOT is not UTF-8")?;
    Ok(PathBuf::from(root.trim()))
}

/// The names of the files in the archive at `path`, sorted by byte.
fn zip_names(path: &Path) -> Result<Vec<String>> {
    let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let archive =
        ZipArchive::new(file).with_context(|| format!("cannot read {}", path.display()))?;
    let mut names = Vec::new();
    for name in archive.file_names() {
        let name = name.with_context(|| format!("cannot read {}", path.display()))?;
        if !name.ends_with('/') {
            names.push(name.into_owned());
        }
    }
    names.sort();
    if names.is_empty() {
        bail!("{} holds no time zone", path.display());
    }
    Ok(names)
}

/// The tzdata version that `update.bash` downloads, from its `DATA=` line.
fn tzdata_version(script: &str) -> Result<String> {
    let mut versions = script
        .lines()
        .filter_map(|line| line.trim().strip_prefix("DATA="));
    match (versions.next(), versions.next()) {
        (Some(version), None) if !version.is_empty() => Ok(version.to_owned()),
        _ => bail!("{UPDATE_BASH} must have one `DATA=` line"),
    }
}

/// The contents of [`DESTINATION`]: two comment lines that name the Go and
/// tzdata versions, then one name per line.
fn render(go: &str, tzdata: &str, names: &[String]) -> String {
    let mut text = format!(
        "# The time zones in {ZONEINFO_ZIP} of {go}, tzdata {tzdata}.\n\
         # `cargo xtask vendor-testdata` writes this file. Do not edit it.\n"
    );
    for name in names {
        text.push_str(name);
        text.push('\n');
    }
    text
}

fn write_if_changed(path: &Path, contents: &str) -> Result<Outcome> {
    let outcome = match fs::read_to_string(path) {
        Ok(current) if current == contents => return Ok(Outcome::UpToDate),
        Ok(_) => Outcome::Updated,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Outcome::Created,
        Err(error) => {
            return Err(error).with_context(|| format!("cannot read {}", path.display()));
        }
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    fs::write(path, contents).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tzdata_version_comes_from_the_data_line() {
        let script = "# Versions to use.\nCODE=2025c\nDATA=2025b\n\nset -e\n";
        assert_eq!(tzdata_version(script).unwrap(), "2025b");
    }

    #[test]
    fn script_without_one_data_line_is_an_error() {
        assert!(tzdata_version("CODE=2025c\n").is_err());
        assert!(tzdata_version("DATA=2025c\nDATA=2025d\n").is_err());
        assert!(tzdata_version("DATA=\n").is_err());
    }

    #[test]
    fn rendered_list_names_the_versions_then_one_zone_per_line() {
        let names = ["Africa/Abidjan".to_owned(), "UTC".to_owned()];
        assert_eq!(
            render("go1.26.4", "2025c", &names),
            "# The time zones in lib/time/zoneinfo.zip of go1.26.4, tzdata 2025c.\n\
             # `cargo xtask vendor-testdata` writes this file. Do not edit it.\n\
             Africa/Abidjan\nUTC\n"
        );
    }

    #[test]
    fn unchanged_file_is_not_written_again() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("go").join("time-zones.txt");

        assert_eq!(write_if_changed(&path, "a\n").unwrap(), Outcome::Created);
        assert_eq!(write_if_changed(&path, "a\n").unwrap(), Outcome::UpToDate);
        assert_eq!(write_if_changed(&path, "b\n").unwrap(), Outcome::Updated);
        assert_eq!(fs::read_to_string(&path).unwrap(), "b\n");
    }

    /// The vendored list in this repository comes from the pinned Go.
    #[test]
    fn vendored_list_names_the_pinned_go_version() {
        let layout = Layout::repository();
        let pinned = reference::pinned_go_version(&layout).unwrap();
        let list = fs::read_to_string(layout.root().join(DESTINATION)).unwrap();
        let first = list.lines().next().unwrap();
        assert!(first.contains(&format!(" of {pinned}, ")), "{first}");
    }
}
