//! The workflow files that a run without file arguments lints, the reference's
//! `LintRepository` and `LintDir` (`linter.go:264-311`).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use workflint_diagnostics::quote::quote;

use crate::project::ProjectRoots;

/// The workflow files of a run without file arguments could not be found. As in the reference,
/// it stops the run.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    /// The working directory has no project.
    #[error(
        "no project was found in any parent directories of {}. check workflows directory is put \
         correctly in your Git repository",
        quote(&.dir.to_string_lossy())
    )]
    NoProject {
        /// The working directory.
        dir: PathBuf,
    },
    /// A file or directory under the workflows directory could not be read.
    #[error(
        "could not read files in {}: {}: {source}",
        quote(&.dir.to_string_lossy()),
        .path.display()
    )]
    Walk {
        /// The workflows directory.
        dir: PathBuf,
        /// The file or directory that could not be read.
        path: PathBuf,
        /// The error of reading it.
        source: io::Error,
    },
    /// The workflows directory has no file whose name ends in `.yml` or `.yaml`.
    #[error("no YAML file was found in {}", quote(&.dir.to_string_lossy()))]
    NoFiles {
        /// The workflows directory.
        dir: PathBuf,
    },
}

/// Returns the paths of the files under `.github/workflows` of the project of `cwd`, an
/// absolute path, whose names end in `.yml` or `.yaml`, sorted by path.
///
/// As the reference's `filepath.Walk` does, the walk descends into subdirectories and does not
/// follow symbolic links, not even when `.github/workflows` itself is one. A symbolic link with
/// a matching name is listed like a file. The paths are sorted as strings, so `a.yaml` comes
/// before `a/b.yaml`. A missing `.github/workflows` has no files.
pub(crate) fn workflow_files(cwd: &Path) -> Result<Vec<PathBuf>, DiscoveryError> {
    tracing::info!(
        "Linting all workflow files in repository: {}",
        cwd.display()
    );
    let Some(root) = ProjectRoots::default().find_dir(cwd, cwd) else {
        return Err(DiscoveryError::NoProject {
            dir: cwd.to_path_buf(),
        });
    };
    tracing::info!("Detected project: {}", root.display());
    let dir = root.join(".github").join("workflows");
    let mut files = Vec::new();
    let walked = match fs::symlink_metadata(&dir) {
        Ok(meta) => walk(&dir, meta.is_dir(), &mut files),
        // A root found by the `.github` fallback may have no workflows directory, which the
        // reference never sees.
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err((dir.clone(), err)),
    };
    walked.map_err(|(path, source)| DiscoveryError::Walk {
        dir: dir.clone(),
        path,
        source,
    })?;
    if files.is_empty() {
        return Err(DiscoveryError::NoFiles { dir });
    }
    tracing::info!("Collected {} YAML files", files.len());
    files.sort_by(|a, b| {
        a.as_os_str()
            .as_encoded_bytes()
            .cmp(b.as_os_str().as_encoded_bytes())
    });
    Ok(files)
}

/// Adds `path` to `files` if it is not a directory and its name ends in `.yml` or `.yaml`, the
/// reference's case-sensitive test, and walks it in the order of its names if it is a directory.
///
/// Fails with the path that could not be read.
fn walk(path: &Path, is_dir: bool, files: &mut Vec<PathBuf>) -> Result<(), (PathBuf, io::Error)> {
    if !is_dir {
        let name = path.as_os_str().as_encoded_bytes();
        if name.ends_with(b".yml") || name.ends_with(b".yaml") {
            files.push(path.to_path_buf());
        }
        return Ok(());
    }
    let fail = |err| (path.to_path_buf(), err);
    let mut entries = fs::read_dir(path)
        .map_err(fail)?
        .collect::<io::Result<Vec<_>>>()
        .map_err(fail)?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        match entry.file_type() {
            Ok(file_type) => walk(&path, file_type.is_dir(), files)?,
            Err(err) => return Err((path, err)),
        }
    }
    Ok(())
}
