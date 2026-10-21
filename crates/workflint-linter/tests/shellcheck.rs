//! shellcheck through the driver: the arguments and the script it gets, one run per script and
//! run, a diagnostic for each way it can fail, the positions of its findings, the settings of
//! the config file, the on-disk cache, and batches of scripts in one process.
//!
//! Most tests run `workflint-test-tool` in place of shellcheck, so they behave the same on every
//! OS. The last test runs the real shellcheck when it is on `PATH`.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tempfile::TempDir;
use workflint_diagnostics::{Color, render_oneline};
use workflint_linter::tool::{RunOptions, ToolCommand, ToolPool};
use workflint_linter::{Input, Linter, Tools};

const TEST_TOOL: &str = env!("CARGO_BIN_EXE_workflint-test-tool");

/// The stand-in for shellcheck: the test tool with `args` before shellcheck's arguments.
fn stand_in(args: &[&str]) -> ToolCommand {
    ToolCommand::new(TEST_TOOL, args.iter().copied())
}

fn tools(command: ToolCommand, timeout: Duration) -> Tools {
    tools_with_limit(command, timeout, 4)
}

/// Runs shellcheck with `command`, at most `limit` processes at once.
fn tools_with_limit(command: ToolCommand, timeout: Duration, limit: usize) -> Tools {
    let pool = ToolPool::new(NonZeroUsize::new(limit).unwrap()).unwrap();
    let options = RunOptions {
        timeout,
        ..RunOptions::default()
    };
    Tools::none().with_shellcheck(command, Arc::new(pool), options)
}

/// A workflow whose one job on `ubuntu-latest` has these steps, each indented to sit in the list.
fn workflow(steps: &[&str]) -> String {
    let mut text =
        String::from("on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n");
    for step in steps {
        text.push_str("      - ");
        text.push_str(step);
        text.push('\n');
    }
    text
}

/// Writes `files` into a new directory, lints them in order with `tools`, and returns the
/// one-line output of each.
fn lint(files: &[(&str, &str)], tools: Tools) -> Vec<String> {
    let dir = TempDir::new().unwrap();
    for (name, text) in files {
        fs::write(dir.path().join(name), text).unwrap();
    }
    let inputs: Vec<Input> = files.iter().map(|(name, _)| Input::file(name)).collect();
    let linter = Linter::new(dir.path().to_path_buf()).with_tools(tools);
    linter
        .lint(&inputs)
        .unwrap()
        .iter()
        .map(|linted| {
            let mut out = Vec::new();
            render_oneline(&mut out, &linted.file, &linted.diagnostics, Color::Off).unwrap();
            String::from_utf8(out).unwrap()
        })
        .collect()
}

/// The records that `record` wrote to `dir`, sorted.
fn records(dir: &Path) -> Vec<String> {
    let mut records: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect();
    records.sort();
    records
}

#[test]
fn shellcheck_gets_the_reference_arguments_and_the_prepared_script_on_stdin() {
    let dir = TempDir::new().unwrap();
    let record = stand_in(&["record", dir.path().to_str().unwrap()]);
    let text = workflow(&[
        "run: echo ${{ github.sha }} $FOO",
        "run: echo $BAR\n        shell: sh",
    ]);
    assert_eq!(
        lint(&[("a.yaml", &text)], tools(record, Duration::from_secs(60))),
        [""]
    );
    let args = |shell: &str| {
        format!(
            "--norc\n-f\njson\n-x\n--shell\n{shell}\n-e\nSC1091,SC2194,SC2050,SC2153,SC2154,SC2157,SC2043\n-\n--\n"
        )
    };
    assert_eq!(
        records(dir.path()),
        [
            format!(
                "{}set -eo pipefail\necho _________________ $FOO\n",
                args("bash")
            ),
            format!("{}set -e\necho $BAR\n", args("sh")),
        ]
    );
}

