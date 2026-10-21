//! The workflow files of a run: those of the project for a run without file arguments, the
//! reference's `LintRepository` and `LintDir` (`linter.go:264-311`), and those that directory
//! and glob arguments name (ADR 0036).
//!
//! Both skip the files that `.gitignore` or the `exclude` key of the config file names. A file
//! given by name is always linted.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use workflint_diagnostics::quote::quote;

use crate::config::{Config, ConfigError};
use crate::doublestar;
use crate::path;
use crate::project::{MARKERS, ProjectRoots};

/// The workflow files of the run could not be found. As in the reference, it stops the run.
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
    /// A file or directory under the walked directory could not be read.
    #[error(
        "could not read files in {}: {}: {source}",
        quote(&.dir.to_string_lossy()),
        .path.display()
    )]
    Walk {
        /// The walked directory: the workflows directory, or a directory argument as given.
        dir: PathBuf,
        /// The file or directory that could not be read.
        path: PathBuf,
        /// The error of reading it.
        source: io::Error,
    },
    /// The walked directory has no file to lint whose name ends in `.yml` or `.yaml`.
    #[error("no YAML file was found in {}", quote(&.dir.to_string_lossy()))]
    NoFiles {
        /// The workflows directory, or a directory argument as given.
        dir: PathBuf,
    },
    /// A glob argument matches no file to lint whose name ends in `.yml` or `.yaml`.
    #[error("no YAML file matches the glob {}", quote(.glob))]
    NoMatch {
        /// The argument.
        glob: String,
    },
    /// A glob argument is malformed.
    #[error("invalid glob pattern {}", quote(.glob))]
    InvalidGlob {
        /// The argument.
        glob: String,
    },
    /// The config file of a project, which `exclude` comes from, could not be read.
    #[error(transparent)]
    Config(#[from] ConfigError),
}

/// Finds the files of a run, and reads the config file of each project it meets once, for its
/// `exclude` globs.
pub(crate) struct Finder<'f> {
    cwd: &'f Path,
    /// The config file of `-config-file`, which takes the place of every project's.
    config_file: Option<&'f Config>,
    roots: ProjectRoots,
    /// The config file of each project root met so far.
    configs: BTreeMap<PathBuf, Option<Config>>,
}

impl<'f> Finder<'f> {
    /// A finder that resolves relative paths from `cwd`, an absolute path.
    pub(crate) fn new(cwd: &'f Path, config_file: Option<&'f Config>) -> Self {
        Self {
            cwd,
            config_file,
            roots: ProjectRoots::default(),
            configs: BTreeMap::new(),
        }
    }

    /// Returns the absolute paths of the files under `.github/workflows` of the project of the
    /// working directory whose names end in `.yml` or `.yaml`, sorted by path, without those
    /// that `.gitignore` or `exclude` names.
    ///
    /// As the reference's `filepath.Walk` does, the walk descends into subdirectories and does
    /// not follow symbolic links, not even when `.github/workflows` itself is one. A symbolic
    /// link with a matching name is listed like a file. The paths are sorted as strings, so
    /// `a.yaml` comes before `a/b.yaml`. A missing `.github/workflows` has no files.
    pub(crate) fn workflow_files(&mut self) -> Result<Vec<PathBuf>, DiscoveryError> {
        tracing::info!(
            "Linting all workflow files in repository: {}",
            self.cwd.display()
        );
        let Some(root) = self.roots.find_dir(self.cwd, self.cwd) else {
            return Err(DiscoveryError::NoProject {
                dir: self.cwd.to_path_buf(),
            });
        };
        tracing::info!("Detected project: {}", root.display());
        let dir = root.join(".github").join("workflows");
        let found = match fs::symlink_metadata(&dir) {
            Ok(meta) if meta.is_dir() => walk(&dir, None),
            // A symbolic link is not followed, and a root found by the `.github` fallback may
            // have no workflows directory, which the reference never sees.
            Ok(_) => Ok(Vec::new()),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(err) => Err((dir.clone(), err)),
        };
        let found = found.map_err(|(path, source)| DiscoveryError::Walk {
            dir: dir.clone(),
            path,
            source,
        })?;
        let mut files = self.keep(found)?;
        if files.is_empty() {
            return Err(DiscoveryError::NoFiles { dir });
        }
        tracing::info!("Collected {} YAML files", files.len());
        sort(&mut files);
        Ok(files)
    }

