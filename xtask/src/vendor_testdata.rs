//! `cargo xtask vendor-testdata`: copies the reference's test data, and the
//! other upstream files that the parity checklist cites, into
//! `tests/conformance/actionlint/` (ADR 0004).
//!
//! The copy keeps the upstream relative paths. The command owns every file in
//! the directory except `README.md`, so it also removes files that upstream
//! does not have. It then lists the time zones of the pinned Go in
//! `tests/conformance/go/time-zones.txt` (see [`time_zones`]).

mod time_zones;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::reference::{self, Layout};

/// The upstream paths to copy, relative to the checkout. A directory stands for
/// every tracked file under it.
pub(crate) const PATHS: &[&str] = &[
    "testdata",
    "docs/checks.md",
    "scripts/generate-actionlint-matcher/test",
    ".github/actionlint-matcher.json",
    "expr_sema.go",
    "availability.go",
    "expr_insecure.go",
    "all_webhooks.go",
    "rule_action.go",
    "rule_deprecated_commands.go",
    "rule_events.go",
    "rule_permissions.go",
    "rule_runner_label.go",
    "rule_shell_name.go",
    "cmd/actionlint/main.go",
    "LICENSE.txt",
];

/// The vendored copy, relative to the repository root.
pub(crate) const DESTINATION: &str = "tests/conformance/actionlint";

/// The file in [`DESTINATION`] that workflint writes and the command keeps.
const README: &str = "README.md";

/// What [`mirror`] changed.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Summary {
    /// The number of files in the copy, without `README.md`.
    pub(crate) files: usize,
    /// Files that the copy did not have.
    pub(crate) added: usize,
    /// Files whose contents differed from upstream.
    pub(crate) updated: usize,
    /// Files that upstream does not have.
    pub(crate) removed: usize,
}

/// Makes `.reference/actionlint` a clean checkout of the reference commit,
/// then copies [`PATHS`] from it into [`DESTINATION`] and prints what changed.
/// Then writes the time zones of the pinned Go, which needs that Go.
pub(crate) fn run(layout: &Layout) -> Result<()> {
    reference::report_checkout(layout)?;
    let files = reference::tracked_files(layout, PATHS)?;
    check_every_path_matched(&files)?;
    let destination = layout.root().join(DESTINATION);
    let summary = mirror(&layout.checkout(), &files, &destination)?;
    let path = layout.display(&destination);
    if summary.added + summary.updated + summary.removed == 0 {
        println!(
            "vendor-testdata: {path} is up to date with {} files",
            summary.files
        );
    } else {
        println!(
            "vendor-testdata: copied {} files into {path} ({} added, {} updated, {} removed)",
            summary.files, summary.added, summary.updated, summary.removed
        );
    }
    time_zones::run(layout)
}

/// Fails if an entry of [`PATHS`] matched no tracked file, which means the
/// path is wrong.
fn check_every_path_matched(files: &[String]) -> Result<()> {
    let unmatched: Vec<&str> = PATHS
        .iter()
        .copied()
        .filter(|path| {
            !files.iter().any(|file| {
                file == path
                    || file
                        .strip_prefix(path)
                        .is_some_and(|rest| rest.starts_with('/'))
            })
        })
        .collect();
    if !unmatched.is_empty() {
        bail!(
            "the checkout tracks no file at these paths:\n  {}",
            unmatched.join("\n  ")
        );
    }
    Ok(())
}

/// Makes `destination` hold exactly `files`, copied byte for byte from
/// `source`, plus its own `README.md`. `files` are relative to `source` and use
/// `/` separators.
///
/// A file that already has the upstream contents is not written again.
/// Directories that end up empty are removed.
pub(crate) fn mirror(source: &Path, files: &[String], destination: &Path) -> Result<Summary> {
    let (existing, directories) = walk(destination)?;
    let mut summary = Summary {
        files: files.len(),
        ..Summary::default()
    };

    for file in files {
        let from = source.join(file);
        let contents =
            fs::read(&from).with_context(|| format!("cannot read {}", from.display()))?;
        let to = destination.join(file);
        if existing.contains(file) {
            let current = fs::read(&to).with_context(|| format!("cannot read {}", to.display()))?;
            if current == contents {
                continue;
            }
            summary.updated += 1;
        } else {
            summary.added += 1;
        }
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        fs::write(&to, contents).with_context(|| format!("cannot write {}", to.display()))?;
    }

    let wanted: BTreeSet<&str> = files.iter().map(String::as_str).collect();
    for file in &existing {
        if file != README && !wanted.contains(file.as_str()) {
            let path = destination.join(file);
            fs::remove_file(&path).with_context(|| format!("cannot remove {}", path.display()))?;
            summary.removed += 1;
        }
    }

    // `walk` lists each directory before the directories inside it.
    for directory in directories.iter().rev() {
        let mut entries = fs::read_dir(directory)
            .with_context(|| format!("cannot read {}", directory.display()))?;
        if entries.next().is_none() {
            fs::remove_dir(directory)
                .with_context(|| format!("cannot remove {}", directory.display()))?;
        }
    }
    Ok(summary)
}