#[test]
fn identical_scripts_in_two_files_run_shellcheck_once() {
    let dir = TempDir::new().unwrap();
    let record = stand_in(&["record", dir.path().to_str().unwrap()]);
    let a = workflow(&["run: echo $FOO", "run: echo $FOO\n        shell: sh"]);
    let b = workflow(&["run: echo $FOO", "run: echo other", "run: echo $FOO"]);
    let outputs = lint(
        &[("a.yaml", &a), ("b.yaml", &b)],
        tools(record, Duration::from_secs(60)),
    );
    assert_eq!(outputs, ["", ""]);
    let mut scripts: Vec<String> = records(dir.path())
        .iter()
        .map(|record| record.split_once("--\n").unwrap().1.to_owned())
        .collect();
    scripts.sort();
    assert_eq!(
        scripts,
        [
            "set -e\necho $FOO\n",
            "set -eo pipefail\necho $FOO\n",
            "set -eo pipefail\necho other\n",
        ]
    );
}

#[test]
fn findings_point_at_their_place_in_the_script() {
    let json = r#"[{"file":"-","line":2,"endLine":2,"column":6,"endColumn":10,"level":"info","code":2086,"message":"Double quote to prevent globbing and word splitting.","fix":null}]"#;
    let outputs = lint(
        &[("a.yaml", &workflow(&["run: echo $FOO"]))],
        tools(stand_in(&["exit", "1", json]), Duration::from_secs(60)),
    );
    assert_eq!(
        outputs,
        [
            "a.yaml:6:19: shellcheck reported issue in this script: SC2086:info:1:6: Double quote \
          to prevent globbing and word splitting [shellcheck-issue]\n"
        ]
    );
}

/// Lints a file whose script makes `command` fail, and a file without steps. Returns the
/// message of the `shellcheck-failed` diagnostic, after checking that the other file is linted.
fn failure(command: ToolCommand, timeout: Duration) -> String {
    let no_steps = "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n";
    let outputs = lint(
        &[
            ("a.yaml", &workflow(&["run: echo $FOO"])),
            ("b.yaml", no_steps),
        ],
        tools(command, timeout),
    );
    assert!(
        outputs[1].starts_with("b.yaml:3:3: \"steps\" section is missing in job"),
        "{outputs:?}"
    );
    let line = outputs[0]
        .strip_prefix("a.yaml:6:9: ")
        .and_then(|line| line.strip_suffix(" [shellcheck-failed]\n"))
        .unwrap_or_else(|| panic!("not one shellcheck-failed diagnostic: {:?}", outputs[0]));
    line.to_owned()
}

const DID_NOT_RUN: &str = "shellcheck did not run successfully while checking this script: ";

#[test]
fn a_non_zero_exit_with_empty_stdout_is_a_diagnostic_on_the_script() {
    assert_eq!(
        failure(stand_in(&["exit", "2", ""]), Duration::from_secs(60)),
        format!(
            "{DID_NOT_RUN}shellcheck exited with status 2 but stdout was empty. stderr: \"failed\""
        )
    );
}

#[test]
fn output_that_is_not_json_is_a_diagnostic_on_the_script() {
    let message = failure(stand_in(&["exit", "1", "notjson"]), Duration::from_secs(60));
    assert!(
        message.starts_with("could not parse JSON output from shellcheck: ")
            && message.ends_with(": stdout=\"notjson\""),
        "{message}"
    );
}

#[test]
fn a_timeout_is_a_diagnostic_on_the_script() {
    assert_eq!(
        failure(stand_in(&["sleep"]), Duration::from_millis(300)),
        format!("{DID_NOT_RUN}shellcheck did not finish in 0.3 seconds and was killed")
    );
}

#[cfg(unix)]
#[test]
fn a_crash_is_a_diagnostic_on_the_script() {
    assert_eq!(
        failure(stand_in(&["kill-self"]), Duration::from_secs(60)),
        format!("{DID_NOT_RUN}shellcheck was terminated by signal 9. stderr: \"\"")
    );
}

#[test]
fn a_program_that_cannot_start_is_a_diagnostic_on_the_script() {
    let missing = TempDir::new().unwrap().path().join("shellcheck");
    let message = failure(
        ToolCommand::new(missing, Vec::<String>::new()),
        Duration::from_secs(60),
    );
    assert!(
        message.starts_with(&format!("{DID_NOT_RUN}shellcheck could not be started: ")),
        "{message}"
    );
}

