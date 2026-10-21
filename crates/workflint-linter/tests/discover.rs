//! The inputs of a run without file arguments, which are the workflow files of the project, and
//! of a run that reads a workflow from stdin, on temporary directories.
//!
//! The expected behaviour is the reference's `LintRepository` and `LintDir`
//! (`linter.go:264-311`), and `LintStdin` and `Lint` (`linter.go:474-495`).

#![allow(clippy::unwrap_used)]

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use tempfile::TempDir;
use workflint_diagnostics::quote::quote;
use workflint_linter::{Input, Linted, Linter, path};

const VALID: &str =
    "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo\n";

/// A workflow that uses the local action `./.github/actions/broken`.
const USES_BROKEN_ACTION: &str = "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: ./.github/actions/broken\n";

/// A temporary project: a directory with `.git` and `.github/workflows`.
fn project() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".git")).unwrap();
    fs::create_dir_all(dir.path().join(".github/workflows")).unwrap();
    dir
}

/// The path of `dir`, cleaned as the linter cleans paths.
fn root(dir: &TempDir) -> PathBuf {
    path::clean(dir.path())
}

/// Writes `contents` to `path` under `dir`, creating its directories.
fn write(dir: &Path, path: &str, contents: &str) {
    let path = dir.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

/// The names of the linted files.
fn names(files: &[Linted]) -> Vec<&str> {
    files.iter().map(|linted| linted.file.path()).collect()
}

/// `path` with the separators of the platform, as the linter prints a path that it computes.
fn native(path: &str) -> String {
    path.replace('/', std::path::MAIN_SEPARATOR_STR)
}

/// The messages of the diagnostics of the one linted file.
fn messages(files: &[Linted]) -> Vec<&str> {
    assert_eq!(files.len(), 1);
    files[0]
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect()
}

// Discovery.

#[test]
fn discovery_walks_the_workflows_directory_and_sorts_the_yaml_files_by_path() {
    let dir = project();
    for name in [
        "b.yml",
        "a/b.yaml",
        "a.yaml",
        ".hidden/e.yaml",
        "x.yaml/y.yml",
        "c.YAML",
        "d.txt",
        "a.yaml.bak",
    ] {
        write(dir.path(), &format!(".github/workflows/{name}"), VALID);
    }
    write(dir.path(), ".github/other.yaml", VALID);
    let linter = Linter::new(root(&dir));
    let inputs = linter.discover().unwrap();
    // The paths are sorted as strings, so `a.yaml` comes before `a/b.yaml`.
    let workflows = root(&dir).join(".github").join("workflows");
    let expected: Vec<PathBuf> = [
        ".hidden/e.yaml",
        "a.yaml",
        "a/b.yaml",
        "b.yml",
        "x.yaml/y.yml",
    ]
    .iter()
    .map(|name| workflows.join(name))
    .collect();
    let paths: Vec<PathBuf> = inputs
        .iter()
        .map(|input| input.path().to_path_buf())
        .collect();
    assert_eq!(paths, expected);
    let files = linter.lint(&inputs).unwrap();
    assert_eq!(
        names(&files),
        [
            ".github/workflows/.hidden/e.yaml",
            ".github/workflows/a.yaml",
            ".github/workflows/a/b.yaml",
            ".github/workflows/b.yml",
            ".github/workflows/x.yaml/y.yml",
        ]
        .map(native)
    );
}

#[test]
fn discovery_from_a_subdirectory_finds_the_project_above_it() {
    let dir = project();
    write(dir.path(), ".github/workflows/ci.yaml", VALID);
    fs::create_dir_all(dir.path().join("sub/dir")).unwrap();
    let linter = Linter::new(root(&dir).join("sub").join("dir"));
    let files = linter.lint(&linter.discover().unwrap()).unwrap();
    assert_eq!(names(&files), [native("../../.github/workflows/ci.yaml")]);
}

#[test]
fn discovery_without_a_project_fails() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = root(&dir).join("sub");
    fs::create_dir_all(&cwd).unwrap();
    let err = Linter::new(cwd.clone()).discover().unwrap_err();
    assert_eq!(
        err.to_string(),
        format!(
            "no project was found in any parent directories of {}. check workflows directory is \
             put correctly in your Git repository",
            quote(&cwd.to_string_lossy())
        )
    );
}

#[test]
fn discovery_without_a_yaml_file_fails() {
    let dir = project();
    write(dir.path(), ".github/workflows/README.md", "");
    write(dir.path(), ".github/workflows/sub/ci.YML", VALID);
    let err = Linter::new(root(&dir)).discover().unwrap_err();
    let workflows = root(&dir).join(".github").join("workflows");
    assert_eq!(
        err.to_string(),
        format!(
            "no YAML file was found in {}",
            quote(&workflows.to_string_lossy())
        )
    );
}