/// The files under `dir`, relative and with `/` separators, and the
/// directories under it, each before its subdirectories. A missing `dir` has
/// neither. Symbolic links count as files.
fn walk(dir: &Path) -> Result<(BTreeSet<String>, Vec<PathBuf>)> {
    let mut files = BTreeSet::new();
    let mut directories = Vec::new();
    if !dir.exists() {
        return Ok((files, directories));
    }
    let mut pending = vec![(dir.to_path_buf(), String::new())];
    let mut next = 0;
    while let Some((absolute, relative)) = pending.get(next).cloned() {
        next += 1;
        let entries = fs::read_dir(&absolute)
            .with_context(|| format!("cannot read {}", absolute.display()))?;
        for entry in entries {
            let entry = entry.with_context(|| format!("cannot read {}", absolute.display()))?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                bail!("{} has a name that is not UTF-8", entry.path().display());
            };
            let key = if relative.is_empty() {
                name.to_owned()
            } else {
                format!("{relative}/{name}")
            };
            let metadata = fs::symlink_metadata(entry.path())
                .with_context(|| format!("cannot read {}", entry.path().display()))?;
            if metadata.is_dir() {
                directories.push(entry.path());
                pending.push((entry.path(), key));
            } else {
                files.insert(key);
            }
        }
    }
    Ok((files, directories))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    use super::*;

    struct Fixture {
        _dir: tempfile::TempDir,
        source: PathBuf,
        destination: PathBuf,
    }

    /// Files in an upstream-like source directory, including a hidden
    /// directory, a CRLF file, and bytes that are not UTF-8.
    const SOURCE: &[(&str, &[u8])] = &[
        ("LICENSE.txt", b"MIT\n"),
        ("testdata/ok/a.yaml", b"on: push\n"),
        ("testdata/ok/crlf.yaml", b"on: push\r\njobs: {}\r\n"),
        (
            "testdata/err/.github/workflows/called-workflow.yml",
            b"on: workflow_call\n",
        ),
        ("testdata/realworld/dataset.zip", b"PK\x03\x04\xff\x00"),
    ];

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("actionlint");
        for (path, contents) in SOURCE {
            let path = source.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        let destination = dir
            .path()
            .join("tests")
            .join("conformance")
            .join("actionlint");
        Fixture {
            _dir: dir,
            source,
            destination,
        }
    }

    fn source_files() -> Vec<String> {
        SOURCE.iter().map(|(path, _)| (*path).to_owned()).collect()
    }

    /// Every file under `dir`, relative and with `/` separators, in sorted
    /// order.
    fn files_under(dir: &Path) -> Vec<String> {
        let mut found = Vec::new();
        let mut pending = vec![dir.to_path_buf()];
        while let Some(current) = pending.pop() {
            for entry in fs::read_dir(&current).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    pending.push(path);
                } else {
                    let relative = path.strip_prefix(dir).unwrap();
                    found.push(relative.to_str().unwrap().replace('\\', "/"));
                }
            }
        }
        found.sort();
        found
    }

    #[test]
    fn missing_copy_gets_every_file_byte_for_byte() {
        let fixture = fixture();

        let summary = mirror(&fixture.source, &source_files(), &fixture.destination).unwrap();

        assert_eq!(
            summary,
            Summary {
                files: 5,
                added: 5,
                ..Summary::default()
            }
        );
        for (path, contents) in SOURCE {
            assert_eq!(
                &fs::read(fixture.destination.join(path)).unwrap(),
                contents,
                "{path}"
            );
        }
    }

    #[test]
    fn second_run_changes_nothing() {
        let fixture = fixture();
        mirror(&fixture.source, &source_files(), &fixture.destination).unwrap();

        let summary = mirror(&fixture.source, &source_files(), &fixture.destination).unwrap();

        assert_eq!(
            summary,
            Summary {
                files: 5,
                ..Summary::default()
            }
        );
    }

    #[test]
    fn edited_file_gets_the_upstream_contents_back() {
        let fixture = fixture();
        mirror(&fixture.source, &source_files(), &fixture.destination).unwrap();
        let edited = fixture.destination.join("testdata/ok/crlf.yaml");
        fs::write(&edited, "on: push\njobs: {}\n").unwrap();

        let summary = mirror(&fixture.source, &source_files(), &fixture.destination).unwrap();

        assert_eq!(summary.updated, 1);
        assert_eq!(fs::read(edited).unwrap(), b"on: push\r\njobs: {}\r\n");
    }

    #[test]
    fn file_that_upstream_lacks_is_removed_with_its_empty_directory() {
        let fixture = fixture();
        mirror(&fixture.source, &source_files(), &fixture.destination).unwrap();
        let stale = fixture.destination.join("testdata/gone/stale.yaml");
        fs::create_dir_all(stale.parent().unwrap()).unwrap();
        fs::write(&stale, "on: push\n").unwrap();

        let summary = mirror(&fixture.source, &source_files(), &fixture.destination).unwrap();

        assert_eq!(summary.removed, 1);
        assert!(!fixture.destination.join("testdata/gone").exists());
        assert_eq!(files_under(&fixture.destination), {
            let mut expected = source_files();
            expected.sort();
            expected
        });
    }

    #[test]
    fn readme_in_the_copy_stays() {
        let fixture = fixture();
        fs::create_dir_all(&fixture.destination).unwrap();
        fs::write(fixture.destination.join(README), "# Vendored\n").unwrap();

        let summary = mirror(&fixture.source, &source_files(), &fixture.destination).unwrap();

        assert_eq!(summary.removed, 0);
        assert_eq!(
            fs::read_to_string(fixture.destination.join(README)).unwrap(),
            "# Vendored\n"
        );
    }

    #[test]
    fn readme_below_the_top_level_is_not_kept() {
        let fixture = fixture();
        mirror(&fixture.source, &source_files(), &fixture.destination).unwrap();
        let nested = fixture.destination.join("testdata").join(README);
        fs::write(&nested, "# Stale\n").unwrap();

        let summary = mirror(&fixture.source, &source_files(), &fixture.destination).unwrap();

        assert_eq!(summary.removed, 1);
        assert!(!nested.exists());
    }

    #[test]
    fn missing_source_file_is_an_error_that_names_it() {
        let fixture = fixture();
        let mut files = source_files();
        files.push("testdata/ok/missing.yaml".to_owned());

        let error = mirror(&fixture.source, &files, &fixture.destination).unwrap_err();

        assert!(format!("{error:#}").contains("missing.yaml"), "{error:#}");
    }

    #[test]
    fn path_that_matches_no_file_is_an_error_that_names_it() {
        let files = vec!["testdata/ok/a.yaml".to_owned(), "LICENSE.txt".to_owned()];

        let error = check_every_path_matched(&files).unwrap_err();

        assert!(format!("{error:#}").contains("docs/checks.md"), "{error:#}");
    }

    #[test]
    fn path_matches_the_file_itself_or_files_below_it() {
        let files = vec![
            "testdata/ok/a.yaml".to_owned(),
            "docs/checks.md".to_owned(),
            "scripts/generate-actionlint-matcher/test/want.json".to_owned(),
            ".github/actionlint-matcher.json".to_owned(),
            "expr_sema.go".to_owned(),
            "availability.go".to_owned(),
            "expr_insecure.go".to_owned(),
            "all_webhooks.go".to_owned(),
            "rule_action.go".to_owned(),
            "rule_deprecated_commands.go".to_owned(),
            "rule_events.go".to_owned(),
            "rule_permissions.go".to_owned(),
            "rule_runner_label.go".to_owned(),
            "rule_shell_name.go".to_owned(),
            "cmd/actionlint/main.go".to_owned(),
            "LICENSE.txt".to_owned(),
        ];

        check_every_path_matched(&files).unwrap();
    }

    // The tests below check the vendored copy in this repository.

    fn repository_root() -> PathBuf {
        Layout::repository().root().to_path_buf()
    }

    fn git_in_repository(args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(repository_root())
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    #[test]
    fn vendored_copy_holds_the_license_and_names_the_commit_and_license() {
        let root = repository_root();
        let copy = root.join(DESTINATION);

        let license = fs::read_to_string(copy.join("LICENSE.txt")).unwrap();
        let readme = fs::read_to_string(copy.join(README)).unwrap();
        let notice = fs::read_to_string(root.join("NOTICE")).unwrap();

        assert!(license.contains("MIT License"), "{license}");
        assert!(readme.contains(reference::COMMIT), "{readme}");
        assert!(readme.contains("MIT"), "{readme}");
        assert!(readme.contains("cargo xtask vendor-testdata"), "{readme}");
        assert!(notice.contains(reference::COMMIT), "{notice}");
    }

    #[test]
    fn git_tracks_the_hidden_directories_in_the_vendored_copy() {
        let tracked = git_in_repository(&["ls-files", "-z", "--", DESTINATION]);
        let tracked: BTreeSet<&str> = tracked.split('\0').collect();

        for path in [
            "testdata/err/.github/workflows/called-workflow.yml",
            "testdata/realworld/.gitignore",
            "testdata/realworld/dataset.zip",
        ] {
            let path = format!("{DESTINATION}/{path}");
            assert!(tracked.contains(path.as_str()), "{path} is not tracked");
        }
    }

    /// On Windows, this fails if git converted line endings at checkout.
    #[test]
    fn git_checks_out_the_vendored_copy_without_converting_line_endings() {
        let listing = git_in_repository(&["ls-files", "--eol", "--", DESTINATION]);
        let mut checked = 0;
        for line in listing.lines() {
            // `i/lf    w/lf    attr/-text            \tpath`
            let (columns, path) = line.split_once('\t').unwrap();
            let columns: Vec<&str> = columns.split_whitespace().collect();
            let [index, worktree, attributes] = columns[..] else {
                panic!("unexpected `git ls-files --eol` line: {line}");
            };
            assert_eq!(attributes, "attr/-text", "{path}");
            assert_eq!(
                index.trim_start_matches("i/"),
                worktree.trim_start_matches("w/"),
                "{path}"
            );
            checked += 1;
        }
        assert!(checked > 600, "only {checked} vendored files are tracked");
    }
}
