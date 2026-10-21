//! Runs the `workflint` binary on the files in `tests/cli/` with the command line the
//! conformance runner uses, and checks its output and exit code.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// What one run of the binary printed and returned.
#[derive(Debug, PartialEq, Eq)]
struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// The directory with the test files, `tests/cli/`.
fn cli_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("cli")
}

/// A command that runs the binary with `args` in `dir`.
fn command<S: AsRef<std::ffi::OsStr>>(dir: &Path, args: &[S]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_workflint"));
    command.args(args).current_dir(dir);
    command
}

fn run(output: Output) -> Run {
    Run {
        code: output.status.code(),
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
    }
}

/// Runs the binary with `args` in `tests/cli/`.
fn workflint(args: &[&str]) -> Run {
    run(command(&cli_dir(), args).output().unwrap())
}

/// The runner's flags, with external tools off and an unused config file.
const RUNNER: [&str; 6] = [
    "-oneline",
    "-no-color",
    "-shellcheck=",
    "-pyflakes=",
    "-config-file",
    "config.yaml",
];

fn runner(files: &[&str]) -> Run {
    let args: Vec<&str> = RUNNER.iter().chain(files).copied().collect();
    workflint(&args)
}

const BROKEN: &str = "broken.yaml:5:12: could not parse as YAML: block sequence entries are not allowed in this context [yaml-syntax-error]\n";

const ANCHORS: &str = "\
anchors.yaml:3:6: anchor \"b\" is defined but not used [unused-anchor]
anchors.yaml:4:6: anchor \"a\" is defined but not used [unused-anchor]
";

#[test]
fn valid_file_prints_nothing_and_exits_with_0() {
    let run = runner(&["valid.yaml"]);
    assert_eq!(
        run,
        Run {
            code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
        }
    );
}

#[test]
fn yaml_syntax_error_prints_one_line_and_exits_with_1() {
    let run = runner(&["broken.yaml"]);
    assert_eq!(
        run,
        Run {
            code: Some(1),
            stdout: BROKEN.to_owned(),
            stderr: String::new(),
        }
    );
}

#[test]
fn diagnostics_of_a_file_are_sorted_by_position() {
    let run = runner(&["anchors.yaml"]);
    assert_eq!(run.code, Some(1));
    assert_eq!(run.stdout, ANCHORS);
}

#[test]
fn files_are_printed_in_argument_order() {
    let run = runner(&["broken.yaml", "valid.yaml", "anchors.yaml"]);
    assert_eq!(run.code, Some(1));
    assert_eq!(run.stdout, format!("{BROKEN}{ANCHORS}"));
}

#[test]
fn a_relative_path_is_printed_as_given() {
    let run = runner(&["./broken.yaml"]);
    assert_eq!(run.code, Some(1));
    assert_eq!(run.stdout, format!("./{BROKEN}"));
}

#[test]
fn default_format_prints_a_snippet_without_color() {
    let run = workflint(&["-no-color", "anchors.yaml"]);
    assert_eq!(run.code, Some(1));
    assert_eq!(
        run.stdout,
        "\
anchors.yaml:3:6: anchor \"b\" is defined but not used [unused-anchor]
  |
3 |   B: &b 1
  |      ^~
anchors.yaml:4:6: anchor \"a\" is defined but not used [unused-anchor]
  |
4 |   A: &a 2
  |      ^~
"
    );
}

#[test]
fn invalid_utf8_is_a_yaml_syntax_error_at_the_first_bad_byte() {
    let run = runner(&["invalid-utf8.yaml"]);
    assert_eq!(
        run,
        Run {
            code: Some(1),
            stdout: "invalid-utf8.yaml:2:12: could not parse as YAML: invalid leading UTF-8 octet [yaml-syntax-error]\n".to_owned(),
            stderr: String::new(),
        }
    );
}

#[test]
fn missing_file_prints_an_error_and_exits_with_3() {
    let run = runner(&["missing.yaml"]);
    assert_eq!(run.code, Some(3));
    assert_eq!(run.stdout, "");
    assert!(
        run.stderr.starts_with("could not read \"missing.yaml\": "),
        "{}",
        run.stderr
    );
}

