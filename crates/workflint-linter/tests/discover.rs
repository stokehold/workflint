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

#[test]
fn discovery_skips_the_files_that_gitignore_excludes() {
    let dir = project();
    for name in ["ci.yaml", "generated.yaml", "sub/nested.yaml", "kept.yaml"] {
        write(dir.path(), &format!(".github/workflows/{name}"), VALID);
    }
    write(dir.path(), ".gitignore", "generated.yaml\n");
    write(dir.path(), ".github/workflows/.gitignore", "sub/\n");
    write(dir.path(), ".git/info/exclude", "kept.yaml\n");
    let linter = Linter::new(root(&dir));
    let files = linter.lint(&linter.discover().unwrap()).unwrap();
    assert_eq!(names(&files), [native(".github/workflows/ci.yaml")]);
}

#[test]
fn discovery_skips_the_files_that_config_exclude_names() {
    let dir = project();
    write(dir.path(), ".github/workflows/ci.yaml", VALID);
    write(dir.path(), ".github/workflows/ci.lock.yml", VALID);
    write(
        dir.path(),
        ".github/workflint.yaml",
        "exclude: ['.github/workflows/*.lock.yml']\n",
    );
    // The globs match from the project root, also when the run starts in a subdirectory.
    fs::create_dir(dir.path().join("sub")).unwrap();
    let linter = Linter::new(root(&dir).join("sub"));
    let files = linter.lint(&linter.discover().unwrap()).unwrap();
    assert_eq!(names(&files), [native("../.github/workflows/ci.yaml")]);
}

