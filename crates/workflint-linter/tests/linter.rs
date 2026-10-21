//! The parallel driver, on temporary directories: the order of the results, where the errors of
//! shared local actions and reusable workflows are reported, and what a panic in one file does.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::Path;

use tempfile::TempDir;
use workflint_diagnostics::{Color, Diagnostic, SourceFile, render_oneline};
use workflint_linter::rules::INTERNAL_ERROR;
use workflint_linter::{IgnorePattern, Input, Linted, Linter};
use workflint_rules::local::LocalMetadata;

const VALID: &str =
    "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo\n";

/// A workflow with one diagnostic: the job has no `steps`.
const NO_STEPS: &str = "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n";

/// A workflow that uses the local action `./.github/actions/broken`.
const USES_BROKEN_ACTION: &str = "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: ./.github/actions/broken\n";

/// A workflow that calls the local reusable workflow `./.github/workflows/broken-callee.yml`.
const CALLS_BROKEN_WORKFLOW: &str =
    "on: push\njobs:\n  call:\n    uses: ./.github/workflows/broken-callee.yml\n";

/// A temporary project: a directory with `.git` and `.github/workflows`.
fn project() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".git")).unwrap();
    fs::create_dir_all(dir.path().join(".github/workflows")).unwrap();
    dir
}

/// Writes `contents` to `path` under `dir`, creating its directories.
fn write(dir: &Path, path: &str, contents: &str) {
    let path = dir.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn inputs(paths: &[String]) -> Vec<Input> {
    paths.iter().map(Input::file).collect()
}

/// The one-line output of each file, in the order of the results.
fn oneline(files: &[Linted]) -> Vec<String> {
    files
        .iter()
        .map(|linted| {
            let mut out = Vec::new();
            render_oneline(&mut out, &linted.file, &linted.diagnostics, Color::Off).unwrap();
            String::from_utf8(out).unwrap()
        })
        .collect()
}

/// Lints `paths` in `dir` with `linter` on a pool of `threads` threads.
fn lint_with_threads(linter: &Linter, paths: &[String], threads: usize) -> Vec<String> {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .unwrap();
    oneline(&pool.install(|| linter.lint(&inputs(paths))).unwrap())
}

/// The workflows `w00.yaml` to `w15.yaml` in `.github/workflows/`, in reverse order.
fn sixteen_workflows_reversed() -> Vec<String> {
    (0..16)
        .rev()
        .map(|index| format!(".github/workflows/w{index:02}.yaml"))
        .collect()
}

#[test]
fn files_come_back_in_input_order_named_by_their_path_from_the_working_directory() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "b.yaml", NO_STEPS);
    write(dir.path(), "a.yaml", VALID);
    write(dir.path(), "sub/c.yaml", NO_STEPS);
    let paths = ["sub/c.yaml", "b.yaml", "a.yaml"].map(str::to_owned);
    let linter = Linter::new(dir.path().to_path_buf());
    let files = linter.lint(&inputs(&paths)).unwrap();
    let names: Vec<&str> = files.iter().map(|linted| linted.file.path()).collect();
    // A relative path is printed as given, with `/` also on Windows.
    assert_eq!(names, ["sub/c.yaml", "b.yaml", "a.yaml"]);
    let counts: Vec<usize> = files
        .iter()
        .map(|linted| linted.diagnostics.len())
        .collect();
    assert_eq!(counts, [1, 1, 0]);
}

#[test]
fn a_file_given_twice_is_linted_twice() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.yaml", NO_STEPS);
    let paths = ["a.yaml", "a.yaml"].map(str::to_owned);
    let output = lint_with_threads(&Linter::new(dir.path().to_path_buf()), &paths, 2);
    assert_eq!(output.len(), 2);
    assert_eq!(output[0], output[1]);
}

#[test]
fn a_broken_local_action_is_reported_in_the_first_file_by_path_whatever_the_order_and_threads() {
    let dir = project();
    write(
        dir.path(),
        ".github/actions/broken/action.yml",
        "outputs: x\n",
    );
    for index in 0..16 {
        write(
            dir.path(),
            &format!(".github/workflows/w{index:02}.yaml"),
            USES_BROKEN_ACTION,
        );
    }
    let linter = Linter::new(dir.path().to_path_buf());
    let paths = sixteen_workflows_reversed();
    let one = lint_with_threads(&linter, &paths, 1);
    let reporting: Vec<&str> = paths
        .iter()
        .zip(&one)
        .filter(|(_, output)| output.contains("could not parse action metadata"))
        .map(|(path, _)| path.as_str())
        .collect();
    assert_eq!(reporting, [".github/workflows/w00.yaml"], "{one:#?}");
    for threads in [2, 8] {
        assert_eq!(lint_with_threads(&linter, &paths, threads), one);
    }
}

#[test]
fn a_broken_reusable_workflow_is_reported_in_the_first_caller_by_path() {
    let dir = project();
    write(
        dir.path(),
        ".github/workflows/broken-callee.yml",
        "on:\n  workflow_call:\n    outputs: my cool outputs\n",
    );
    for name in ["c", "a", "b"] {
        write(
            dir.path(),
            &format!(".github/workflows/{name}.yaml"),
            CALLS_BROKEN_WORKFLOW,
        );
    }
    let paths = ["c", "a", "b"].map(|name| format!(".github/workflows/{name}.yaml"));
    let linter = Linter::new(dir.path().to_path_buf());
    let one = lint_with_threads(&linter, &paths, 1);
    let reporting: Vec<bool> = one
        .iter()
        .map(|output| output.contains("error while parsing reusable workflow"))
        .collect();
    assert_eq!(reporting, [false, true, false], "{one:#?}");
    assert_eq!(lint_with_threads(&linter, &paths, 4), one);
}