#[test]
fn missing_file_prints_no_diagnostics_of_other_files() {
    let run = runner(&["broken.yaml", "missing.yaml"]);
    assert_eq!(run.code, Some(3));
    assert_eq!(run.stdout, "");
    assert_eq!(run.stderr.lines().count(), 1, "{}", run.stderr);
}

#[test]
fn unknown_flag_prints_a_usage_error_and_exits_with_2() {
    let run = workflint(&["-oneline", "-bogus", "broken.yaml"]);
    assert_eq!(run.code, Some(2));
    assert_eq!(run.stdout, "");
    assert!(
        run.stderr
            .starts_with("flag provided but not defined: -bogus\n"),
        "{}",
        run.stderr
    );
}

#[test]
fn string_flag_without_a_value_exits_with_2() {
    let run = workflint(&["-config-file"]);
    assert_eq!(run.code, Some(2));
    assert!(
        run.stderr
            .starts_with("flag needs an argument: -config-file\n"),
        "{}",
        run.stderr
    );
}

#[test]
fn invalid_boolean_value_exits_with_2() {
    let run = workflint(&["-oneline=yes", "broken.yaml"]);
    assert_eq!(run.code, Some(2));
    assert!(
        run.stderr
            .starts_with("invalid boolean value \"yes\" for -oneline: parse error\n"),
        "{}",
        run.stderr
    );
}

#[test]
fn go_flag_forms_are_accepted() {
    let run = workflint(&[
        "--oneline=true",
        "--no-color",
        "-shellcheck",
        "",
        "--pyflakes=/usr/bin/pyflakes",
        "-config-file=config.yaml",
        "broken.yaml",
    ]);
    assert_eq!(
        run,
        Run {
            code: Some(1),
            stdout: BROKEN.to_owned(),
            stderr: String::new(),
        }
    );
}

#[test]
fn oneline_false_selects_the_default_format() {
    let run = workflint(&["-oneline=false", "-no-color", "broken.yaml"]);
    assert_eq!(run.code, Some(1));
    assert_eq!(run.stdout.lines().count(), 4, "{}", run.stdout);
}

#[test]
fn flags_end_at_the_first_file() {
    let run = workflint(&["broken.yaml", "-oneline"]);
    assert_eq!(run.code, Some(3));
    assert!(
        run.stderr.starts_with("could not read \"-oneline\": "),
        "{}",
        run.stderr
    );
}

#[test]
fn double_dash_ends_the_flags() {
    let run = workflint(&["-oneline", "--", "-missing.yaml"]);
    assert_eq!(run.code, Some(3));
    assert!(
        run.stderr.starts_with("could not read \"-missing.yaml\": "),
        "{}",
        run.stderr
    );
}

#[test]
fn no_files_is_a_usage_error() {
    let run = workflint(&["-oneline"]);
    assert_eq!(run.code, Some(2));
    assert_eq!(run.stdout, "");
    assert!(run.stderr.starts_with("no files given"), "{}", run.stderr);
}

#[test]
fn an_absolute_path_is_printed_relative_to_the_working_directory() {
    let path = cli_dir().join("broken.yaml");
    let path = path.to_str().unwrap();
    let run = runner(&[path]);
    assert_eq!(run.code, Some(1));
    assert_eq!(run.stdout, BROKEN);
}

#[test]
fn an_absolute_path_outside_the_working_directory_starts_with_parent_directories() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let file = cli_dir().join("broken.yaml");
    let output = command(&dir, &[Path::new("-oneline"), &file])
        .output()
        .unwrap();
    let run = run(output);
    let printed = Path::new("..")
        .join("tests")
        .join("cli")
        .join("broken.yaml");
    assert_eq!(run.code, Some(1));
    assert_eq!(
        run.stdout,
        format!("{}{}", printed.display(), &BROKEN["broken.yaml".len()..])
    );
}

