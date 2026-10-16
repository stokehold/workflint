//! Step 3: the real-world corpus unpacked from the checkout's `dataset.zip`.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use zip::ZipArchive;

use super::Outcome;

/// Files by their path relative to the corpus, with `/` separators, mapped to
/// their size and the CRC-32 of their content.
type Listing = BTreeMap<String, Fingerprint>;

/// The size and CRC-32 of a file. The archive stores both for each entry, so
/// comparing them needs no unpacking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fingerprint {
    size: u64,
    crc32: u32,
}

/// Unpacks `archive` into `target`, unless `target` already holds exactly the
/// archive's files with the same content.
///
/// The files go into a sibling directory first, which is then renamed to
/// `target`, so an interrupted run leaves no partial corpus. An existing
/// `target` that does not match the archive is an error, and nothing changes.
/// Returns the outcome and the number of files in the archive.
pub(crate) fn ensure(archive: &Path, target: &Path) -> Result<(Outcome, usize)> {
    let file = File::open(archive).with_context(|| format!("cannot open {}", archive.display()))?;
    let mut zip =
        ZipArchive::new(file).with_context(|| format!("cannot read {}", archive.display()))?;
    let expected = archive_listing(&mut zip)
        .with_context(|| format!("cannot unpack {}", archive.display()))?;

    if target.exists() {
        let found = directory_listing(target)?;
        if found == expected {
            return Ok((Outcome::UpToDate, expected.len()));
        }
        bail!(
            "{} does not match {}, so it was left as it is. Remove it and run the command \
             again.{}",
            target.display(),
            archive.display(),
            differences(&expected, &found)
        );
    }

    let (Some(parent), Some(name)) = (target.parent(), target.file_name()) else {
        bail!("{} has no parent directory", target.display());
    };
    let partial = parent.join(format!(".{}.partial", name.to_string_lossy()));
    // Only an interrupted run of this step leaves this directory behind.
    if partial.exists() {
        fs::remove_dir_all(&partial)
            .with_context(|| format!("cannot remove {}", partial.display()))?;
    }
    let result = unpack(&mut zip, &partial).and_then(|()| {
        fs::rename(&partial, target).with_context(|| {
            format!(
                "cannot rename {} to {}",
                partial.display(),
                target.display()
            )
        })
    });
    if result.is_err() {
        let _ = fs::remove_dir_all(&partial);
    }
    result?;
    Ok((Outcome::Created, expected.len()))
}

/// The files in `zip`. Fails on an entry whose path would leave the target
/// directory, and on symbolic links.
fn archive_listing(zip: &mut ZipArchive<File>) -> Result<Listing> {
    let mut listing = Listing::new();
    for index in 0..zip.len() {
        let entry = zip.by_index(index)?;
        let path = entry_path(&entry)?;
        if entry.is_dir() {
            continue;
        }
        if entry.is_symlink() {
            bail!("entry {} is a symbolic link", entry.name()?);
        }
        listing.insert(
            key(&path),
            Fingerprint {
                size: entry.size(),
                crc32: entry.crc32(),
            },
        );
    }
    Ok(listing)
}

/// The relative path of `entry`, or an error if it is absolute or climbs out
/// with `..`.
fn entry_path<R: io::Read>(entry: &zip::read::ZipFile<'_, R>) -> Result<PathBuf> {
    let name = entry.name()?;
    let path = PathBuf::from(name.as_ref());
    let relative = path.components().next().is_some()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    if !relative || entry.enclosed_name().is_none() {
        bail!("entry {name} would be written outside the target directory");
    }
    Ok(path)
}

fn unpack(zip: &mut ZipArchive<File>, dir: &Path) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let path = dir.join(entry_path(&entry)?);
        if entry.is_dir() {
            fs::create_dir_all(&path)
                .with_context(|| format!("cannot create {}", path.display()))?;
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        let mut file =
            File::create(&path).with_context(|| format!("cannot create {}", path.display()))?;
        io::copy(&mut entry, &mut file)
            .with_context(|| format!("cannot unpack {}", path.display()))?;
    }
    Ok(())
}

