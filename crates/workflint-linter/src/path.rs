//! Paths computed as actionlint computes them with Go's `path/filepath`.
//!
//! actionlint prints `filepath.Rel(cwd, path)` when it succeeds and the path as given otherwise
//! (`linter.go:374`). The working directory comes from Go's `os.Getwd`, or is `.` when that
//! fails (`linter.go:170-175`). It finds a file's project from `filepath.Abs` of the path, and
//! joins the project root with a `uses:` value with `filepath.Join`, which cleans the result.

use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

/// Returns the working directory as Go's `os.Getwd` does, or `.` when it is unknown.
///
/// On Unix, Go returns `$PWD` when it is absolute and names the same directory as `.`, so a
/// directory reached through a symbolic link keeps the link's path.
pub fn working_dir() -> PathBuf {
    #[cfg(unix)]
    if let Some(pwd) = std::env::var_os("PWD").map(PathBuf::from)
        && pwd.is_absolute()
        && same_dir(&pwd, Path::new("."))
    {
        return pwd;
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

#[cfg(unix)]
fn same_dir(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

/// Returns the path to print for the argument `path`: `path` relative to `cwd` as Go's
/// `filepath.Rel` computes it, or `path` unchanged when `filepath.Rel` fails.
#[must_use]
pub fn display_path(cwd: &Path, path: &Path) -> PathBuf {
    relative(cwd, path).unwrap_or_else(|| path.to_path_buf())
}

/// Returns `path` as Go's `filepath.Abs` does with the working directory `cwd`: `path` cleaned
/// if it is absolute, and `cwd` joined with `path` and cleaned otherwise.
#[must_use]
pub fn absolute(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        clean(path)
    } else {
        clean(&cwd.join(path))
    }
}

/// Go's `filepath.Clean`: `path` with `.` names and repeated separators removed, and each `..`
/// that follows a name removed with that name. An empty result is `.`.
#[must_use]
pub fn clean(path: &Path) -> PathBuf {
    let clean = Clean::new(path);
    let mut result = PathBuf::new();
    if let Some(prefix) = clean.prefix {
        result.push(prefix);
    }
    if clean.rooted {
        result.push(Component::RootDir);
    }
    result.extend(clean.names);
    if result.as_os_str().is_empty() {
        result.push(".");
    }
    result
}

/// Go's `filepath.Rel`: a path that names `target` when joined to `base`, found lexically.
///
/// Both paths are cleaned first. It fails when one path is absolute and the other is not, when
/// they are on different volumes, and when finding the result would need to know what a `..` in
/// `base` refers to.
#[must_use]
pub fn relative(base: &Path, target: &Path) -> Option<PathBuf> {
    let base = Clean::new(base);
    let target = Clean::new(target);
    if !same_name(base.prefix, target.prefix) || base.rooted != target.rooted {
        return None;
    }
    let common = base
        .names
        .iter()
        .zip(&target.names)
        .take_while(|(a, b)| same_name(Some(a), Some(b)))
        .count();
    let (base_rest, target_rest) = (&base.names[common..], &target.names[common..]);
    if base_rest.iter().any(|name| *name == "..") {
        return None;
    }
    let mut result = PathBuf::new();
    for _ in base_rest {
        result.push("..");
    }
    result.extend(target_rest);
    if result.as_os_str().is_empty() {
        result.push(".");
    }
    Some(result)
}

/// A path cleaned as Go's `filepath.Clean` cleans it: `.` removed and each `..` that follows a
/// name removed with that name.
struct Clean<'a> {
    /// The volume name, such as `C:`. Only Windows has one.
    prefix: Option<&'a OsStr>,
    /// Whether the path starts at a root directory.
    rooted: bool,
    /// The names after the root. Only a relative path can start with `..`.
    names: Vec<&'a OsStr>,
}

impl<'a> Clean<'a> {
    fn new(path: &'a Path) -> Self {
        let mut clean = Clean {
            prefix: None,
            rooted: false,
            names: Vec::new(),
        };
        for component in path.components() {
            match component {
                Component::Prefix(prefix) => clean.prefix = Some(prefix.as_os_str()),
                Component::RootDir => clean.rooted = true,
                Component::CurDir => {}
                Component::ParentDir => match clean.names.last() {
                    Some(last) if *last != ".." => {
                        clean.names.pop();
                    }
                    // `..` at the root is the root.
                    _ if clean.rooted => {}
                    _ => clean.names.push(OsStr::new("..")),
                },
                Component::Normal(name) => clean.names.push(name),
            }
        }
        clean
    }
}

/// Compares two path names or volume names. Windows compares them without case, as Go does.
fn same_name(a: Option<&OsStr>, b: Option<&OsStr>) -> bool {
    if cfg!(windows) {
        let fold = |name: Option<&OsStr>| name.map(|name| name.to_string_lossy().to_lowercase());
        fold(a) == fold(b)
    } else {
        a == b
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn rel(base: &str, target: &str) -> Option<String> {
        relative(Path::new(base), Path::new(target)).map(|path| path.to_string_lossy().into_owned())
    }

    #[test]
    fn a_file_below_the_base_is_printed_without_the_base() {
        assert_eq!(
            rel("/work/repo", "/work/repo/t1.yaml").as_deref(),
            Some("t1.yaml")
        );
    }

    #[test]
    fn a_file_outside_the_base_is_reached_through_parent_directories() {
        assert_eq!(
            rel("/work/repo/sub", "/work/repo/.github/workflows/x.yaml").as_deref(),
            Some("../.github/workflows/x.yaml")
        );
        assert_eq!(rel("/a/b", "/c").as_deref(), Some("../../c"));
    }

    #[test]
    fn the_base_itself_is_a_dot() {
        assert_eq!(rel("/a/b", "/a/b/").as_deref(), Some("."));
    }

    #[test]
    fn both_paths_are_cleaned_first() {
        assert_eq!(
            rel("/a/./b/", "/a//b/c/../d.yaml").as_deref(),
            Some("d.yaml")
        );
        assert_eq!(rel("/a", "/../a/x").as_deref(), Some("x"));
        assert_eq!(rel(".", "./a/../b.yaml").as_deref(), Some("b.yaml"));
    }

    #[test]
    fn names_are_compared_whole() {
        assert_eq!(
            rel("/work/repo", "/work/repo-other/x").as_deref(),
            Some("../repo-other/x")
        );
    }

    #[test]
    fn an_absolute_and_a_relative_path_have_no_relative_path() {
        assert_eq!(rel("/work", "./t1.yaml"), None);
        assert_eq!(rel(".", "/work/t1.yaml"), None);
    }

    #[test]
    fn a_parent_directory_left_in_the_base_has_no_relative_path() {
        assert_eq!(rel("../a", "b"), None);
        assert_eq!(rel("../a", "../a/b").as_deref(), Some("b"));
    }

    #[test]
    fn clean_removes_dots_and_resolves_parent_names() {
        assert_eq!(
            clean(Path::new("/repo/./a//b/../c/")),
            Path::new("/repo/a/c")
        );
        assert_eq!(clean(Path::new("/../a")), Path::new("/a"));
        assert_eq!(clean(Path::new("../a/..")), Path::new(".."));
        assert_eq!(clean(Path::new("a/..")), Path::new("."));
    }

    #[test]
    fn absolute_joins_a_relative_path_to_the_working_directory() {
        assert_eq!(
            absolute(Path::new("/work"), Path::new("./x/../t1.yaml")),
            Path::new("/work/t1.yaml")
        );
        assert_eq!(
            absolute(Path::new("/work"), Path::new("/other/./t1.yaml")),
            Path::new("/other/t1.yaml")
        );
    }

    #[test]
    fn a_relative_argument_is_printed_as_given_from_an_absolute_working_directory() {
        assert_eq!(
            display_path(Path::new("/work"), Path::new("./t1.yaml")),
            Path::new("./t1.yaml")
        );
    }
}