/// The real shellcheck, if it is on `PATH`. The CI legs that install it run this test for real.
#[test]
fn real_shellcheck_findings_point_at_their_place_after_expressions_and_tabs() {
    let Ok(shellcheck) = ToolCommand::resolve("shellcheck") else {
        eprintln!("skipped: shellcheck is not installed");
        return;
    };
    let text = "on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: |
          echo \"${{ github.sha }}\" $FOO
          printf '%s\\n'\t$BAR
";
    let outputs = lint(
        &[("a.yaml", text)],
        tools(shellcheck, Duration::from_secs(60)),
    );
    let message = "Double quote to prevent globbing and word splitting [shellcheck-issue]";
    assert_eq!(
        outputs,
        [format!(
            "a.yaml:7:36: shellcheck reported issue in this script: SC2086:info:1:26: {message}\n\
             a.yaml:8:25: shellcheck reported issue in this script: SC2086:info:2:17: {message}\n"
        )]
    );
}

/// A temporary project with `.git`, the workflows `files` in `.github/workflows`, and
/// `.github/workflint.yaml` with `config` unless it is empty.
fn project(files: &[(&str, &str)], config: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join(".git")).unwrap();
    let workflows = dir.path().join(".github").join("workflows");
    fs::create_dir_all(&workflows).unwrap();
    if !config.is_empty() {
        fs::write(dir.path().join(".github").join("workflint.yaml"), config).unwrap();
    }
    for (name, text) in files {
        fs::write(workflows.join(name), text).unwrap();
    }
    dir
}

/// Lints the workflows `names` of the project at `root` with `linter`, and returns the one-line
/// output of each.
fn lint_in(root: &Path, names: &[&str], linter: &Linter) -> Vec<String> {
    let inputs: Vec<Input> = names
        .iter()
        .map(|name| Input::file(root.join(".github").join("workflows").join(name)))
        .collect();
    linter
        .lint(&inputs)
        .unwrap()
        .iter()
        .map(|linted| {
            let mut out = Vec::new();
            render_oneline(&mut out, &linted.file, &linted.diagnostics, Color::Off).unwrap();
            String::from_utf8(out).unwrap()
        })
        .collect()
}

/// The arguments of each record in `dir`, without the script.
fn recorded_args(dir: &Path) -> Vec<Vec<String>> {
    records(dir)
        .iter()
        .map(|record| {
            let (args, _) = record.split_once("--\n").unwrap();
            args.lines().map(str::to_owned).collect()
        })
        .collect()
}

/// shellcheck's arguments for a bash script without config: the reference's.
const REFERENCE_ARGS: [&str; 9] = [
    "--norc",
    "-f",
    "json",
    "-x",
    "--shell",
    "bash",
    "-e",
    "SC1091,SC2194,SC2050,SC2153,SC2154,SC2157,SC2043",
    "-",
];

#[test]
fn without_rcfile_shellcheck_gets_norc() {
    let log = TempDir::new().unwrap();
    let record = stand_in(&["record", log.path().to_str().unwrap()]);
    let root = project(
        &[("a.yaml", &workflow(&["run: echo $FOO"]))],
        "shellcheck:\n  args: []\n",
    );
    let linter =
        Linter::new(root.path().to_path_buf()).with_tools(tools(record, Duration::from_secs(60)));
    assert_eq!(lint_in(root.path(), &["a.yaml"], &linter), [""]);
    assert_eq!(recorded_args(log.path()), [REFERENCE_ARGS]);
}

#[test]
fn rcfile_replaces_norc_with_the_file_from_the_project_root() {
    let log = TempDir::new().unwrap();
    let record = stand_in(&["record", log.path().to_str().unwrap()]);
    let root = project(
        &[("a.yaml", &workflow(&["run: echo $FOO"]))],
        "shellcheck:\n  rcfile: .shellcheckrc\n",
    );
    fs::write(root.path().join(".shellcheckrc"), "disable=SC2086\n").unwrap();
    // The working directory is not the project root, and the path still starts at the root.
    let cwd = root.path().join(".github");
    let linter = Linter::new(cwd.clone()).with_tools(tools(record, Duration::from_secs(60)));
    assert_eq!(lint_in(root.path(), &["a.yaml"], &linter), [""]);
    let rcfile = root.path().join(".shellcheckrc");
    let mut expected = vec!["--rcfile".to_owned(), rcfile.to_str().unwrap().to_owned()];
    expected.extend(REFERENCE_ARGS[1..].iter().map(|&arg| arg.to_owned()));
    assert_eq!(recorded_args(log.path()), [expected]);
}

