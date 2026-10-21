//! The project root of a linted file, the reference's `findProject` (`project.go:24`).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::path::absolute;

/// Finds the project roots of the linted files, and remembers the root of each directory it
/// has searched, so that a run reads the file system once per directory instead of once per
/// file and ancestor.
///
/// The reference walks from each file to the file-system root again, unless the file is in the
/// workflows directory of a project it has already found. Its answers are the same as long as
/// the file system does not change during the run.
#[derive(Debug, Default)]
pub struct ProjectRoots {
    /// The project root of each directory searched so far, `None` where it has none.
    known: BTreeMap<PathBuf, Option<PathBuf>>,
}

impl ProjectRoots {
    /// Returns the project root of the file at `path`, relative to `cwd` if it is relative: the
    /// nearest ancestor of `path` that has a `.github/workflows` directory and a `.git` file or
    /// directory. Returns `None` if no ancestor has both.
    ///
    /// The ancestors are those of `path` made absolute and cleaned as Go's `filepath.Abs` does,
    /// so the root is too. Symbolic links are followed when checking for `.github/workflows` and
    /// `.git`, but not resolved in the path. `path` names a file, which has no `.github` inside
    /// it, so the search starts at its directory.
    pub fn find(&mut self, path: &Path, cwd: &Path) -> Option<PathBuf> {
        let path = absolute(cwd, path);
        let mut searched = Vec::new();
        let mut root = None;
        for dir in path.ancestors().skip(1) {
            if let Some(known) = self.known.get(dir) {
                root.clone_from(known);
                break;
            }
            searched.push(dir.to_path_buf());
            if is_project_root(dir) {
                root = Some(dir.to_path_buf());
                break;
            }
        }
        for dir in searched {
            self.known.insert(dir, root.clone());
        }
        root
    }
}

fn is_project_root(dir: &Path) -> bool {
    fs::metadata(dir.join(".github").join("workflows")).is_ok_and(|meta| meta.is_dir())
        && fs::metadata(dir.join(".git")).is_ok()
}

/// Returns the project root of the directory `dir`, an absolute path: the nearest of `dir` and
/// its ancestors that has a `.github/workflows` directory and a `.git` file or directory.
pub(crate) fn root_of_dir(dir: &Path) -> Option<PathBuf> {
    dir.ancestors()
        .find(|dir| is_project_root(dir))
        .map(Path::to_path_buf)
}
