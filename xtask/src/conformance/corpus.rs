//! The corpus of real-world workflows in the vendored archive
//! `tests/conformance/actionlint/testdata/realworld/dataset.zip`. An
//! allow-list entry names a corpus file as `realworld/<file name>`, or every
//! corpus file as `realworld/*`.

use std::collections::BTreeSet;
use std::fs::File;
use std::path::{Path, PathBuf};

use zip::ZipArchive;

use super::repository_root;

/// The start of the name of an allow-list entry for a corpus file.
pub const PREFIX: &str = "realworld/";

/// The name of an allow-list entry for every corpus file.
pub const EVERY_FILE: &str = "realworld/*";

/// The vendored archive, relative to the repository root.
pub const ARCHIVE: &str = "tests/conformance/actionlint/testdata/realworld/dataset.zip";

/// The vendored archive.
#[must_use]
pub fn archive() -> PathBuf {
    repository_root().join(ARCHIVE)
}

/// The names of the files in `archive`, without directories.
///
/// # Errors
///
/// Returns a message if the archive cannot be read.
pub fn file_names(archive: &Path) -> Result<BTreeSet<String>, String> {
    let error = |error: &dyn std::fmt::Display| format!("{}: {error}", archive.display());
    let file = File::open(archive).map_err(|e| error(&e))?;
    let mut zip = ZipArchive::new(file).map_err(|e| error(&e))?;
    let mut names = BTreeSet::new();
    for index in 0..zip.len() {
        let entry = zip.by_index(index).map_err(|e| error(&e))?;
        if !entry.is_dir() {
            names.insert(entry.name().map_err(|e| error(&e))?.into_owned());
        }
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vendored_archive_holds_the_1503_corpus_files() {
        let names = file_names(&archive()).unwrap();
        assert_eq!(names.len(), 1503);
        assert!(names.contains("Alamofire_Alamofire_.github_workflows_ci.yml"));
    }

    #[test]
    fn a_missing_archive_is_an_error_that_names_it() {
        let error = file_names(Path::new("no/such/dataset.zip")).unwrap_err();
        assert!(error.contains("no/such/dataset.zip"), "{error}");
    }
}