#[test]
fn a_missing_rcfile_stops_the_run_naming_the_file() {
    let root = project(
        &[("a.yaml", &workflow(&["run: echo $FOO"]))],
        "shellcheck:\n  rcfile: missing/.shellcheckrc\n",
    );
    let linter = Linter::new(root.path().to_path_buf()).with_tools(tools(
        stand_in(&["exit", "0", "[]"]),
        Duration::from_secs(60),
    ));
    let input = Input::file(root.path().join(".github").join("workflows").join("a.yaml"));
    let err = linter.lint(&[input]).unwrap_err().to_string();
    let config = root.path().join(".github").join("workflint.yaml");
    let rcfile = root.path().join("missing").join(".shellcheckrc");
    let prefix = format!(
        "could not read shellcheck.rcfile {:?} of config file {:?}: ",
        rcfile.to_str().unwrap(),
        config.to_str().unwrap()
    );
    assert!(err.starts_with(&prefix), "{err}");
}

#[test]
fn args_come_after_workflints_arguments_and_before_the_script() {
    let log = TempDir::new().unwrap();
    let record = stand_in(&["record", log.path().to_str().unwrap()]);
    let root = project(
        &[("a.yaml", &workflow(&["run: echo $FOO"]))],
        "shellcheck:\n  args: [--severity=warning, -o, all]\n",
    );
    let linter =
        Linter::new(root.path().to_path_buf()).with_tools(tools(record, Duration::from_secs(60)));
    assert_eq!(lint_in(root.path(), &["a.yaml"], &linter), [""]);
    let mut expected: Vec<&str> = REFERENCE_ARGS[..8].to_vec();
    expected.extend(["--severity=warning", "-o", "all", "-"]);
    assert_eq!(recorded_args(log.path()), [expected]);
}

#[test]
fn the_tool_timeout_of_the_config_file_kills_shellcheck() {
    let root = project(
        &[("a.yaml", &workflow(&["run: echo $FOO"]))],
        "tool-timeout: 1\n",
    );
    // Without the config file, the process would get 60 seconds.
    let linter = Linter::new(root.path().to_path_buf())
        .with_tools(tools(stand_in(&["sleep"]), Duration::from_secs(60)));
    assert_eq!(
        lint_in(root.path(), &["a.yaml"], &linter),
        [format!(
            ".github/workflows/a.yaml:6:9: {DID_NOT_RUN}shellcheck did not finish in 1 seconds \
             and was killed [shellcheck-failed]\n"
        )]
    );
}

/// A cache test: the stand-in that records each script and reports `version` for `--version`,
/// with the cache in `cache`.
fn cached(records: &Path, version: &str, cache: &Path) -> Tools {
    let command = stand_in(&["version", version, "record", records.to_str().unwrap()]);
    tools(command, Duration::from_secs(60)).with_cache_dir(cache)
}

/// The workflows of the cache tests and the config that turns the cache on.
fn cache_project() -> TempDir {
    project(
        &[
            ("a.yaml", &workflow(&["run: echo $FOO", "run: echo $BAR"])),
            ("b.yaml", &workflow(&["run: echo $FOO"])),
        ],
        "shellcheck:\n  cache: true\n",
    )
}

/// Lints the workflows of [`cache_project`] once with `tools`.
fn lint_cached(root: &Path, tools: Tools) -> Vec<String> {
    let linter = Linter::new(root.to_path_buf()).with_tools(tools);
    lint_in(root, &["a.yaml", "b.yaml"], &linter)
}

#[test]
fn a_second_run_with_the_cache_on_does_not_start_shellcheck_for_a_cached_script() {
    let (log, cache) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let root = cache_project();
    let first = lint_cached(root.path(), cached(log.path(), "0.11.0", cache.path()));
    assert_eq!(records(log.path()).len(), 2);
    fs::remove_dir_all(log.path()).unwrap();
    fs::create_dir(log.path()).unwrap();
    let second = lint_cached(root.path(), cached(log.path(), "0.11.0", cache.path()));
    assert_eq!(second, first);
    assert_eq!(records(log.path()), Vec::<String>::new());
}

