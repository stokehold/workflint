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
fn command<S: AsRef<std::ffi::OsStr>>(dir: &Path, args: &[S]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_workflint"));
    command
        .args(args)
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
            .contains("Reading a workflow from stdin\n  with `-` is not supported yet."),
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
        "-config-file=config.yaml",
        "-init-config",
        "-stdin-filename",
        "name.yaml",
        "-shellcheck=shellcheck",
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
