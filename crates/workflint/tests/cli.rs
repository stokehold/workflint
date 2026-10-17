//! Runs the `workflint` binary on the files in `tests/cli/` with the command line the
//! conformance runner uses, and checks its output and exit code.

#![allow(clippy::unwrap_used)]

use std::fmt::Write as _;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// What one run of the binary printed and returned.
#[derive(Debug, PartialEq, Eq)]
struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// The directory of the test files, `tests/cli/`.
fn cli_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("cli")
}

/// Runs the binary with `args` in `tests/cli/`.
fn workflint(args: &[&str]) -> Run {
    workflint_in(&cli_dir(), args)
}

/// Runs the binary with `args` in `dir`, with `PWD` set to `dir` as a shell sets it.
fn workflint_in(dir: &Path, args: &[&str]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_workflint"))
        .args(args)
        .current_dir(dir)
        .env("PWD", dir)
        .output()
        .unwrap();
    Run {
        code: output.status.code(),
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
    }
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
fn relative_paths_are_printed_as_given() {
    let run = runner(&["./broken.yaml", "../cli/broken.yaml"]);
    assert_eq!(run.code, Some(1));
    assert_eq!(run.stdout, format!("./{BROKEN}../cli/{BROKEN}"));
}

#[test]
fn absolute_paths_are_printed_relative_to_the_working_directory() {
    let path = cli_dir().join("broken.yaml");
    let run = runner(&[path.to_str().unwrap()]);
    assert_eq!(run.code, Some(1));
    assert_eq!(run.stdout, BROKEN);
}

#[test]
fn absolute_paths_outside_the_working_directory_start_with_dot_dot() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let path = cli_dir().join("broken.yaml");
    let run = workflint_in(&dir, &["-oneline", path.to_str().unwrap()]);
    assert_eq!(run.code, Some(1));
    let dir = Path::new("..").join("tests").join("cli");
    let separator = std::path::MAIN_SEPARATOR;
    assert_eq!(run.stdout, format!("{}{separator}{BROKEN}", dir.display()));
}

#[test]
fn read_errors_print_the_path_as_given() {
    let path = cli_dir().join("missing.yaml");
    let path = path.to_str().unwrap();
    let run = runner(&[path]);
    assert_eq!(run.code, Some(3));
    let message = format!("could not read {path:?}: ");
    assert!(run.stderr.starts_with(&message), "{}", run.stderr);
}

/// Go's `os.Getwd` returns `PWD` when it names the working directory, so a path through a
/// symbolic link that the shell followed stays relative.
#[cfg(unix)]
#[test]
fn working_directory_is_pwd_when_it_names_the_same_directory() {
    let link = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-link");
    if let Err(err) = fs::remove_file(&link) {
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound, "{err}");
    }
    std::os::unix::fs::symlink(cli_dir(), &link).unwrap();
    let path = link.join("broken.yaml");
    let run = workflint_in(&link, &["-oneline", path.to_str().unwrap()]);
    assert_eq!(run.code, Some(1));
    assert_eq!(run.stdout, BROKEN);
}

#[cfg(unix)]
#[test]
fn pwd_naming_another_directory_is_ignored() {
    let path = cli_dir().join("broken.yaml");
    let output = Command::new(env!("CARGO_BIN_EXE_workflint"))
        .args(["-oneline", path.to_str().unwrap()])
        .current_dir(cli_dir())
        .env("PWD", "/")
        .output()
        .unwrap();
    assert_eq!(String::from_utf8(output.stdout).unwrap(), BROKEN);
}

#[test]
fn closed_output_pipe_exits_quietly_with_the_lint_result() {
    // Enough diagnostics to fill any pipe buffer, so that the binary is still writing when the
    // reader closes its end.
    let mut text = String::from("on: push\nenv:\n");
    for index in 0..20_000 {
        writeln!(text, "  K{index}: &a{index} 1").unwrap();
    }
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("many-anchors.yaml");
    fs::write(&path, text).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_workflint"))
        .args(["-no-color", path.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut first = String::new();
    stdout.read_line(&mut first).unwrap();
    drop(stdout);
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    let status = child.wait().unwrap();

    assert!(first.contains("many-anchors.yaml:"), "{first}");
    assert_eq!(stderr, "");
    assert_eq!(status.code(), Some(1));
}

#[test]
fn a_lone_dash_says_that_stdin_is_not_supported_yet() {
    let run = workflint(&["-oneline", "-"]);
    assert_eq!(run.code, Some(2));
    assert_eq!(run.stdout, "");
    assert!(
        run.stderr
            .starts_with("reading standard input is not supported yet\n"),
        "{}",
        run.stderr
    );
}

#[test]
fn usage_says_that_stdin_is_not_supported_yet() {
    let run = workflint(&["-bogus"]);
    assert_eq!(run.code, Some(2));
    assert!(
        run.stderr.contains("does not read standard input yet"),
        "{}",
        run.stderr
    );
}

#[test]
fn a_dash_next_to_other_files_is_a_file_name() {
    let run = runner(&["-", "valid.yaml"]);
    assert_eq!(run.code, Some(3));
    assert!(
        run.stderr.starts_with("could not read \"-\": "),
        "{}",
        run.stderr
    );
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