#[test]
fn a_changed_shellcheck_version_misses_the_cache() {
    let (log, cache) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let root = cache_project();
    lint_cached(root.path(), cached(log.path(), "0.10.0", cache.path()));
    fs::remove_dir_all(log.path()).unwrap();
    fs::create_dir(log.path()).unwrap();
    lint_cached(root.path(), cached(log.path(), "0.11.0", cache.path()));
    assert_eq!(records(log.path()).len(), 2);
}

#[test]
fn a_changed_rcfile_misses_the_cache() {
    let (log, cache) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let root = project(
        &[("a.yaml", &workflow(&["run: echo $FOO"]))],
        "shellcheck:\n  cache: true\n  rcfile: .shellcheckrc\n",
    );
    let rcfile = root.path().join(".shellcheckrc");
    let lint = |log: &Path| {
        let linter =
            Linter::new(root.path().to_path_buf()).with_tools(cached(log, "0.11.0", cache.path()));
        lint_in(root.path(), &["a.yaml"], &linter)
    };
    fs::write(&rcfile, "disable=SC2086\n").unwrap();
    lint(log.path());
    lint(log.path());
    assert_eq!(records(log.path()).len(), 1);
    fs::write(&rcfile, "disable=SC2034\n").unwrap();
    lint(log.path());
    assert_eq!(records(log.path()).len(), 2);
}

#[test]
fn a_corrupt_cache_entry_is_a_miss_not_an_error() {
    let (log, cache) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let root = cache_project();
    let first = lint_cached(root.path(), cached(log.path(), "0.11.0", cache.path()));
    let mut entries = 0;
    for entry in walk(cache.path()) {
        fs::write(entry, "{\"findings\": [tru").unwrap();
        entries += 1;
    }
    assert_eq!(entries, 2);
    fs::remove_dir_all(log.path()).unwrap();
    fs::create_dir(log.path()).unwrap();
    let second = lint_cached(root.path(), cached(log.path(), "0.11.0", cache.path()));
    assert_eq!(second, first);
    assert_eq!(records(log.path()).len(), 2);
    // The entries are written again, so a third run starts no shellcheck.
    fs::remove_dir_all(log.path()).unwrap();
    fs::create_dir(log.path()).unwrap();
    lint_cached(root.path(), cached(log.path(), "0.11.0", cache.path()));
    assert_eq!(records(log.path()).len(), 0);
}

#[test]
fn the_cache_is_off_unless_the_config_or_the_flag_turns_it_on() {
    let (log, cache) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let root = project(
        &[("a.yaml", &workflow(&["run: echo $FOO"]))],
        "shellcheck:\n  cache: false\n",
    );
    let lint = |linter: Linter| lint_in(root.path(), &["a.yaml"], &linter);
    lint(Linter::new(root.path().to_path_buf()).with_tools(cached(
        log.path(),
        "0.11.0",
        cache.path(),
    )));
    assert_eq!(walk(cache.path()).len(), 0);
    lint(
        Linter::new(root.path().to_path_buf())
            .with_tools(cached(log.path(), "0.11.0", cache.path()))
            .with_shellcheck_cache(),
    );
    assert_eq!(walk(cache.path()).len(), 1);
}

#[test]
fn a_script_that_shellcheck_could_not_check_is_not_cached() {
    let cache = TempDir::new().unwrap();
    let root = cache_project();
    let command = stand_in(&["version", "0.11.0", "exit", "2", ""]);
    let tools = tools(command, Duration::from_secs(60)).with_cache_dir(cache.path());
    let outputs = lint_cached(root.path(), tools);
    assert!(outputs[0].contains("[shellcheck-failed]"), "{outputs:?}");
    assert_eq!(walk(cache.path()).len(), 0);
}

/// The files under `dir`, at any depth.
fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let mut work = vec![dir.to_path_buf()];
    while let Some(dir) = work.pop() {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                work.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files
}

/// The number of processes that wrote the records in `dir`.
fn processes(dir: &Path) -> usize {
    let mut pids: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| {
            let name = entry.unwrap().file_name().into_string().unwrap();
            name.split_once('-').unwrap().0.to_owned()
        })
        .collect();
    pids.sort();
    pids.dedup();
    pids.len()
}