/// The files under `dir`, in the form of [`archive_listing`].
fn directory_listing(dir: &Path) -> Result<Listing> {
    let mut listing = Listing::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        let absolute = dir.join(&relative);
        let entries = fs::read_dir(&absolute)
            .with_context(|| format!("cannot read {}", absolute.display()))?;
        for entry in entries {
            let entry = entry.with_context(|| format!("cannot read {}", absolute.display()))?;
            let path = relative.join(entry.file_name());
            let metadata = fs::symlink_metadata(entry.path())
                .with_context(|| format!("cannot read {}", entry.path().display()))?;
            if metadata.is_dir() {
                pending.push(path);
            } else {
                let fingerprint = if metadata.is_file() {
                    file_fingerprint(&entry.path())?
                } else {
                    // A symbolic link or another special file never matches
                    // an archive entry.
                    Fingerprint {
                        size: metadata.len(),
                        crc32: 0,
                    }
                };
                listing.insert(key(&path), fingerprint);
            }
        }
    }
    Ok(listing)
}

/// The size and CRC-32 of the file at `path`.
fn file_fingerprint(path: &Path) -> Result<Fingerprint> {
    let content = fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    Ok(Fingerprint {
        size: u64::try_from(content.len())?,
        crc32: crc32fast::hash(&content),
    })
}

fn key(path: &Path) -> String {
    let parts: Vec<_> = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect();
    parts.join("/")
}