/// Go's `os.Getwd` returns `$PWD` when it names the working directory, so a directory reached
/// through a symbolic link keeps the link's path.
#[cfg(unix)]
#[test]
fn the_working_directory_is_pwd_when_it_names_the_same_directory() {
    let link = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-link");
    if link.symlink_metadata().is_ok() {
        std::fs::remove_file(&link).unwrap();
    }
    std::os::unix::fs::symlink(cli_dir(), &link).unwrap();
    let file = link.join("broken.yaml");
    let output = command(&link, &[Path::new("-oneline"), &file])
        .env("PWD", &link)
        .output()
        .unwrap();
    let run = run(output);
    assert_eq!(run.code, Some(1));
    assert_eq!(run.stdout, BROKEN);
}

#[test]
fn a_closed_output_pipe_exits_quietly_with_the_lint_result() {
    // Enough output to fill the pipe, so the binary writes to it after it is closed.
    let args: Vec<&str> = std::iter::once("-oneline")
        .chain(std::iter::repeat_n("anchors.yaml", 2000))
        .collect();
    let mut child = command(&cli_dir(), &args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let run = run(child.wait_with_output().unwrap());
    assert_eq!(run.code, Some(1));
    assert_eq!(run.stderr, "");
}

#[test]
fn a_lone_dash_is_a_usage_error_until_stdin_is_supported() {
    let run = workflint(&["-oneline", "-"]);
    assert_eq!(run.code, Some(2));
    assert_eq!(run.stdout, "");
    assert!(
        run.stderr
            .starts_with("this version of workflint cannot read stdin\n"),
        "{}",
        run.stderr
    );
}

#[test]
fn a_dash_among_other_files_is_a_file_name() {
    let run = workflint(&["-oneline", "valid.yaml", "-"]);
    assert_eq!(run.code, Some(3));
    assert!(
        run.stderr.starts_with("could not read \"-\": "),
        "{}",
        run.stderr
    );
}

#[test]
fn the_usage_says_stdin_is_not_supported_yet() {
    let run = workflint(&["-bogus"]);
    assert_eq!(run.code, Some(2));
    assert!(
        run.stderr
            .contains("Reading a workflow from stdin with `-` is not supported yet."),
        "{}",
        run.stderr
    );
}

/// A temporary project with a local action whose metadata does not parse, and `count`
/// workflows that use it, `w0.yaml` to `w<count - 1>.yaml` in `.github/workflows/`.
fn project_with_broken_action(count: usize) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    let action = dir.path().join(".github/actions/broken");
    std::fs::create_dir_all(&action).unwrap();
    std::fs::write(action.join("action.yml"), "outputs: x\n").unwrap();
    let workflows = dir.path().join(".github/workflows");
    std::fs::create_dir_all(&workflows).unwrap();
    for index in 0..count {
        std::fs::write(
            workflows.join(format!("w{index}.yaml")),
            "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: ./.github/actions/broken\n",
        )
        .unwrap();
    }
    dir
}

/// Runs the binary on `files` in `dir` with `threads` threads.
fn runner_with_threads(dir: &Path, files: &[String], threads: usize) -> Run {
    let args: Vec<&str> = RUNNER
        .iter()
        .copied()
        .chain(files.iter().map(String::as_str))
        .collect();
    run(command(dir, &args)
        .env("RAYON_NUM_THREADS", threads.to_string())
        .output()
        .unwrap())
}

#[test]
fn the_first_file_by_path_reports_a_broken_local_action_whatever_the_order_and_thread_count() {
    let project = project_with_broken_action(16);
    let mut files: Vec<String> = (0..16)
        .map(|index| format!(".github/workflows/w{index}.yaml"))
        .collect();
    files.reverse();
    let one = runner_with_threads(project.path(), &files, 1);
    let lines: Vec<&str> = one
        .stdout
        .lines()
        .filter(|line| line.contains("could not parse action metadata"))
        .collect();
    assert_eq!(lines.len(), 1, "{}", one.stdout);
    assert!(
        lines[0].starts_with(".github/workflows/w0.yaml:6:15:"),
        "{}",
        lines[0]
    );
    for threads in [2, 8] {
        assert_eq!(runner_with_threads(project.path(), &files, threads), one);
    }
}