#[test]
fn the_scripts_of_several_files_run_in_one_shellcheck_process() {
    let dir = TempDir::new().unwrap();
    let record = stand_in(&["record", dir.path().to_str().unwrap()]);
    let a = workflow(&["run: echo $A1", "run: echo $A2"]);
    let b = workflow(&["run: echo $B1", "run: echo $B2"]);
    let c = workflow(&[
        "run: echo $C1\n        shell: sh",
        "run: echo $C2\n        shell: sh",
    ]);
    let outputs = lint(
        &[("a.yaml", &a), ("b.yaml", &b), ("c.yaml", &c)],
        tools_with_limit(record, Duration::from_secs(60), 1),
    );
    assert_eq!(outputs, ["", "", ""]);
    // One process for the bash scripts and one for the sh scripts, which need other arguments.
    assert_eq!(processes(dir.path()), 2);
    let mut scripts: Vec<String> = records(dir.path())
        .iter()
        .map(|record| record.split_once("--\n").unwrap().1.to_owned())
        .collect();
    scripts.sort();
    assert_eq!(
        scripts,
        [
            "set -e\necho $C1\n",
            "set -e\necho $C2\n",
            "set -eo pipefail\necho $A1\n",
            "set -eo pipefail\necho $A2\n",
            "set -eo pipefail\necho $B1\n",
            "set -eo pipefail\necho $B2\n",
        ]
    );
}

#[test]
fn each_finding_of_a_batch_goes_to_its_script() {
    let a = workflow(&["run: echo one", "run: echo two"]);
    let b = workflow(&["run: echo three"]);
    for limit in [1, 4] {
        let outputs = lint(
            &[("a.yaml", &a), ("b.yaml", &b)],
            tools_with_limit(stand_in(&["per-file"]), Duration::from_secs(60), limit),
        );
        let finding = |line: usize, text: &str| {
            format!(
                "{}.yaml:{line}:14: shellcheck reported issue in this script: SC1000:info:1:1: \
                 {text} [shellcheck-issue]\n",
                if text == "echo three" { "b" } else { "a" }
            )
        };
        assert_eq!(
            outputs,
            [
                finding(6, "echo one") + &finding(7, "echo two"),
                finding(6, "echo three"),
            ],
            "limit {limit}"
        );
    }
}

#[test]
fn a_batch_that_fails_runs_each_script_alone() {
    let dir = TempDir::new().unwrap();
    let command = stand_in(&["no-batch", "record", dir.path().to_str().unwrap()]);
    let a = workflow(&["run: echo $A1", "run: echo $A2"]);
    let b = workflow(&["run: echo $B1"]);
    let outputs = lint(
        &[("a.yaml", &a), ("b.yaml", &b)],
        tools_with_limit(command, Duration::from_secs(60), 1),
    );
    assert_eq!(outputs, ["", ""]);
    assert_eq!(processes(dir.path()), 3);
}

/// The real shellcheck, if it is on `PATH`, checks a batch as it checks each script alone.
#[test]
fn real_shellcheck_gives_a_batch_the_findings_of_each_script_alone() {
    let Ok(shellcheck) = ToolCommand::resolve("shellcheck") else {
        eprintln!("skipped: shellcheck is not installed");
        return;
    };
    let a = workflow(&[
        "run: echo ${{ github.sha }} $FOO",
        "run: printf '%s\\n'\t$BAR",
    ]);
    let b = workflow(&[
        "run: ls *.txt | grep x\n        shell: sh",
        "run: cd dir; echo $X",
    ]);
    let files = [("a.yaml", a.as_str()), ("b.yaml", b.as_str())];
    let alone = lint(
        &files[..1],
        tools(shellcheck.clone(), Duration::from_secs(60)),
    )
    .into_iter()
    .chain(lint(
        &files[1..],
        tools(shellcheck.clone(), Duration::from_secs(60)),
    ))
    .collect::<Vec<_>>();
    let batched = lint(
        &files,
        tools_with_limit(shellcheck, Duration::from_secs(60), 1),
    );
    assert_eq!(batched, alone);
    assert!(
        batched.iter().all(|output| output.contains("SC2086")),
        "{batched:?}"
    );
}
