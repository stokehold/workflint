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

/// A command that runs the binary with `args` in `dir`, without the environment variables
/// that change its output.
///
/// Unless `args` set `-shellcheck`, the command starts with `-shellcheck=`, after a leading
/// `check`, so the output does not depend on whether shellcheck is installed.
fn command<S: AsRef<std::ffi::OsStr>>(dir: &Path, args: &[S]) -> Command {
    let args: Vec<&std::ffi::OsStr> = args.iter().map(AsRef::as_ref).collect();
    let sets_shellcheck = args.iter().any(|arg| {
        let arg = arg.to_string_lossy();
        arg.starts_with("-shellcheck") || arg.starts_with("--shellcheck")
    });
    let mut command = Command::new(env!("CARGO_BIN_EXE_workflint"));
    let (first, rest) = match args.split_first() {
        Some((first, rest)) if *first == "check" => (Some(*first), rest),
        _ => (None, &args[..]),
    };
    command.args(first);
    if !sets_shellcheck {
        command.arg("-shellcheck=");
    }
    command
        .args(rest)
        .current_dir(dir)
        .env_remove("NO_COLOR")
        .env_remove("TERM")
        .env_remove("WORKFLINT_INSTALLED_FROM");
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

/// The runner's flags, with external tools off and the empty config file `config.yaml`.
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
fn a_directory_argument_prints_an_error_and_exits_with_3() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    let run = workflint_in(dir.path(), &["-oneline", "sub"]);
    assert_eq!(run.code, Some(3));
    assert_eq!(run.stdout, "");
    assert!(
        run.stderr.starts_with("could not read \"sub\": "),
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
fn a_dash_among_other_files_is_a_file_name() {
    let run = workflint(&["-oneline", "valid.yaml", "-"]);
    assert_eq!(run.code, Some(3));
    assert!(
        run.stderr.starts_with("could not read \"-\": "),
        "{}",
        run.stderr
    );
}

/// A temporary project with a local action whose metadata does not parse, `count` workflows
/// that use it, `w0.yaml` to `w<count - 1>.yaml` in `.github/workflows/`, and the empty
/// `config.yaml` of [`RUNNER`].
fn project_with_broken_action(count: usize) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    std::fs::write(dir.path().join("config.yaml"), "").unwrap();
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

/// Runs the binary with `args` in `dir`.
fn workflint_in(dir: &Path, args: &[&str]) -> Run {
    run(command(dir, args).output().unwrap())
}

/// Runs the binary with `args` in `dir`, with `input` on stdin.
fn workflint_with_stdin(dir: &Path, args: &[&str], input: &str) -> Run {
    use std::io::Write;

    let mut child = command(dir, args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    run(child.wait_with_output().unwrap())
}

/// `path` with the separators of the platform, as workflint prints a path that it computes.
fn native(path: &str) -> String {
    path.replace('/', std::path::MAIN_SEPARATOR_STR)
}

/// The header lines of the diagnostics in `stdout` that say the local action is broken.
fn broken_action_lines(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .filter(|line| line.contains("could not parse action metadata"))
        .collect()
}

#[test]
fn no_arguments_lint_the_workflows_of_the_project() {
    let project = project_with_broken_action(2);
    let run = workflint_in(project.path(), &["-oneline"]);
    assert_eq!(run.code, Some(1));
    assert_eq!(run.stderr, "");
    let lines = broken_action_lines(&run.stdout);
    assert_eq!(lines.len(), 1, "{}", run.stdout);
    assert!(
        lines[0].starts_with(&format!("{}:6:15: ", native(".github/workflows/w0.yaml"))),
        "{}",
        run.stdout
    );
}

#[test]
fn no_arguments_in_a_subdirectory_print_paths_from_it() {
    let project = project_with_broken_action(1);
    let sub = project.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    let run = workflint_in(&sub, &["-oneline"]);
    assert_eq!(run.code, Some(1));
    let lines = broken_action_lines(&run.stdout);
    assert_eq!(lines.len(), 1, "{}", run.stdout);
    assert!(
        lines[0].starts_with(&format!(
            "{}:6:15: ",
            native("../.github/workflows/w0.yaml")
        )),
        "{}",
        run.stdout
    );
}

#[test]
fn no_arguments_without_a_project_exit_with_3() {
    let dir = tempfile::tempdir().unwrap();
    let run = workflint_in(dir.path(), &["-oneline"]);
    assert_eq!(run.code, Some(3));
    assert_eq!(run.stdout, "");
    assert!(
        run.stderr
            .starts_with("no project was found in any parent directories of \""),
        "{}",
        run.stderr
    );
    assert!(
        run.stderr
            .ends_with("\". check workflows directory is put correctly in your Git repository\n"),
        "{}",
        run.stderr
    );
}

#[test]
fn no_arguments_without_a_yaml_file_exit_with_3() {
    let project = project_with_broken_action(0);
    let run = workflint_in(project.path(), &["-oneline"]);
    assert_eq!(run.code, Some(3));
    assert_eq!(run.stdout, "");
    assert!(
        run.stderr.starts_with("no YAML file was found in \""),
        "{}",
        run.stderr
    );
}

const NO_STEPS: &str = "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n";

#[test]
fn a_lone_dash_reads_stdin_and_names_it_stdin() {
    let run = workflint_with_stdin(&cli_dir(), &["-oneline", "-"], NO_STEPS);
    assert_eq!(
        run,
        Run {
            code: Some(1),
            stdout:
                "<stdin>:3:3: \"steps\" section is missing in job \"test\" [missing-required-key]\n"
                    .to_owned(),
            stderr: String::new(),
        }
    );
}

#[test]
fn stdin_filename_names_the_input_from_stdin() {
    let run = workflint_with_stdin(
        &cli_dir(),
        &["-oneline", "-stdin-filename", "from/stdin.yaml", "-"],
        NO_STEPS,
    );
    assert_eq!(run.code, Some(1));
    assert_eq!(
        run.stdout,
        "from/stdin.yaml:3:3: \"steps\" section is missing in job \"test\" [missing-required-key]\n"
    );
}

#[test]
fn empty_stdin_is_an_empty_workflow_and_exits_with_1() {
    let run = workflint_with_stdin(&cli_dir(), &["-oneline", "-"], "");
    assert_eq!(run.code, Some(1));
    assert_eq!(
        run.stdout,
        "<stdin>:1:1: workflow is empty [empty-workflow]\n"
    );
}

#[test]
fn stdin_filename_of_a_file_in_a_project_checks_its_local_actions() {
    let project = project_with_broken_action(1);
    let workflow =
        std::fs::read_to_string(project.path().join(".github/workflows/w0.yaml")).unwrap();
    let run = workflint_with_stdin(
        project.path(),
        &[
            "-oneline",
            "-stdin-filename",
            ".github/workflows/w0.yaml",
            "-",
        ],
        &workflow,
    );
    assert_eq!(run.code, Some(1));
    let lines = broken_action_lines(&run.stdout);
    assert_eq!(lines.len(), 1, "{}", run.stdout);
    assert!(
        lines[0].starts_with(".github/workflows/w0.yaml:6:15: "),
        "{}",
        run.stdout
    );
    // Without a file at the path, there is no project and no local action to check.
    let run = workflint_with_stdin(
        project.path(),
        &[
            "-oneline",
            "-stdin-filename",
            ".github/workflows/new.yaml",
            "-",
        ],
        &workflow,
    );
    assert_eq!(run.code, Some(0), "{}", run.stdout);
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

/// The usage that `-h` prints and an invalid command line prints after its error.
fn usage() -> String {
    workflint(&["-h"]).stderr
}

#[test]
fn h_and_help_print_the_usage_to_stderr_and_exit_with_0() {
    for flag in ["-h", "-help", "--help", "--h"] {
        let run = workflint(&[flag, "broken.yaml"]);
        assert_eq!(run.code, Some(0), "{flag}");
        assert_eq!(run.stdout, "", "{flag}");
        assert!(
            run.stderr
                .starts_with("Usage: workflint [check] [FLAGS] [FILES...] [-]\n"),
            "{flag}: {}",
            run.stderr
        );
    }
}

#[test]
fn the_usage_lists_every_flag_of_actionlint_and_workflint() {
    let usage = usage();
    for flag in [
        "-color",
        "-config string",
        "-config-file string",
        "-debug",
        "-format string",
        "-ignore value",
        "-init-config",
        "-no-color",
        "-oneline",
        "-pyflakes string",
        "-q",
        "-quiet",
        "-shellcheck string",
        "-silent",
        "-stdin-filename string",
        "-summary",
        "-verbose",
        "-version",
    ] {
        assert!(
            usage
                .lines()
                .any(|line| line.starts_with(&format!("  {flag}"))),
            "{flag} missing from\n{usage}"
        );
    }
}

#[test]
fn an_invalid_command_line_prints_its_error_and_then_the_usage() {
    let run = workflint(&["-bogus"]);
    assert_eq!(run.code, Some(2));
    assert_eq!(
        run.stderr,
        format!("flag provided but not defined: -bogus\n{}", usage())
    );
}

#[test]
fn version_prints_three_lines_to_stdout_and_exits_with_0() {
    let run = workflint(&["-version"]);
    assert_eq!(run.code, Some(0));
    assert_eq!(run.stderr, "");
    let lines: Vec<&str> = run.stdout.lines().collect();
    assert_eq!(lines.len(), 3, "{}", run.stdout);
    assert_eq!(lines[0], env!("CARGO_PKG_VERSION"));
    assert_eq!(lines[1], "installed by building from source");
    let target = lines[2]
        .strip_prefix("built with rustc ")
        .and_then(|rest| rest.split_once(" for "))
        .map(|(_, target)| target);
    let (os, arch) = target.and_then(|target| target.split_once('/')).unwrap();
    assert!(["darwin", "linux", "windows"].contains(&os), "{}", lines[2]);
    assert!(["amd64", "arm64"].contains(&arch), "{}", lines[2]);
}

#[test]
fn version_names_the_install_channel_from_the_environment() {
    let run = run(command(&cli_dir(), &["--version"])
        .env("WORKFLINT_INSTALLED_FROM", "installed from npm")
        .output()
        .unwrap());
    assert_eq!(run.stdout.lines().nth(1), Some("installed from npm"));
}

#[test]
fn version_wins_over_the_files() {
    let run = workflint(&["-version", "missing.yaml"]);
    assert_eq!(run.code, Some(0));
    assert_eq!(run.stdout.lines().count(), 3);
}

#[test]
fn check_is_the_default_command() {
    assert_eq!(
        workflint(&["check", "-oneline", "broken.yaml"]),
        workflint(&["-oneline", "broken.yaml"])
    );
}

#[test]
fn ignore_drops_the_diagnostics_whose_message_matches() {
    let run = workflint(&["-oneline", "-ignore", "anchor \"b\"", "anchors.yaml"]);
    assert_eq!(run.code, Some(1));
    assert_eq!(
        run.stdout,
        ANCHORS.lines().nth(1).unwrap().to_owned() + "\n"
    );
}

#[test]
fn ignore_is_repeatable_and_a_run_without_diagnostics_left_exits_with_0() {
    let run = workflint(&[
        "-oneline",
        "-ignore=anchor \"a\"",
        "--ignore",
        "anchor \"b\"",
        "anchors.yaml",
    ]);
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
fn ignore_matches_go_syntax() {
    // In Go, `\d` is ASCII and `{` without a count is a literal.
    let run = workflint(&[
        "-oneline",
        "-ignore",
        "^anchor \"\\w\" is {?defined",
        "anchors.yaml",
    ]);
    assert_eq!(run.code, Some(0), "{}", run.stdout);
}

#[test]
fn an_invalid_ignore_pattern_prints_the_go_error_and_exits_with_3() {
    let run = workflint(&["-ignore", "(", "anchors.yaml"]);
    assert_eq!(
        run,
        Run {
            code: Some(3),
            stdout: String::new(),
            stderr: "invalid regular expression for ignore pattern \"(\": error parsing regexp: \
                     missing closing ): `(`\n"
                .to_owned(),
        }
    );
}

#[test]
fn flags_of_later_tasks_are_accepted_and_ignored() {
    let run = workflint(&[
        "-oneline",
        "-format",
        "{{json .}}",
        "-stdin-filename",
        "name.yaml",
        "-pyflakes",
        "pyflakes",
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

/// The colored one-line output of `broken.yaml`. As in the reference, the reset after the rule
/// ID comes after the line break.
const BROKEN_COLOR: &str = "\x1b[33mbroken.yaml\x1b[0m\x1b[90m:\x1b[0m5\x1b[90m:\x1b[0m12\x1b[90m: \x1b[0m\x1b[1mcould not parse as YAML: block sequence entries are not allowed in this context\x1b[0m\x1b[90m [yaml-syntax-error]\n\x1b[0m";

#[test]
fn output_to_a_pipe_has_no_color() {
    let run = workflint(&["-oneline", "broken.yaml"]);
    assert_eq!(run.stdout, BROKEN);
}

#[test]
fn color_forces_color_on_a_pipe_and_with_no_color_set() {
    let run = run(command(&cli_dir(), &["-oneline", "-color", "broken.yaml"])
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .output()
        .unwrap());
    assert_eq!(run.stdout, BROKEN_COLOR);
}

#[test]
fn no_color_wins_over_color() {
    for args in [["-color", "-no-color"], ["-no-color", "-color"]] {
        let args: Vec<&str> = ["-oneline"]
            .iter()
            .chain(&args)
            .chain(&["broken.yaml"])
            .copied()
            .collect();
        assert_eq!(workflint(&args).stdout, BROKEN, "{args:?}");
    }
}

/// The regex of upstream `.github/actionlint-matcher.json`, which GitHub's problem matchers
/// apply to each line of a step's output.
const MATCHER: &str = r"^(?:\x1b\[\d+m)?(.+?)(?:\x1b\[\d+m)*:(?:\x1b\[\d+m)*(\d+)(?:\x1b\[\d+m)*:(?:\x1b\[\d+m)*(\d+)(?:\x1b\[\d+m)*: (?:\x1b\[\d+m)*(.+?)(?:\x1b\[\d+m)* \[(.+?)\]$";

#[test]
fn the_problem_matcher_matches_the_default_output_with_and_without_color() {
    let matcher = regex::Regex::new(MATCHER).unwrap();
    for color in ["-color", "-no-color"] {
        let run = workflint(&[color, "anchors.yaml", "broken.yaml"]);
        let found: Vec<Vec<&str>> = run
            .stdout
            .lines()
            .filter_map(|line| matcher.captures(line))
            .map(|groups| {
                groups
                    .iter()
                    .skip(1)
                    .map(|group| group.map_or("", |group| group.as_str()))
                    .collect()
            })
            .collect();
        assert_eq!(
            found,
            [
                vec![
                    "anchors.yaml",
                    "3",
                    "6",
                    "anchor \"b\" is defined but not used",
                    "unused-anchor"
                ],
                vec![
                    "anchors.yaml",
                    "4",
                    "6",
                    "anchor \"a\" is defined but not used",
                    "unused-anchor"
                ],
                vec![
                    "broken.yaml",
                    "5",
                    "12",
                    "could not parse as YAML: block sequence entries are not allowed in this \
                     context",
                    "yaml-syntax-error"
                ],
            ],
            "{color}:\n{}",
            run.stdout
        );
    }
}

#[test]
fn silent_prints_nothing_and_keeps_the_exit_code() {
    assert_eq!(
        workflint(&["-silent", "broken.yaml"]),
        Run {
            code: Some(1),
            stdout: String::new(),
            stderr: String::new(),
        }
    );
    assert_eq!(workflint(&["-silent", "valid.yaml"]).code, Some(0));
}

#[test]
fn silent_still_prints_fatal_messages() {
    let run = workflint(&["-silent", "missing.yaml"]);
    assert_eq!(run.code, Some(3));
    assert!(run.stderr.starts_with("could not read"), "{}", run.stderr);
}

#[test]
fn summary_prints_the_counts_by_severity_to_stderr() {
    let run = workflint(&[
        "-oneline",
        "-summary",
        "anchors.yaml",
        "broken.yaml",
        "valid.yaml",
    ]);
    assert_eq!(run.code, Some(1));
    assert_eq!(run.stdout, format!("{ANCHORS}{BROKEN}"));
    assert_eq!(
        run.stderr,
        "3 problems in 3 files: 3 errors, 0 warnings, 0 info\n"
    );
}

#[test]
fn summary_of_a_clean_run_counts_nothing() {
    let run = workflint(&["--summary", "valid.yaml"]);
    assert_eq!(run.code, Some(0));
    assert_eq!(
        run.stderr,
        "0 problems in 1 file: 0 errors, 0 warnings, 0 info\n"
    );
}

#[test]
fn quiet_turns_off_the_summary_and_the_log() {
    for quiet in ["-q", "-quiet"] {
        let run = workflint(&[quiet, "-summary", "-verbose", "-oneline", "broken.yaml"]);
        assert_eq!(
            run,
            Run {
                code: Some(1),
                stdout: BROKEN.to_owned(),
                stderr: String::new(),
            },
            "{quiet}"
        );
    }
}

#[test]
fn verbose_logs_each_file_to_stderr() {
    let run = workflint(&["-verbose", "-oneline", "anchors.yaml", "valid.yaml"]);
    assert_eq!(run.code, Some(1));
    assert_eq!(run.stdout, ANCHORS);
    let lines: Vec<&str> = run.stderr.lines().collect();
    assert_eq!(
        lines.first(),
        Some(&"verbose: Linting 2 files"),
        "{}",
        run.stderr
    );
    assert_eq!(
        lines.last(),
        Some(&"verbose: Found 2 errors in 2 files"),
        "{}",
        run.stderr
    );
    for file in ["anchors.yaml", "valid.yaml"] {
        assert!(
            lines.contains(&format!("verbose: Linting {file}").as_str()),
            "{}",
            run.stderr
        );
    }
    assert!(
        lines.iter().all(|line| line.starts_with("verbose: ")),
        "{}",
        run.stderr
    );
}

#[test]
fn debug_adds_linter_lines_and_includes_the_verbose_ones() {
    for args in [&["-debug"][..], &["-verbose", "-debug"]] {
        let args: Vec<&str> = args
            .iter()
            .chain(&["-oneline", "-ignore=anchor \"a\"", "anchors.yaml"])
            .copied()
            .collect();
        let run = workflint(&args);
        let lines: Vec<&str> = run.stderr.lines().collect();
        for expected in [
            "verbose: Linting anchors.yaml",
            "[Linter] unused-anchor found 2 errors",
            "[Linter] Error \"anchor \\\"a\\\" is defined but not used\" is ignored due to -ignore command line option",
            "verbose: Filtered 1 error(s) due to \"-ignore\" command line option and \"ignore\" configuration",
            "verbose: Found total 1 errors in 0 ms for anchors.yaml",
        ] {
            let found = lines.iter().any(|line| {
                // The time a file takes may be over 0 ms.
                line == &expected
                    || expected.starts_with("verbose: Found total")
                        && line.starts_with("verbose: Found total 1 errors in ")
                        && line.ends_with(" ms for anchors.yaml")
            });
            assert!(found, "{expected:?} not in {args:?}:\n{}", run.stderr);
        }
    }
}

#[test]
fn the_log_lines_of_different_files_never_interleave() {
    let names: Vec<String> = (0..64).map(|index| format!("w{index:02}.yaml")).collect();
    let dir = tempfile::tempdir().unwrap();
    for name in &names {
        std::fs::write(
            dir.path().join(name),
            "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: &a echo\n",
        )
        .unwrap();
    }
    let args: Vec<&str> = ["-debug", "-oneline"]
        .into_iter()
        .chain(names.iter().map(String::as_str))
        .collect();
    let run = run(command(dir.path(), &args)
        .env("RAYON_NUM_THREADS", "8")
        .output()
        .unwrap());
    // Each file's lines are one block that starts with `Linting <file>` and ends with
    // `Found total ... for <file>`.
    let mut current: Option<&str> = None;
    let mut blocks = 0;
    for line in run.stderr.lines() {
        if let Some(file) = line
            .strip_prefix("verbose: Linting ")
            .filter(|file| file.starts_with('w'))
        {
            assert_eq!(
                current, None,
                "{file} starts inside a block:\n{}",
                run.stderr
            );
            current = Some(file);
        } else if let Some(file) = current {
            if line.starts_with("verbose: Found total ") {
                assert!(
                    line.ends_with(&format!(" ms for {file}")),
                    "{line}:\n{}",
                    run.stderr
                );
                current = None;
                blocks += 1;
            } else {
                assert!(line.starts_with("[Linter] "), "{line}:\n{}", run.stderr);
            }
        }
    }
    assert_eq!(blocks, names.len(), "{}", run.stderr);
}

/// A file with a `run:` script that shellcheck reports.
const UNQUOTED: &str =
    "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo $FOO\n";

/// Writes `files` into a new directory with a `bin` directory that has no shellcheck, for `PATH`.
fn without_shellcheck(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("bin")).unwrap();
    for (name, text) in files {
        std::fs::write(dir.path().join(name), text).unwrap();
    }
    dir
}

/// Runs the binary with `args` in `dir`, with `dir/bin` as the only directory on `PATH`.
fn run_without_shellcheck(dir: &Path, args: &[&str]) -> Run {
    let mut command = Command::new(env!("CARGO_BIN_EXE_workflint"));
    command
        .args(args)
        .current_dir(dir)
        .env("PATH", dir.join("bin"))
        .env_remove("NO_COLOR")
        .env_remove("TERM");
    run(command.output().unwrap())
}

const MISSING_SHELLCHECK: &str = "warning: shellcheck was not found on PATH, so the scripts of \
                                  run: steps are not checked. Install shellcheck, or pass \
                                  -shellcheck= to turn the check off\n";

#[test]
fn a_missing_default_shellcheck_prints_one_warning_and_lints_every_file() {
    let dir = without_shellcheck(&[("a.yaml", UNQUOTED), ("b.yaml", UNQUOTED)]);
    let run = run_without_shellcheck(dir.path(), &["-oneline", "a.yaml", "b.yaml"]);
    assert_eq!(
        run,
        Run {
            code: Some(0),
            stdout: String::new(),
            stderr: MISSING_SHELLCHECK.to_owned(),
        }
    );
}

#[test]
fn quiet_and_silent_hide_the_warning_about_a_missing_shellcheck() {
    let dir = without_shellcheck(&[("a.yaml", UNQUOTED)]);
    for flag in ["-quiet", "-silent"] {
        let run = run_without_shellcheck(dir.path(), &[flag, "a.yaml"]);
        assert_eq!(run.stderr, "", "{flag}");
        assert_eq!(run.code, Some(0), "{flag}");
    }
}

#[test]
fn a_missing_shellcheck_named_by_the_flag_exits_with_3() {
    let dir = without_shellcheck(&[("a.yaml", UNQUOTED)]);
    for (flag, error) in [
        (
            "-shellcheck=shellcheck",
            "executable \"shellcheck\" was not found on PATH",
        ),
        (
            "-shellcheck=./no/shellcheck",
            "executable \"./no/shellcheck\" was not found",
        ),
    ] {
        let run = run_without_shellcheck(dir.path(), &[flag, "a.yaml"]);
        assert_eq!(
            run,
            Run {
                code: Some(3),
                stdout: String::new(),
                stderr: format!("shellcheck given with -shellcheck was not found: {error}\n"),
            },
            "{flag}"
        );
    }
}

#[test]
fn an_empty_shellcheck_flag_turns_the_check_off_without_a_message() {
    let dir = without_shellcheck(&[("a.yaml", UNQUOTED)]);
    let run = run_without_shellcheck(dir.path(), &["-shellcheck=", "a.yaml"]);
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
fn shellcheck_from_path_checks_run_scripts() {
    let Some(shellcheck) = std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join("shellcheck"))
            .find(|path| path.is_file())
    }) else {
        eprintln!("skipped: shellcheck is not installed");
        return;
    };
    let dir = without_shellcheck(&[("a.yaml", UNQUOTED)]);
    std::fs::copy(&shellcheck, dir.path().join("bin").join("shellcheck")).unwrap();
    let run = run_without_shellcheck(dir.path(), &["-oneline", "-jobs=1", "a.yaml"]);
    assert_eq!(
        run,
        Run {
            code: Some(1),
            stdout: "a.yaml:6:19: shellcheck reported issue in this script: SC2086:info:1:6: \
                     Double quote to prevent globbing and word splitting [shellcheck-issue]\n"
                .to_owned(),
            stderr: String::new(),
        }
    );
}

#[test]
fn jobs_must_be_a_positive_whole_number() {
    for (value, reason) in [("0", "must be at least 1"), ("two", "parse error")] {
        let run = workflint(&[&format!("-jobs={value}"), "valid.yaml"]);
        assert_eq!(run.code, Some(2), "{value}");
        assert!(
            run.stderr.starts_with(&format!(
                "invalid value \"{value}\" for flag -jobs: {reason}\n"
            )),
            "{}",
            run.stderr
        );
    }
}

/// The path of `dir` that the binary gets from `getcwd` when it runs in `dir`: `dir` with
/// symbolic links resolved, as on macOS, where the temporary directory is under a link, and
/// without the `\\?\` prefix that `canonicalize` adds on Windows.
fn working_dir_of(dir: &Path) -> PathBuf {
    let canonical = dir.canonicalize().unwrap();
    match canonical
        .to_str()
        .and_then(|path| path.strip_prefix(r"\\?\"))
    {
        Some(path) => PathBuf::from(path),
        None => canonical,
    }
}

/// A temporary project: a directory with `.git` and `.github/workflows`.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    std::fs::create_dir_all(dir.path().join(".github/workflows")).unwrap();
    dir
}

/// A workflow that runs on the custom label `gpu`.
const CUSTOM_LABEL: &str =
    "on: push\njobs:\n  test:\n    runs-on: gpu\n    steps:\n      - run: echo\n";

#[test]
fn init_config_writes_workflint_yaml_ignores_the_arguments_and_exits_with_0() {
    let project = project();
    let root = working_dir_of(project.path());
    let sub = root.as_path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    let run = run(command(&sub, &["-init-config", "missing.yaml"])
        .output()
        .unwrap());
    let path = root.as_path().join(".github").join("workflint.yaml");
    assert_eq!(
        run,
        Run {
            code: Some(0),
            stdout: format!(
                "Config file was generated at {:?}\n",
                path.display().to_string()
            ),
            stderr: String::new(),
        }
    );
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("config-variables: null\n"), "{written}");
}

#[test]
fn init_config_with_an_existing_config_exits_with_3() {
    let project = project();
    let root = working_dir_of(project.path());
    let existing = root.as_path().join(".github").join("actionlint.yml");
    std::fs::write(&existing, "").unwrap();
    let run = run(command(root.as_path(), &["-init-config"]).output().unwrap());
    assert_eq!(
        run,
        Run {
            code: Some(3),
            stdout: String::new(),
            stderr: format!(
                "config file already exists at {:?}\n",
                existing.display().to_string()
            ),
        }
    );
    assert!(!root.as_path().join(".github/workflint.yaml").exists());
}

#[test]
fn the_project_config_reaches_the_rules() {
    let project = project();
    let root = working_dir_of(project.path());
    let workflow = ".github/workflows/a.yaml";
    std::fs::write(root.as_path().join(workflow), CUSTOM_LABEL).unwrap();
    let before = run(command(root.as_path(), &["-oneline", workflow])
        .output()
        .unwrap());
    assert_eq!(before.code, Some(1), "{before:?}");
    std::fs::write(
        root.as_path().join(".github/workflint.yaml"),
        "self-hosted-runner:\n  labels: [gpu]\n",
    )
    .unwrap();
    let after = run(command(root.as_path(), &["-oneline", workflow])
        .output()
        .unwrap());
    assert_eq!(
        after,
        Run {
            code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
        }
    );
}

#[test]
fn config_file_and_config_name_the_config_to_use() {
    let project = project();
    let root = working_dir_of(project.path());
    let workflow = ".github/workflows/a.yaml";
    std::fs::write(root.as_path().join(workflow), CUSTOM_LABEL).unwrap();
    std::fs::write(
        root.as_path().join("lint.yaml"),
        "self-hosted-runner:\n  labels: [gpu]\n",
    )
    .unwrap();
    for flag in ["-config-file", "--config", "-config"] {
        let run = run(command(root.as_path(), &[flag, "lint.yaml", workflow])
            .output()
            .unwrap());
        assert_eq!(run.code, Some(0), "{flag}: {run:?}");
    }
}

#[test]
fn a_config_file_that_cannot_be_read_exits_with_3() {
    let run = workflint(&["-config-file", "missing.yaml", "valid.yaml"]);
    assert_eq!(run.code, Some(3));
    assert_eq!(run.stdout, "");
    assert!(
        run.stderr
            .starts_with("could not read config file \"missing.yaml\": "),
        "{}",
        run.stderr
    );
}

#[test]
fn a_broken_project_config_exits_with_3_before_any_output() {
    let project = project();
    let root = working_dir_of(project.path());
    let workflow = ".github/workflows/a.yaml";
    std::fs::write(root.as_path().join(workflow), CUSTOM_LABEL).unwrap();
    let config = root.as_path().join(".github").join("actionlint.yaml");
    std::fs::write(&config, "paths:\n  a:\n    ignore: ['(']\n").unwrap();
    let run = run(command(root.as_path(), &["-oneline", workflow])
        .output()
        .unwrap());
    assert_eq!(
        run,
        Run {
            code: Some(3),
            stdout: String::new(),
            stderr: format!(
                "could not parse config file {:?}: yaml: unmarshal errors:   line 3: invalid \
                 regular expression \"(\" in \"ignore\" at line:3,col:14: error parsing regexp: \
                 missing closing ): `(`\n",
                config.display().to_string()
            ),
        }
    );
}

#[test]
fn an_unknown_key_in_a_workflint_config_exits_with_3() {
    let project = project();
    let root = working_dir_of(project.path());
    let workflow = ".github/workflows/a.yaml";
    std::fs::write(root.as_path().join(workflow), CUSTOM_LABEL).unwrap();
    let config = root.as_path().join(".github").join("workflint.yaml");
    std::fs::write(&config, "self-hosted-runners:\n  labels: [gpu]\n").unwrap();
    let run = run(command(root.as_path(), &["-oneline", workflow])
        .output()
        .unwrap());
    assert_eq!(
        run,
        Run {
            code: Some(3),
            stdout: String::new(),
            stderr: format!(
                "could not parse config file {:?}: unknown key \"self-hosted-runners\" at \
                 line:1,col:1\n",
                config.display().to_string()
            ),
        }
    );
}

#[test]
fn an_unknown_key_in_an_actionlint_config_prints_a_warning_and_the_run_goes_on() {
    let project = project();
    let root = working_dir_of(project.path());
    let workflow = ".github/workflows/a.yaml";
    std::fs::write(
        root.as_path().join(workflow),
        "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo\n",
    )
    .unwrap();
    let config = root.as_path().join(".github").join("actionlint.yaml");
    std::fs::write(&config, "self-hosted-runners:\n  labels: [gpu]\n").unwrap();
    let run = run(command(root.as_path(), &["-oneline", workflow])
        .output()
        .unwrap());
    assert_eq!(
        run,
        Run {
            code: Some(0),
            stdout: String::new(),
            stderr: format!(
                "warning: config file {:?}: unknown key \"self-hosted-runners\" at line:1,col:1 \
                 is ignored\n",
                config.display().to_string()
            ),
        }
    );
    let quiet = run_quiet(root.as_path(), workflow);
    assert_eq!(quiet.stderr, "");
}

fn run_quiet(dir: &Path, workflow: &str) -> Run {
    run(command(dir, &["-oneline", "-quiet", workflow])
        .output()
        .unwrap())
}

#[test]
fn a_run_without_arguments_uses_the_config_of_the_discovered_project() {
    let project = project();
    let root = working_dir_of(project.path());
    std::fs::write(root.join(".github/workflows/a.yaml"), CUSTOM_LABEL).unwrap();
    std::fs::write(
        root.join(".github/actionlint.yml"),
        "self-hosted-runner:\n  labels: [g*]\npaths:\n  .github/workflows/*.yaml:\n    ignore: [nothing]\n",
    )
    .unwrap();
    let sub = root.join(".github/workflows");
    let run = run(command(&sub, &["-oneline"]).output().unwrap());
    assert_eq!(
        run,
        Run {
            code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
        }
    );
}

/// A stand-in for shellcheck in `dir`: a shell script that prints `version: 0.11.0` for
/// `--version`, and otherwise adds a line to `dir/runs` and prints one finding.
#[cfg(unix)]
fn counting_shellcheck(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("shellcheck");
    let runs = dir.join("runs");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'version: 0.11.0'; exit 0; fi\n\
             cat > /dev/null\necho run >> '{}'\n\
             printf '[{{\"line\":2,\"column\":6,\"level\":\"info\",\"code\":2086,\"message\":\"Double quote.\"}}]'\n",
            runs.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(unix)]
#[test]
fn shellcheck_cache_keeps_the_findings_under_xdg_cache_home() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.yaml"), UNQUOTED).unwrap();
    let shellcheck = counting_shellcheck(dir.path());
    let flag = format!("-shellcheck={}", shellcheck.display());
    let cache = dir.path().join("cache");
    let lint = || {
        let mut command = command(
            dir.path(),
            &[flag.as_str(), "-shellcheck-cache", "-oneline", "a.yaml"],
        );
        command.env("XDG_CACHE_HOME", &cache);
        run(command.output().unwrap())
    };
    let first = lint();
    assert_eq!(first.code, Some(1), "{}", first.stderr);
    assert_eq!(
        first.stdout,
        "a.yaml:6:19: shellcheck reported issue in this script: SC2086:info:1:6: Double quote \
         [shellcheck-issue]\n"
    );
    assert_eq!(lint(), first);
    let runs = std::fs::read_to_string(dir.path().join("runs")).unwrap();
    assert_eq!(runs, "run\n");
    let entries = std::fs::read_dir(cache.join("workflint").join("shellcheck")).unwrap();
    assert_eq!(entries.count(), 1);
}