    /// Returns the files that the argument `arg` names: the YAML files under it, sorted by
    /// path, if it is a directory, the YAML files that it matches, sorted by path, if it is a
    /// glob, and else `arg` itself. Files under a directory and files that a glob matches skip
    /// those that `.gitignore` or `exclude` names.
    ///
    /// An argument is a glob if it does not exist and has `*`, `?`, `[`, or `{`. The paths are
    /// relative to the working directory if `arg` is, and start with `arg` or the part of the
    /// glob before its first wildcard.
    pub(crate) fn expand(&mut self, arg: &OsStr) -> Result<Vec<PathBuf>, DiscoveryError> {
        let given = Path::new(arg);
        let absolute = path::absolute(self.cwd, given);
        match fs::metadata(&absolute) {
            Ok(meta) if meta.is_dir() => return self.directory(given, &absolute),
            Ok(_) => return Ok(vec![given.to_path_buf()]),
            Err(_) => {}
        }
        match arg.to_str() {
            Some(glob)
                if fs::symlink_metadata(&absolute).is_err()
                    && glob.contains(['*', '?', '[', '{']) =>
            {
                self.glob(glob)
            }
            // A file that does not exist is left for the lint to report.
            _ => Ok(vec![given.to_path_buf()]),
        }
    }

    /// The files under the directory argument `given`, whose absolute path is `absolute`.
    fn directory(&mut self, given: &Path, absolute: &Path) -> Result<Vec<PathBuf>, DiscoveryError> {
        let found = walk(absolute, None).map_err(|(path, source)| DiscoveryError::Walk {
            dir: given.to_path_buf(),
            path,
            source,
        })?;
        let mut files = self.keep(found)?;
        if files.is_empty() {
            return Err(DiscoveryError::NoFiles {
                dir: given.to_path_buf(),
            });
        }
        // `.` adds nothing to the paths, so they are printed as discovery prints them.
        let is_cwd = path::clean(given) == Path::new(".");
        for file in &mut files {
            if let Ok(relative) = file.strip_prefix(absolute) {
                *file = if is_cwd {
                    relative.to_path_buf()
                } else {
                    given.join(relative)
                };
            }
        }
        sort(&mut files);
        Ok(files)
    }

    /// The files that the glob argument `glob` matches.
    fn glob(&mut self, glob: &str) -> Result<Vec<PathBuf>, DiscoveryError> {
        // A Windows path separates directories with `\`, which is no escape there.
        #[cfg(windows)]
        let pattern = glob.replace('\\', "/");
        #[cfg(not(windows))]
        let pattern = glob.to_owned();
        if !doublestar::validate(&pattern) {
            return Err(DiscoveryError::InvalidGlob {
                glob: glob.to_owned(),
            });
        }
        let (base, rest) = split_glob(&pattern);
        let absolute = path::absolute(
            self.cwd,
            if base.is_empty() {
                Path::new(".")
            } else {
                Path::new(base)
            },
        );
        // Without `**`, a match is never deeper than the segments of the pattern.
        let depth = (!rest.contains("**")).then(|| rest.split('/').count());
        let found = if fs::metadata(&absolute).is_ok_and(|meta| meta.is_dir()) {
            walk(&absolute, depth).map_err(|(path, source)| DiscoveryError::Walk {
                dir: PathBuf::from(if base.is_empty() { "." } else { base }),
                path,
                source,
            })?
        } else {
            Vec::new()
        };
        let matched = found
            .into_iter()
            .filter(|file| doublestar::is_match(rest, &path::slash_path(&absolute, file)))
            .collect();
        let mut files = self.keep(matched)?;
        if files.is_empty() {
            return Err(DiscoveryError::NoMatch {
                glob: glob.to_owned(),
            });
        }
        for file in &mut files {
            if let Ok(relative) = file.strip_prefix(&absolute) {
                *file = Path::new(base).join(relative);
            }
        }
        sort(&mut files);
        Ok(files)
    }

    /// Drops the files, absolute paths, that an `exclude` glob of their config file matches.
    fn keep(&mut self, files: Vec<PathBuf>) -> Result<Vec<PathBuf>, ConfigError> {
        let mut kept = Vec::with_capacity(files.len());
        for file in files {
            if self.is_excluded(&file)? {
                tracing::debug!("Skipped {} because exclude names it", file.display());
            } else {
                kept.push(file);
            }
        }
        Ok(kept)
    }

