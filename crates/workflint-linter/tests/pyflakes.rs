//! pyflakes and ruff through the driver: the arguments and the script they get, which of them
//! `python.linter` picks, one run per script and run, failures, and the positions of findings.
//!
//! Most tests run `workflint-test-tool` in place of pyflakes and ruff, so they behave the same on
//! every OS. The last tests run the real pyflakes and ruff when they are on `PATH`.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Arc;

use tempfile::TempDir;
use workflint_diagnostics::{Color, render_oneline};
use workflint_linter::tool::{RunOptions, ToolCommand, ToolPool};
use workflint_linter::{Input, LintError, Linter, Tools};
use workflint_rules::PythonLinter;
use workflint_rules::tools::PythonTool;

const TEST_TOOL: &str = env!("CARGO_BIN_EXE_workflint-test-tool");

/// The test tool with `args` before the tool's arguments.
fn stand_in(args: &[&str]) -> ToolCommand {
    ToolCommand::new(TEST_TOOL, args.iter().copied())
}

fn pool() -> Arc<ToolPool> {
    Arc::new(ToolPool::new(NonZeroUsize::new(4).unwrap()).unwrap())
}

fn with_pyflakes(command: ToolCommand) -> Tools {
    Tools::none().with_pyflakes(command, pool(), RunOptions::default())
}

fn with_ruff(command: ToolCommand) -> Tools {
    Tools::none().with_ruff(command, pool(), RunOptions::default())
}

/// A workflow whose one job on `ubuntu-latest` runs Python by default and has these steps, each
/// indented to sit in the list.
fn workflow(steps: &[&str]) -> String {
    let mut text = String::from(
        "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    defaults:\n      run:\n        \
         shell: python\n    steps:\n",
    );
    for step in steps {
        text.push_str("      - ");
        text.push_str(step);
        text.push('\n');
    }
    text
}

