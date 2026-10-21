//! The project root of a linted file, the reference's `findProject` (`project.go:24`), with
//! more markers and a fallback (ADR 0010).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::path::absolute;

/// The names of the version control directories that mark a project root. `.git` may also be
/// a file, as in a Git worktree or submodule.
const MARKERS: [&str; 3] = [".git", ".jj", ".hg"];

/// Finds the project roots of the linted files, and remembers the answer for each directory it
/// has searched, so that a run reads the file system once per directory instead of once per
/// file and ancestor.
///
/// The reference walks from each file to the file-system root again, unless the file is in the
/// workflows directory of a project it has already found. Its answers are the same as long as
/// the file system does not change during the run.
#[derive(Debug, Default)]
pub struct ProjectRoots {
    /// The answer of each directory searched so far.
    known: BTreeMap<PathBuf, Known>,
}

/// What a directory and its ancestors hold.
#[derive(Debug, Clone)]
enum Known {
    /// The nearest of them with `.github/workflows` and a marker.
    Marked(PathBuf),
    /// None of them has both, and this is the nearest with `.github`, if any has one.
    Unmarked(Option<PathBuf>),
}

impl Known {
    fn root(&self) -> Option<PathBuf> {
        match self {
            Self::Marked(root) => Some(root.clone()),
            Self::Unmarked(github) => github.clone(),
        }
    }
}

impl ProjectRoots {
    /// Returns the project root of the file at `path`, relative to `cwd` if it is relative.
    ///
    /// The root is the nearest ancestor of `path` that has a `.github/workflows` directory and a
    /// `.git`, `.jj`, or `.hg` marker. If no ancestor has both, it is the nearest ancestor with a
    /// `.github` directory. Without that either, the file has no project.
    ///
    /// The ancestors are those of `path` made absolute and cleaned as Go's `filepath.Abs` does,
    /// so the root is too. Symbolic links are followed when checking for directories and
    /// markers, but not resolved in the path. `path` names a file, which has no `.github` inside
    /// it, so the search starts at its directory.
    pub fn find(&mut self, path: &Path, cwd: &Path) -> Option<PathBuf> {
        let path = absolute(cwd, path);
        path.parent().and_then(|dir| self.search(dir))
    }

    /// Returns the project root of the directory `dir`, relative to `cwd` if it is relative, as
    /// [`find`](Self::find) does for a file, except that `dir` itself may be the root.
    pub fn find_dir(&mut self, dir: &Path, cwd: &Path) -> Option<PathBuf> {
        self.search(&absolute(cwd, dir))
    }

    /// Returns the project root of `start`, an absolute and clean directory.
    fn search(&mut self, start: &Path) -> Option<PathBuf> {
        let mut searched = Vec::new();
        let mut above = Known::Unmarked(None);
        for dir in start.ancestors() {
            if let Some(known) = self.known.get(dir) {
                above = known.clone();
                break;
            }
            if is_marked_root(dir) {
                above = Known::Marked(dir.to_path_buf());
                self.known.insert(dir.to_path_buf(), above.clone());
                break;
            }
            searched.push(dir);
        }
        // From the top down, a directory's answer is that of its parent, unless it has
        // `.github` and nothing above it is marked.
        for dir in searched.into_iter().rev() {
            if matches!(above, Known::Unmarked(_)) && is_dir(&dir.join(".github")) {
                above = Known::Unmarked(Some(dir.to_path_buf()));
            }
            self.known.insert(dir.to_path_buf(), above.clone());
        }
        above.root()
    }
}

fn is_marked_root(dir: &Path) -> bool {
    is_dir(&dir.join(".github").join("workflows"))
        && MARKERS
            .iter()
            .any(|marker| fs::metadata(dir.join(marker)).is_ok())
}

fn is_dir(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|meta| meta.is_dir())
}