/// The rules, except that the lint of a file named `panics.yaml` panics.
fn check_or_panic(file: &SourceFile, local: &dyn LocalMetadata) -> Vec<Diagnostic> {
    assert!(
        !file.path().ends_with("panics.yaml"),
        "a rule failed on {}",
        file.path()
    );
    workflint_rules::check_file(file, local)
}

#[test]
fn a_panic_in_one_file_is_an_internal_error_on_that_file_and_the_others_are_linted() {
    let dir = project();
    write(
        dir.path(),
        ".github/actions/broken/action.yml",
        "outputs: x\n",
    );
    for name in ["a", "panics", "z"] {
        write(
            dir.path(),
            &format!(".github/workflows/{name}.yaml"),
            USES_BROKEN_ACTION,
        );
    }
    write(dir.path(), "outside.yaml", NO_STEPS);
    let paths = [
        ".github/workflows/z.yaml",
        ".github/workflows/panics.yaml",
        "outside.yaml",
        ".github/workflows/a.yaml",
    ]
    .map(str::to_owned);
    let cwd = dir.path().to_path_buf();
    let expected = lint_with_threads(&Linter::new(cwd.clone()), &paths, 1);
    let panicking = Linter::new(cwd).with_check(check_or_panic);
    for threads in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        let files = pool.install(|| panicking.lint(&inputs(&paths))).unwrap();
        let output = oneline(&files);
        for index in [0, 2, 3] {
            assert_eq!(output[index], expected[index], "{}", paths[index]);
            assert!(!files[index].has_internal_error());
        }
        let panicked = &files[1];
        assert!(panicked.has_internal_error());
        assert_eq!(panicked.diagnostics.len(), 1, "{output:#?}");
        let diagnostic = &panicked.diagnostics[0];
        assert_eq!(diagnostic.rule.id, INTERNAL_ERROR.id);
        let panics = ".github/workflows/panics.yaml";
        assert_eq!(
            output[1],
            format!(
                "{panics}:1:1: workflint panicked while linting this file: a rule failed on \
                 {panics}. This is a bug in workflint. Please report it at \
                 https://github.com/stokehold/workflint/issues [internal-error]\n"
            )
        );
    }
}

#[test]
fn the_first_unreadable_file_in_input_order_is_the_error() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.yaml", VALID);
    let paths = ["a.yaml", "missing-2.yaml", "missing-1.yaml"].map(str::to_owned);
    let linter = Linter::new(dir.path().to_path_buf());
    for threads in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        let err = pool.install(|| linter.lint(&inputs(&paths))).unwrap_err();
        assert!(
            err.to_string()
                .starts_with("could not read \"missing-2.yaml\": "),
            "{err}"
        );
    }
}

#[test]
fn bytes_that_do_not_decode_are_a_yaml_syntax_error_at_the_end_of_the_decoded_text() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("bad.yaml"), b"on: push\xFF\n").unwrap();
    let paths = ["bad.yaml".to_owned()];
    let output = lint_with_threads(&Linter::new(dir.path().to_path_buf()), &paths, 1);
    assert_eq!(
        output,
        [
            "bad.yaml:1:9: could not parse as YAML: invalid leading UTF-8 octet [yaml-syntax-error]\n"
        ]
    );
}

/// A workflow with two diagnostics of the `unused-anchor` rule.
const TWO_ANCHORS: &str = "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: &a echo\n      - run: &b echo\n";

#[test]
fn an_ignore_pattern_drops_the_diagnostics_whose_message_it_matches() {
    let dir = project();
    write(dir.path(), "a.yaml", TWO_ANCHORS);
    let paths = ["a.yaml".to_owned()];
    let linter = Linter::new(dir.path().to_path_buf());
    let all = oneline(&linter.lint(&inputs(&paths)).unwrap());
    assert_eq!(all[0].lines().count(), 2, "{}", all[0]);

    let ignore = vec![IgnorePattern::new("anchor \"a\"").unwrap()];
    let filtered = oneline(&linter.with_ignore(ignore).lint(&inputs(&paths)).unwrap());
    assert_eq!(
        filtered,
        [all[0]
            .lines()
            .filter(|line| !line.contains("anchor \"a\""))
            .flat_map(|line| [line, "\n"])
            .collect::<String>()]
    );
}

#[test]
fn an_ignore_pattern_that_matches_every_message_leaves_no_diagnostic() {
    let dir = project();
    write(dir.path(), "a.yaml", TWO_ANCHORS);
    write(dir.path(), "b.yaml", NO_STEPS);
    let paths = ["a.yaml".to_owned(), "b.yaml".to_owned()];
    let linter =
        Linter::new(dir.path().to_path_buf()).with_ignore(vec![IgnorePattern::new("").unwrap()]);
    let files = linter.lint(&inputs(&paths)).unwrap();
    assert!(files.iter().all(|linted| linted.diagnostics.is_empty()));
}

#[test]
fn an_ignore_pattern_never_drops_an_internal_error() {
    let dir = project();
    write(dir.path(), "panics.yaml", VALID);
    let paths = ["panics.yaml".to_owned()];
    let linter = Linter::new(dir.path().to_path_buf())
        .with_check(check_or_panic)
        .with_ignore(vec![IgnorePattern::new("panicked").unwrap()]);
    let files = linter.lint(&inputs(&paths)).unwrap();
    assert!(files[0].has_internal_error());
}