/// Up to five missing, extra, and changed files each, one per line.
fn differences(expected: &Listing, found: &Listing) -> String {
    let missing = expected.keys().filter(|path| !found.contains_key(*path));
    let extra = found.keys().filter(|path| !expected.contains_key(*path));
    let changed = expected
        .iter()
        .filter(|(path, expected)| found.get(*path).is_some_and(|found| found != *expected))
        .map(|(path, _)| path);
    let mut text = String::new();
    for (label, paths) in [
        ("missing", missing.collect::<Vec<_>>()),
        ("not in the archive", extra.collect()),
        ("different content", changed.collect()),
    ] {
        for path in paths.iter().take(5) {
            let _ = write!(text, "\n  {label}: {path}");
        }
        if paths.len() > 5 {
            let _ = write!(text, "\n  {label}: {} more", paths.len() - 5);
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;

    use zip::write::SimpleFileOptions;

    use super::*;

    fn write_archive(path: &Path, files: &[(&str, &str)]) {
        let mut writer = zip::ZipWriter::new(fs::File::create(path).unwrap());
        for (name, contents) in files {
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            writer.start_file(*name, options).unwrap();
            writer.write_all(contents.as_bytes()).unwrap();
        }
        writer.finish().unwrap();
    }

    struct Fixture {
        dir: tempfile::TempDir,
        archive: PathBuf,
        target: PathBuf,
    }

    fn fixture(files: &[(&str, &str)]) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("dataset.zip");
        write_archive(&archive, files);
        let target = dir.path().join("reference").join("realworld");
        fs::create_dir(dir.path().join("reference")).unwrap();
        Fixture {
            dir,
            archive,
            target,
        }
    }

    fn entries(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    const FILES: &[(&str, &str)] = &[
        ("a_b_.github_workflows_ci.yml", "on: push\n"),
        ("c_d_.github_workflows_test.yaml", "on: pull_request\n"),
    ];

    #[test]
    fn missing_corpus_is_unpacked() {
        let fixture = fixture(FILES);

        let outcome = ensure(&fixture.archive, &fixture.target).unwrap();

        assert_eq!(outcome, (Outcome::Created, 2));
        assert_eq!(
            fs::read_to_string(fixture.target.join("a_b_.github_workflows_ci.yml")).unwrap(),
            "on: push\n",
        );
        assert_eq!(
            entries(fixture.target.parent().unwrap()),
            ["realworld"],
            "no temporary directory is left behind",
        );
    }

    #[test]
    fn corpus_that_matches_the_archive_is_up_to_date() {
        let fixture = fixture(FILES);
        ensure(&fixture.archive, &fixture.target).unwrap();

        let outcome = ensure(&fixture.archive, &fixture.target).unwrap();

        assert_eq!(outcome, (Outcome::UpToDate, 2));
    }

    #[test]
    fn nested_entries_are_unpacked_into_directories() {
        let fixture = fixture(&[("dir/", ""), ("dir/inner.yml", "on: push\n")]);

        let outcome = ensure(&fixture.archive, &fixture.target).unwrap();

        assert_eq!(outcome, (Outcome::Created, 1));
        assert!(fixture.target.join("dir").join("inner.yml").is_file());
        assert_eq!(
            ensure(&fixture.archive, &fixture.target).unwrap(),
            (Outcome::UpToDate, 1),
        );
    }

    #[test]
    fn corpus_with_a_missing_file_is_an_error_and_stays() {
        let fixture = fixture(FILES);
        ensure(&fixture.archive, &fixture.target).unwrap();
        fs::remove_file(fixture.target.join("a_b_.github_workflows_ci.yml")).unwrap();

        let error = ensure(&fixture.archive, &fixture.target).unwrap_err();

        assert!(
            format!("{error:#}").contains("a_b_.github_workflows_ci.yml"),
            "{error:#}"
        );
        assert_eq!(
            entries(&fixture.target),
            ["c_d_.github_workflows_test.yaml"]
        );
    }

    #[test]
    fn corpus_with_a_changed_size_is_an_error() {
        let fixture = fixture(FILES);
        ensure(&fixture.archive, &fixture.target).unwrap();
        fs::write(
            fixture.target.join("a_b_.github_workflows_ci.yml"),
            "on: [push]\n",
        )
        .unwrap();

        assert!(ensure(&fixture.archive, &fixture.target).is_err());
    }

    #[test]
    fn corpus_with_changed_content_of_the_same_size_is_an_error_that_names_the_file() {
        let fixture = fixture(FILES);
        ensure(&fixture.archive, &fixture.target).unwrap();
        fs::write(
            fixture.target.join("a_b_.github_workflows_ci.yml"),
            "on: pull\n",
        )
        .unwrap();

        let error = ensure(&fixture.archive, &fixture.target).unwrap_err();

        assert!(
            format!("{error:#}").contains("different content: a_b_.github_workflows_ci.yml"),
            "{error:#}"
        );
    }

    #[test]
    fn corpus_with_an_extra_file_is_an_error() {
        let fixture = fixture(FILES);
        ensure(&fixture.archive, &fixture.target).unwrap();
        fs::write(fixture.target.join("extra.yml"), "on: push\n").unwrap();

        let error = ensure(&fixture.archive, &fixture.target).unwrap_err();

        assert!(format!("{error:#}").contains("extra.yml"), "{error:#}");
    }

    #[test]
    fn entry_that_escapes_the_target_is_rejected() {
        for name in ["../escape.yml", "a/../../escape.yml", "/escape.yml"] {
            let fixture = fixture(&[("ok.yml", "on: push\n"), (name, "on: push\n")]);

            let error = ensure(&fixture.archive, &fixture.target).unwrap_err();

            assert!(format!("{error:#}").contains(name), "{name}: {error:#}");
            assert!(!fixture.target.exists(), "{name}");
            assert_eq!(
                entries(fixture.target.parent().unwrap()),
                Vec::<String>::new()
            );
            assert_eq!(entries(fixture.dir.path()), ["dataset.zip", "reference"]);
        }
    }

    #[test]
    fn leftover_from_an_interrupted_run_is_replaced() {
        let fixture = fixture(FILES);
        let parent = fixture.target.parent().unwrap();
        fs::create_dir(parent.join(".realworld.partial")).unwrap();
        fs::write(parent.join(".realworld.partial").join("half.yml"), "on:").unwrap();

        let outcome = ensure(&fixture.archive, &fixture.target).unwrap();

        assert_eq!(outcome, (Outcome::Created, 2));
        assert_eq!(entries(parent), ["realworld"]);
        assert!(!fixture.target.join("half.yml").exists());
    }
}
