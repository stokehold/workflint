//! The project root of a linted file, the reference's `findProject` (`project.go:24`).

use std::fs;
use std::path::{Path, PathBuf};

use crate::path::absolute;

/// Returns the project root of the file at `path`, relative to `cwd` if it is relative: the
/// nearest ancestor of `path` that has a `.github/workflows` directory and a `.git` file or
/// directory. Returns `None` if no ancestor has both.
///
/// The ancestors are those of `path` made absolute and cleaned as Go's `filepath.Abs` does, so
/// the root is too. Symbolic links are followed when checking for `.github/workflows` and
/// `.git`, but not resolved in the path.
#[must_use]
pub fn find_project_root(path: &Path, cwd: &Path) -> Option<PathBuf> {
    let path = absolute(cwd, path);
    path.ancestors()
        .find(|dir| {
            fs::metadata(dir.join(".github").join("workflows")).is_ok_and(|meta| meta.is_dir())
                && fs::metadata(dir.join(".git")).is_ok()
        })
        .map(Path::to_path_buf)
}