    /// Whether an `exclude` glob of the config file of the file at `file`, an absolute path,
    /// matches its path from its project root, or from the working directory for a file in no
    /// project.
    fn is_excluded(&mut self, file: &Path) -> Result<bool, ConfigError> {
        let root = self.roots.find(file, self.cwd);
        let config = match (self.config_file, &root) {
            (Some(config), _) => Some(config),
            (None, Some(root)) => {
                if !self.configs.contains_key(root) {
                    self.configs.insert(root.clone(), Config::find(root)?);
                }
                self.configs.get(root).and_then(Option::as_ref)
            }
            (None, None) => None,
        };
        Ok(config.is_some_and(|config| {
            config.is_excluded(&path::slash_path(root.as_deref().unwrap_or(self.cwd), file))
        }))
    }
}

/// Splits `glob`, with `/` separators, into the directory before its first segment with a
/// wildcard or an escape, and the rest. The directory of `/*.yml` is `/`.
fn split_glob(glob: &str) -> (&str, &str) {
    let mut end = 0;
    for (start, segment) in segments(glob) {
        if segment.contains(['*', '?', '[', '{', '\\']) {
            let base = &glob[..end];
            let base = if base.is_empty() && glob.starts_with('/') {
                "/"
            } else {
                base
            };
            return (base, &glob[start..]);
        }
        end = start + segment.len();
    }
    (glob, "")
}

/// The segments of `glob` between `/`, with the byte offset where each starts.
fn segments(glob: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut start = 0;
    glob.split('/').map(move |segment| {
        let at = start;
        start += segment.len() + 1;
        (at, segment)
    })
}

/// Sorts `files` by their paths as strings.
fn sort(files: &mut [PathBuf]) {
    files.sort_by(|a, b| {
        a.as_os_str()
            .as_encoded_bytes()
            .cmp(b.as_os_str().as_encoded_bytes())
    });
}

/// Returns the paths of the files under `dir`, an absolute path to a directory, whose names end
/// in `.yml` or `.yaml`, the reference's case-sensitive test, down to `depth` levels if it is
/// given, without those that `.gitignore` names.
///
/// The `.gitignore` files of `dir` and the directories above it apply, and so does
/// `.git/info/exclude`, as in a Git repository or a Jujutsu one. The user's global excludes
/// file does not, so that every machine finds the same files. Symbolic links below `dir` are
/// not followed, and version control directories are not walked.
///
/// Fails with the path that could not be read.
fn walk(dir: &Path, depth: Option<usize>) -> Result<Vec<PathBuf>, (PathBuf, io::Error)> {
    let mut builder = ignore::WalkBuilder::new(dir);
    builder
        .standard_filters(false)
        .git_ignore(true)
        .git_exclude(true)
        .parents(true)
        .follow_links(false)
        .max_depth(depth)
        .filter_entry(|entry| {
            !(entry.file_type().is_some_and(|kind| kind.is_dir())
                && MARKERS.iter().any(|marker| entry.file_name() == *marker))
        });
    let mut files = Vec::new();
    for entry in builder.build() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                let path = error_path(&err).unwrap_or(dir).to_path_buf();
                match err.into_io_error() {
                    Some(source) => return Err((path, source)),
                    // A malformed line of a `.gitignore` file, which Git skips too.
                    None => continue,
                }
            }
        };
        if let Some(err) = entry.error() {
            tracing::debug!("Skipped a line of an ignore file: {err}");
        }
        if entry.file_type().is_none_or(|kind| kind.is_dir()) {
            continue;
        }
        let name = entry.path().as_os_str().as_encoded_bytes();
        if name.ends_with(b".yml") || name.ends_with(b".yaml") {
            files.push(entry.into_path());
        }
    }
    Ok(files)
}

/// The path that `err` names, if it names one.
fn error_path(err: &ignore::Error) -> Option<&Path> {
    match err {
        ignore::Error::WithPath { path, .. } => Some(path),
        ignore::Error::WithDepth { err, .. } | ignore::Error::WithLineNumber { err, .. } => {
            error_path(err)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_glob_splits_before_its_first_segment_with_a_wildcard() {
        assert_eq!(split_glob("*.yml"), ("", "*.yml"));
        assert_eq!(
            split_glob(".github/workflows/*.yml"),
            (".github/workflows", "*.yml")
        );
        assert_eq!(split_glob("a/**/b/*.yml"), ("a", "**/b/*.yml"));
        assert_eq!(split_glob("/*.yml"), ("/", "*.yml"));
        assert_eq!(split_glob("/repo/{a,b}/c.yml"), ("/repo", "{a,b}/c.yml"));
        assert_eq!(split_glob(r"a/\*.yml"), ("a", r"\*.yml"));
    }
}