#[test]
fn discovery_in_a_project_without_a_workflows_directory_finds_no_yaml_file() {
    // Without a version control marker, the directory with `.github` is the project root.
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".github")).unwrap();
    let err = Linter::new(root(&dir)).discover().unwrap_err();
    let workflows = root(&dir).join(".github").join("workflows");
    assert_eq!(
        err.to_string(),
        format!(
            "no YAML file was found in {}",
            quote(&workflows.to_string_lossy())
        )
    );
}

#[cfg(unix)]
#[test]
fn discovery_fails_on_a_directory_it_cannot_read() {
    use std::os::unix::fs::PermissionsExt;

    let dir = project();
    write(dir.path(), ".github/workflows/a.yaml", VALID);
    let locked = dir.path().join(".github/workflows/locked");
    fs::create_dir(&locked).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    // The superuser reads the directory anyway.
    let readable = fs::read_dir(&locked).is_ok();
    let result = Linter::new(root(&dir)).discover();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    if readable {
        return;
    }
    let workflows = root(&dir).join(".github").join("workflows");
    let message = result.unwrap_err().to_string();
    assert!(
        message.starts_with(&format!(
            "could not read files in {}: {}: ",
            quote(&workflows.to_string_lossy()),
            workflows.join("locked").display()
        )),
        "{message}"
    );
}

#[cfg(unix)]
#[test]
fn discovery_lists_a_symbolic_link_to_a_file_and_does_not_follow_one_to_a_directory() {
    let dir = project();
    write(dir.path(), "elsewhere/linked.yaml", VALID);
    write(dir.path(), "elsewhere/dir/inside.yaml", VALID);
    let workflows = dir.path().join(".github/workflows");
    std::os::unix::fs::symlink(
        dir.path().join("elsewhere/linked.yaml"),
        workflows.join("file.yaml"),
    )
    .unwrap();
    std::os::unix::fs::symlink(dir.path().join("elsewhere/dir"), workflows.join("dir")).unwrap();
    let linter = Linter::new(root(&dir));
    let files = linter.lint(&linter.discover().unwrap()).unwrap();
    assert_eq!(names(&files), [native(".github/workflows/file.yaml")]);
}

// Stdin.

/// A reader that fails.
struct Broken;

impl Read for Broken {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("the pipe broke"))
    }
}

fn stdin(name: &str, text: &str) -> Input {
    Input::stdin(name, text.as_bytes()).unwrap()
}

/// A project with a local action whose metadata does not parse, and a workflow that uses it at
/// `.github/workflows/ci.yaml`.
fn project_with_broken_action() -> TempDir {
    let dir = project();
    write(
        dir.path(),
        ".github/actions/broken/action.yml",
        "outputs: x\n",
    );
    write(dir.path(), ".github/workflows/ci.yaml", USES_BROKEN_ACTION);
    dir
}

#[test]
fn stdin_without_a_name_is_named_stdin_and_has_no_project() {
    let dir = project_with_broken_action();
    let linter = Linter::new(root(&dir));
    for name in ["", "<stdin>"] {
        let files = linter.lint(&[stdin(name, USES_BROKEN_ACTION)]).unwrap();
        assert_eq!(names(&files), ["<stdin>"]);
        assert_eq!(messages(&files), Vec::<&str>::new());
    }
}

#[test]
fn stdin_named_after_a_file_in_a_project_checks_its_local_actions() {
    let dir = project_with_broken_action();
    let linter = Linter::new(root(&dir).join(".github"));
    let files = linter
        .lint(&[stdin("workflows/ci.yaml", USES_BROKEN_ACTION)])
        .unwrap();
    assert_eq!(names(&files), ["workflows/ci.yaml"]);
    let messages = messages(&files);
    assert_eq!(messages.len(), 1);
    assert!(
        messages[0].starts_with("could not parse action metadata"),
        "{messages:?}"
    );
}

#[test]
fn stdin_named_after_a_missing_file_has_no_project() {
    let dir = project_with_broken_action();
    let linter = Linter::new(root(&dir));
    let files = linter
        .lint(&[stdin(".github/workflows/missing.yaml", USES_BROKEN_ACTION)])
        .unwrap();
    assert_eq!(messages(&files), Vec::<&str>::new());
}

#[test]
fn stdin_named_by_an_absolute_path_prints_it_as_given() {
    let dir = project_with_broken_action();
    let name = root(&dir).join(".github/workflows/ci.yaml");
    let name = name.to_str().unwrap();
    let linter = Linter::new(root(&dir));
    let files = linter.lint(&[stdin(name, VALID)]).unwrap();
    assert_eq!(names(&files), [name]);
}

#[test]
fn empty_stdin_is_an_empty_workflow() {
    let dir = tempfile::tempdir().unwrap();
    let files = Linter::new(root(&dir)).lint(&[stdin("", "")]).unwrap();
    assert_eq!(messages(&files), ["workflow is empty"]);
}

#[test]
fn stdin_that_cannot_be_read_is_an_error() {
    let err = Input::stdin("", Broken).unwrap_err();
    assert_eq!(err.to_string(), "could not read stdin: the pipe broke");
}
