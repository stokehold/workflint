//! shellcheck through the driver: the arguments and the script it gets, one run per script and
//! run, a diagnostic for each way it can fail, and the positions of its findings.
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
    let pool = ToolPool::new(NonZeroUsize::new(4).unwrap()).unwrap();
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