#[test]
fn discovery_that_excludes_every_file_finds_no_yaml_file() {
    let dir = project();
    write(dir.path(), ".github/workflows/ci.yaml", VALID);
    write(dir.path(), ".gitignore", "*.yaml\n");
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
fn a_broken_config_file_stops_discovery() {
    let dir = project();
    write(dir.path(), ".github/workflows/ci.yaml", VALID);
    write(dir.path(), ".github/workflint.yaml", "exclude: ['{']\n");
    let err = Linter::new(root(&dir)).discover().unwrap_err();
    assert!(
        err.to_string()
            .ends_with(r#"workflint.yaml": invalid glob pattern "{" in "exclude""#),
        "{err}"
    );
}

// Arguments.

/// The paths of the inputs of the arguments `args`, with `/` separators.
fn expanded(linter: &Linter, args: &[&str]) -> Vec<String> {
    linter
        .expand(args)
        .unwrap()
        .iter()
        .map(|input| input.path().to_string_lossy().replace('\\', "/"))
        .collect()
}

#[test]
fn a_directory_argument_lists_its_yaml_files_sorted_by_path_from_the_argument() {
    let dir = project();
    for name in [
        "b.yml",
        "a/b.yaml",
        "a.yaml",
        ".hidden/e.yaml",
        "c.YAML",
        "d.txt",
    ] {
        write(dir.path(), &format!(".github/workflows/{name}"), VALID);
    }
    write(dir.path(), ".github/other.yaml", VALID);
    let linter = Linter::new(root(&dir));
    assert_eq!(
        expanded(&linter, &[".github"]),
        [
            ".github/other.yaml",
            ".github/workflows/.hidden/e.yaml",
            ".github/workflows/a.yaml",
            ".github/workflows/a/b.yaml",
            ".github/workflows/b.yml",
        ]
    );
    let files = linter
        .lint(&linter.expand(&[".github/workflows/a"]).unwrap())
        .unwrap();
    assert_eq!(names(&files), [native(".github/workflows/a/b.yaml")]);
}

#[test]
fn the_working_directory_as_an_argument_lists_paths_without_a_dot() {
    let dir = project();
    write(dir.path(), ".github/workflows/ci.yaml", VALID);
    write(dir.path(), "docs/example.yml", VALID);
    // The version control directory is never walked.
    write(dir.path(), ".git/hooks/x.yaml", VALID);
    let linter = Linter::new(root(&dir));
    assert_eq!(
        expanded(&linter, &["."]),
        [".github/workflows/ci.yaml", "docs/example.yml"]
    );
}

#[test]
fn arguments_keep_their_order_and_each_directory_is_sorted() {
    let dir = project();
    for name in ["z/b.yaml", "z/a.yaml", "a/c.yaml", "one.yaml"] {
        write(dir.path(), name, VALID);
    }
    let linter = Linter::new(root(&dir));
    assert_eq!(
        expanded(&linter, &["z", "one.yaml", "a", "z/a.yaml"]),
        ["z/a.yaml", "z/b.yaml", "one.yaml", "a/c.yaml", "z/a.yaml"]
    );
}

#[test]
fn a_directory_argument_skips_files_that_gitignore_or_exclude_name() {
    let dir = project();
    for name in ["ci.yaml", "ci.lock.yml", "generated/x.yaml"] {
        write(dir.path(), &format!(".github/workflows/{name}"), VALID);
    }
    write(dir.path(), ".gitignore", "generated/\n");
    write(
        dir.path(),
        ".github/actionlint.yaml",
        "exclude: ['**/*.lock.yml']\n",
    );
    let linter = Linter::new(root(&dir).join(".github"));
    assert_eq!(expanded(&linter, &["workflows"]), ["workflows/ci.yaml"]);
}

#[test]
fn an_explicit_file_is_linted_when_gitignore_or_exclude_names_it() {
    let dir = project();
    write(dir.path(), ".github/workflows/generated.yaml", VALID);
    write(dir.path(), ".github/workflows/ci.lock.yml", VALID);
    write(dir.path(), ".gitignore", "generated.yaml\n");
    write(
        dir.path(),
        ".github/workflint.yaml",
        "exclude: ['**/*.lock.yml']\n",
    );
    let linter = Linter::new(root(&dir));
    let args = [
        ".github/workflows/generated.yaml",
        ".github/workflows/ci.lock.yml",
    ];
    assert_eq!(expanded(&linter, &args), args);
}

#[test]
fn exclude_of_the_config_file_flag_applies_to_every_project() {
    let dir = project();
    write(dir.path(), ".github/workflows/ci.yaml", VALID);
    write(dir.path(), ".github/workflows/ci.lock.yml", VALID);
    write(dir.path(), "outside/x.lock.yml", VALID);
    write(dir.path(), "outside/y.yml", VALID);
    let config = workflint_linter::config::Config::parse(
        Path::new("c.yaml"),
        workflint_linter::config::ConfigKind::Workflint,
        "exclude: ['**/*.lock.yml']\n",
    )
    .unwrap();
    let linter = Linter::new(root(&dir)).with_config_file(config);
    assert_eq!(
        expanded(&linter, &[".github", "outside"]),
        [".github/workflows/ci.yaml", "outside/y.yml"]
    );
}

#[test]
fn a_glob_argument_lists_the_yaml_files_it_matches_sorted_by_path() {
    let dir = project();
    for name in ["b.yml", "a.yaml", "sub/c.yaml", "d.txt", "e.lock.yml"] {
        write(dir.path(), &format!(".github/workflows/{name}"), VALID);
    }
    write(
        dir.path(),
        ".github/workflint.yaml",
        "exclude: ['**/*.lock.yml']\n",
    );
    let linter = Linter::new(root(&dir));
    assert_eq!(
        expanded(&linter, &[".github/workflows/*"]),
        [".github/workflows/a.yaml", ".github/workflows/b.yml"]
    );
    assert_eq!(
        expanded(&linter, &["**/*.{yaml,txt}"]),
        [
            ".github/workflint.yaml",
            ".github/workflows/a.yaml",
            ".github/workflows/sub/c.yaml",
        ]
    );
    let absolute = format!(
        "{}/.github/*/*.yml",
        root(&dir).to_string_lossy().replace('\\', "/")
    );
    let files = linter.lint(&linter.expand(&[absolute]).unwrap()).unwrap();
    assert_eq!(names(&files), [native(".github/workflows/b.yml")]);
}

#[test]
fn a_glob_argument_skips_files_that_gitignore_names() {
    let dir = project();
    write(dir.path(), "a/x.yaml", VALID);
    write(dir.path(), "b/x.yaml", VALID);
    write(dir.path(), ".gitignore", "/b\n");
    let linter = Linter::new(root(&dir));
    assert_eq!(expanded(&linter, &["*/x.yaml"]), ["a/x.yaml"]);
}

#[test]
fn an_existing_file_whose_name_looks_like_a_glob_is_a_file() {
    let dir = project();
    write(dir.path(), "[ci].yaml", VALID);
    write(dir.path(), "c.yaml", VALID);
    let linter = Linter::new(root(&dir));
    assert_eq!(expanded(&linter, &["[ci].yaml"]), ["[ci].yaml"]);
}

#[test]
fn a_glob_that_matches_no_yaml_file_or_is_invalid_is_an_error() {
    let dir = project();
    write(dir.path(), "a.txt", VALID);
    let linter = Linter::new(root(&dir));
    for (glob, message) in [
        ("*.txt", r#"no YAML file matches the glob "*.txt""#),
        (
            "missing/*.yaml",
            r#"no YAML file matches the glob "missing/*.yaml""#,
        ),
        ("a.{yaml", r#"invalid glob pattern "a.{yaml""#),
    ] {
        assert_eq!(linter.expand(&[glob]).unwrap_err().to_string(), message);
    }
}

#[test]
fn a_directory_argument_without_a_yaml_file_is_an_error() {
    let dir = project();
    write(dir.path(), "docs/README.md", "");
    let linter = Linter::new(root(&dir));
    assert_eq!(
        linter.expand(&["docs"]).unwrap_err().to_string(),
        r#"no YAML file was found in "docs""#
    );
}

#[test]
fn a_missing_file_argument_is_left_for_the_lint_to_report() {
    let dir = project();
    let linter = Linter::new(root(&dir));
    assert_eq!(
        expanded(&linter, &["missing.yaml", "-"]),
        ["missing.yaml", "-"]
    );
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