/// Writes `files` into a new directory, lints them in order with `linter`, and returns the
/// one-line output of each.
fn lint_in(dir: &Path, files: &[(&str, &str)], linter: &Linter) -> Result<Vec<String>, LintError> {
    for (name, text) in files {
        let path = name
            .split('/')
            .fold(dir.to_path_buf(), |path, part| path.join(part));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    let inputs: Vec<Input> = files
        .iter()
        .filter(|(name, _)| !name.starts_with(".github/"))
        .map(|(name, _)| Input::file(name))
        .collect();
    Ok(linter
        .lint(&inputs)?
        .iter()
        .map(|linted| {
            let mut out = Vec::new();
            render_oneline(&mut out, &linted.file, &linted.diagnostics, Color::Off).unwrap();
            String::from_utf8(out).unwrap()
        })
        .collect())
}

fn lint(files: &[(&str, &str)], tools: Tools) -> Vec<String> {
    let dir = TempDir::new().unwrap();
    let linter = Linter::new(dir.path().to_path_buf()).with_tools(tools);
    lint_in(dir.path(), files, &linter).unwrap()
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
fn pyflakes_gets_no_arguments_and_the_script_with_expressions_replaced() {
    let dir = TempDir::new().unwrap();
    let record = stand_in(&["record", dir.path().to_str().unwrap()]);
    let text = workflow(&["run: print('${{ runner.os }}')", "run: print(1)"]);
    assert_eq!(lint(&[("a.yaml", &text)], with_pyflakes(record)), [""]);
    assert_eq!(
        records(dir.path()),
        ["--\nprint('________________')", "--\nprint(1)"]
    );
}

#[test]
fn ruff_runs_only_the_pyflakes_rules_without_config_and_prints_json() {
    let dir = TempDir::new().unwrap();
    let record = stand_in(&["record", dir.path().to_str().unwrap()]);
    let text = workflow(&["run: print(1)"]);
    assert_eq!(lint(&[("a.yaml", &text)], with_ruff(record)), [""]);
    assert_eq!(
        records(dir.path()),
        ["check\n--isolated\n--no-cache\n--select\nF\n--output-format\njson\n-\n--\nprint(1)"]
    );
}

#[test]
fn auto_picks_ruff_when_it_is_there_and_pyflakes_otherwise() {
    let ruff = TempDir::new().unwrap();
    let pyflakes = TempDir::new().unwrap();
    let both = Tools::none()
        .with_ruff(
            stand_in(&["record", ruff.path().to_str().unwrap()]),
            pool(),
            RunOptions::default(),
        )
        .with_pyflakes(
            stand_in(&["record", pyflakes.path().to_str().unwrap()]),
            pool(),
            RunOptions::default(),
        );
    assert_eq!(both.python_tool(PythonLinter::Auto), Some(PythonTool::Ruff));
    assert_eq!(
        both.python_tool(PythonLinter::Pyflakes),
        Some(PythonTool::Pyflakes)
    );
    assert_eq!(both.python_tool(PythonLinter::Off), None);
    lint(&[("a.yaml", &workflow(&["run: print(1)"]))], both);
    assert_eq!(records(ruff.path()).len(), 1);
    assert_eq!(records(pyflakes.path()).len(), 0);

    let only_pyflakes = with_pyflakes(stand_in(&["record", pyflakes.path().to_str().unwrap()]));
    assert_eq!(
        only_pyflakes.python_tool(PythonLinter::Auto),
        Some(PythonTool::Pyflakes)
    );
    assert_eq!(only_pyflakes.python_tool(PythonLinter::Ruff), None);
    lint(&[("a.yaml", &workflow(&["run: print(1)"]))], only_pyflakes);
    assert_eq!(records(pyflakes.path()).len(), 1);
    assert_eq!(Tools::none().python_tool(PythonLinter::Auto), None);
}

#[test]
fn python_linter_in_the_config_file_picks_the_linter_of_its_project() {
    let ruff = TempDir::new().unwrap();
    let pyflakes = TempDir::new().unwrap();
    let tools = || {
        Tools::none()
            .with_ruff(
                stand_in(&["record", ruff.path().to_str().unwrap()]),
                pool(),
                RunOptions::default(),
            )
            .with_pyflakes(
                stand_in(&["record", pyflakes.path().to_str().unwrap()]),
                pool(),
                RunOptions::default(),
            )
    };
    let project = |linter: &str| {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::create_dir_all(dir.path().join(".github/workflows")).unwrap();
        let config = format!("python:\n  linter: {linter}\n");
        let linter = Linter::new(dir.path().to_path_buf()).with_tools(tools());
        lint_in(
            dir.path(),
            &[
                (".github/workflint.yaml", &config),
                ("a.yaml", &workflow(&["run: print(1)"])),
            ],
            &linter,
        )
        .unwrap();
        (records(ruff.path()).len(), records(pyflakes.path()).len())
    };
    assert_eq!(project("pyflakes"), (0, 1));
    assert_eq!(project("ruff"), (1, 1));
    assert_eq!(project("auto"), (2, 1));
    assert_eq!(project("'off'"), (2, 1));
}

#[test]
fn the_pyflakes_flag_wins_over_python_linter() {
    let dir = TempDir::new().unwrap();
    let pyflakes = TempDir::new().unwrap();
    fs::create_dir(dir.path().join(".git")).unwrap();
    let tools = with_pyflakes(stand_in(&["record", pyflakes.path().to_str().unwrap()]));
    let linter = Linter::new(dir.path().to_path_buf())
        .with_tools(tools)
        .with_python_linter(PythonLinter::Pyflakes);
    let files = [
        (".github/workflint.yaml", "python:\n  linter: ruff\n"),
        ("a.yaml", &*workflow(&["run: print(1)"])),
    ];
    // The config file asks for ruff, which is not there, but `-pyflakes` picks pyflakes.
    lint_in(dir.path(), &files, &linter).unwrap();
    assert_eq!(records(pyflakes.path()).len(), 1);
}

#[test]
fn a_python_linter_that_the_config_file_names_and_that_is_missing_stops_the_run() {
    for (linter, tools) in [
        ("ruff", with_pyflakes(stand_in(&["args"]))),
        ("pyflakes", with_ruff(stand_in(&["args"]))),
    ] {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::create_dir_all(dir.path().join(".github/workflows")).unwrap();
        let config = format!("python:\n  linter: {linter}\n");
        let files = [
            (".github/workflint.yaml", config.as_str()),
            ("a.yaml", "on: push\n"),
        ];
        let err = lint_in(
            dir.path(),
            &files,
            &Linter::new(dir.path().to_path_buf()).with_tools(tools),
        )
        .unwrap_err();
        let path = dir.path().join(".github").join("workflint.yaml");
        assert_eq!(
            err.to_string(),
            format!(
                "python.linter in config file \"{}\" is \"{linter}\", but {linter} was not found \
                 on PATH",
                path.display().to_string().replace('\\', "\\\\")
            )
        );
    }
}

#[test]
fn identical_scripts_in_two_files_run_once() {
    let dir = TempDir::new().unwrap();
    let record = stand_in(&["record", dir.path().to_str().unwrap()]);
    let a = workflow(&["run: print(1)", "run: print(2)"]);
    let b = workflow(&["run: print(2)", "run: print(1)"]);
    let outputs = lint(&[("a.yaml", &a), ("b.yaml", &b)], with_pyflakes(record));
    assert_eq!(outputs, ["", ""]);
    assert_eq!(records(dir.path()), ["--\nprint(1)", "--\nprint(2)"]);
}

#[test]
fn pyflakes_findings_on_stdout_and_stderr_point_into_the_script() {
    let stdout = "<stdin>:2:7: undefined name 'hello'\n";
    let stderr = "<stdin>:3:6: '(' was never closed\nprint(\n     ^\n";
    let tools = with_pyflakes(stand_in(&["say", "1", stdout, stderr]));
    let text = workflow(&["run: |\n          import os\n          print(hello)\n          print("]);
    assert_eq!(
        lint(&[("a.yaml", &text)], tools),
        [
            "a.yaml:11:17: pyflakes reported issue in this script: 2:7: undefined name 'hello' \
          [pyflakes-issue]\n\
          a.yaml:12:16: pyflakes reported issue in this script: 3:6: '(' was never closed \
          [pyflakes-issue]\n"
        ]
    );
}

#[test]
fn ruff_findings_point_at_their_line_and_column_of_the_file() {
    let json = r#"[{"code":"F821","message":"Undefined name `hello`","location":{"row":2,"column":12},"end_location":{"row":2,"column":17}}]"#;
    let tools = with_ruff(stand_in(&["say", "1", json, ""]));
    let text = workflow(&["run: |\n          x = 1\n          print('é', hello)"]);
    assert_eq!(
        lint(&[("a.yaml", &text)], tools),
        [
            "a.yaml:11:22: pyflakes reported issue in this script: 2:12: F821 Undefined name \
          `hello` [pyflakes-issue]\n"
        ]
    );
}

#[test]
fn a_failing_linter_gives_pyflakes_failed_and_the_other_file_is_still_linted() {
    let tools = with_pyflakes(stand_in(&["say", "1", "<stdin>:1:1: no line break", ""]));
    let python = workflow(&["run: print(1)"]);
    let other = "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo\n        shell: bash\n";
    assert_eq!(
        lint(&[("a.yaml", &python), ("b.yaml", other)], tools),
        [
            "a.yaml:9:9: error message from pyflakes does not end with \\n nor \\r\\n while \
             checking this script. output: \"<stdin>:1:1: no line break\" [pyflakes-failed]\n",
            ""
        ]
    );
    let tools = with_ruff(stand_in(&["say", "2", "", "error: bad option"]));
    assert_eq!(
        lint(&[("a.yaml", &python)], tools),
        [
            "a.yaml:9:9: ruff did not run successfully while checking this script: ruff exited \
          with status 2 but stdout was empty. stderr: \"error: bad option\" [pyflakes-failed]\n"
        ]
    );
}

/// The executable `name` on `PATH`, if there is one.
fn on_path(name: &str) -> Option<ToolCommand> {
    ToolCommand::resolve(name).ok()
}

#[test]
fn the_real_pyflakes_reports_the_fixture_findings() {
    let Some(pyflakes) = on_path("pyflakes") else {
        eprintln!("skipped: pyflakes is not installed");
        return;
    };
    let text = workflow(&[
        "run: |\n          import sys\n          for sys in [1]:\n            print(sys, hello)",
    ]);
    assert_eq!(
        lint(&[("a.yaml", &text)], with_pyflakes(pyflakes)),
        [
            "a.yaml:11:15: pyflakes reported issue in this script: 2:5: import 'sys' from line 1 \
          shadowed by loop variable [pyflakes-issue]\n\
          a.yaml:12:24: pyflakes reported issue in this script: 3:14: undefined name 'hello' \
          [pyflakes-issue]\n"
        ]
    );
}

#[test]
fn the_real_ruff_points_at_the_right_line_and_column_of_the_file() {
    let Some(ruff) = on_path("ruff") else {
        eprintln!("skipped: ruff is not installed");
        return;
    };
    let text = workflow(&["run: |\n          x = 'é'\n          print(x, hello)"]);
    assert_eq!(
        lint(&[("a.yaml", &text)], with_ruff(ruff)),
        [
            "a.yaml:11:20: pyflakes reported issue in this script: 2:10: F821 Undefined name \
          `hello` [pyflakes-issue]\n"
        ]
    );
}
