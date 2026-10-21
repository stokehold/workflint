#![allow(clippy::unwrap_used)]

use std::cell::RefCell;
use std::sync::Arc;

use workflint_diagnostics::{Color, FileId, SourceFile, render_oneline, sort_and_dedup};

use super::*;
use crate::RuleConfig;
use crate::tools::{ExternalTools, ShellScript, ShellcheckOutcome};
use crate::visit;

/// Gives every script the same outcome and records the linter and the scripts it was given.
#[derive(Debug)]
struct Canned {
    outcome: Arc<PythonOutcome>,
    calls: RefCell<Vec<(PythonLinter, Vec<String>)>>,
}

impl Canned {
    fn findings(findings: Vec<PythonFinding>) -> Self {
        Self::outcome(PythonOutcome::Findings(findings))
    }

    fn outcome(outcome: PythonOutcome) -> Self {
        Self {
            outcome: Arc::new(outcome),
            calls: RefCell::new(Vec::new()),
        }
    }

    /// The scripts of every call, in order.
    fn scripts(&self) -> Vec<String> {
        self.calls
            .borrow()
            .iter()
            .flat_map(|(_, scripts)| scripts.clone())
            .collect()
    }
}

impl ExternalTools for Canned {
    fn has_shellcheck(&self) -> bool {
        false
    }

    fn shellcheck(&self, _scripts: &[ShellScript]) -> Vec<Arc<ShellcheckOutcome>> {
        Vec::new()
    }

    fn python(&self, linter: PythonLinter, scripts: &[String]) -> Vec<Arc<PythonOutcome>> {
        self.calls.borrow_mut().push((linter, scripts.to_vec()));
        scripts.iter().map(|_| Arc::clone(&self.outcome)).collect()
    }
}

/// A finding of pyflakes at `line` and `column`, in bytes, with the text pyflakes prints.
fn pyflakes(line: i64, column: i64, message: &str) -> PythonFinding {
    PythonFinding {
        line,
        column: Some(column),
        end: None,
        unit: ColumnUnit::Bytes,
        text: format!("{line}:{column}: {message}"),
    }
}

/// Lints `text` with only the `pyflakes` rule, `tools`, and `linter` as `python.linter`, and
/// returns its diagnostics in the one-line format.
fn lint_with(text: &str, tools: &dyn ExternalTools, linter: PythonLinter) -> Vec<String> {
    let file = SourceFile::new(FileId::new(0), "test.yaml", text).unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    let parsed = workflint_syntax::parse(&file, &document);
    let config = RuleConfig {
        python_linter: linter,
        ..RuleConfig::DEFAULT
    };
    let mut rules: Vec<Box<dyn Rule<'_>>> = vec![Box::new(Pyflakes::new())];
    let mut cx = Context::with_config(&file, &crate::local::NoProject, &config).with_tools(tools);
    visit(&parsed.workflow, &mut rules, &mut cx);
    let mut diagnostics = cx.into_diagnostics();
    sort_and_dedup(&file, &mut diagnostics);
    let mut out = Vec::new();
    render_oneline(&mut out, &file, &diagnostics, Color::Off).unwrap();
    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn lint(text: &str, tools: &dyn ExternalTools) -> Vec<String> {
    lint_with(text, tools, PythonLinter::Auto)
}

/// A workflow with one job on `ubuntu-latest` whose one step is `step`, indented to sit in the
/// list of steps.
fn one_step(step: &str) -> String {
    format!("on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - {step}")
}

const HELLO: &str = "undefined name 'hello'";

#[test]
fn python_steps_are_found_by_step_job_and_workflow_shell_in_that_order() {
    let text = "on: push
defaults:
  run:
    shell: python
jobs:
  workflow-default:
    runs-on: ubuntu-latest
    steps:
      - run: print(1)
      - run: print(2)
        shell: bash
  job-default:
    runs-on: ubuntu-latest
    defaults:
      run:
        shell: bash
    steps:
      - run: print(3)
      - run: print(4)
        shell: python {0}
  job-without-shell:
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: src
    steps:
      - run: print(5)
";
    let tools = Canned::findings(Vec::new());
    assert_eq!(lint(text, &tools), Vec::<String>::new());
    assert_eq!(tools.scripts(), ["print(1)", "print(4)", "print(5)"]);
}

#[test]
fn without_a_python_shell_no_step_is_python_and_the_runner_does_not_matter() {
    let text = "on: push
jobs:
  windows:
    runs-on: windows-latest
    steps:
      - run: print(1)
      - run: print(2)
        shell: Python
      - run: print(3)
        shell: python3
      - run: print(4)
        shell: ${{ matrix.shell }}
";
    let tools = Canned::findings(Vec::new());
    assert_eq!(lint(text, &tools), Vec::<String>::new());
    assert!(tools.calls.borrow().is_empty());
}

#[test]
fn the_job_default_resets_after_the_job() {
    let text = "on: push
jobs:
  a:
    runs-on: ubuntu-latest
    defaults:
      run:
        shell: python
    steps:
      - run: print(1)
  b:
    runs-on: ubuntu-latest
    steps:
      - run: print(2)
";
    let tools = Canned::findings(Vec::new());
    lint(text, &tools);
    assert_eq!(tools.scripts(), ["print(1)"]);
}

#[test]
fn expressions_are_replaced_and_no_line_is_added() {
    let tools = Canned::findings(Vec::new());
    lint(
        &one_step("run: |\n          print('${{ runner.os }}')\n        shell: python\n"),
        &tools,
    );
    assert_eq!(tools.scripts(), ["print('________________')\n"]);
}

#[test]
fn every_script_of_the_file_goes_in_one_call_with_the_configured_linter() {
    let text = one_step(
        "run: print(1)\n        shell: python\n      - run: print(2)\n        shell: python\n",
    );
    for linter in [
        PythonLinter::Auto,
        PythonLinter::Ruff,
        PythonLinter::Pyflakes,
    ] {
        let tools = Canned::findings(Vec::new());
        lint_with(&text, &tools, linter);
        assert_eq!(
            *tools.calls.borrow(),
            [(linter, vec!["print(1)".to_owned(), "print(2)".to_owned()])]
        );
    }
}

#[test]
fn python_linter_off_collects_no_script() {
    let tools = Canned::findings(vec![pyflakes(1, 7, HELLO)]);
    let text = one_step("run: print(hello)\n        shell: python\n");
    assert_eq!(
        lint_with(&text, &tools, PythonLinter::Off),
        Vec::<String>::new()
    );
    assert!(tools.calls.borrow().is_empty());
}

#[test]
fn a_finding_points_at_its_line_and_column_in_a_literal_block() {
    let tools = Canned::findings(vec![pyflakes(2, 7, HELLO)]);
    let text =
        one_step("run: |\n          import os\n          print(hello)\n        shell: python\n");
    assert_eq!(
        lint(&text, &tools),
        [format!(
            "test.yaml:8:17: pyflakes reported issue in this script: 2:7: {HELLO} [pyflakes-issue]"
        )]
    );
}

#[test]
fn a_pyflakes_column_counts_bytes_and_a_ruff_column_counts_characters() {
    // `é` is two bytes. `hello` starts at byte 16 and at character 15, from 0.
    let text = one_step("run: x = 'é'; print(hello)\n        shell: python\n");
    let expected = |message: &str| {
        format!(
            "test.yaml:6:29: pyflakes reported issue in this script: {message} [pyflakes-issue]"
        )
    };
    let tools = Canned::findings(vec![pyflakes(1, 17, HELLO)]);
    assert_eq!(lint(&text, &tools), [expected(&format!("1:17: {HELLO}"))]);
    let tools = Canned::findings(vec![PythonFinding {
        line: 1,
        column: Some(16),
        end: Some((1, 21)),
        unit: ColumnUnit::Chars,
        text: "1:16: F821 Undefined name `hello`".to_owned(),
    }]);
    assert_eq!(
        lint(&text, &tools),
        [expected("1:16: F821 Undefined name `hello`")]
    );
}

#[test]
fn a_finding_without_a_column_points_at_the_start_of_its_line() {
    let tools = Canned::findings(vec![PythonFinding {
        line: 2,
        column: None,
        end: None,
        unit: ColumnUnit::Bytes,
        text: "2: invalid syntax".to_owned(),
    }]);
    let text = one_step("run: |\n          a = 1\n          b = = 2\n        shell: python\n");
    assert_eq!(
        lint(&text, &tools),
        [
            "test.yaml:8:11: pyflakes reported issue in this script: 2: invalid syntax [pyflakes-issue]"
        ]
    );
}

#[test]
fn a_finding_past_the_end_points_at_the_end_of_the_script() {
    // ruff reports the end of `print(` on the line after it.
    let tools = Canned::findings(vec![PythonFinding {
        line: 2,
        column: Some(1),
        end: Some((2, 1)),
        unit: ColumnUnit::Chars,
        text: "2:1: invalid-syntax: unexpected EOF while parsing".to_owned(),
    }]);
    let text = one_step("run: print(\n        shell: python\n");
    assert_eq!(
        lint(&text, &tools),
        [
            "test.yaml:6:20: pyflakes reported issue in this script: 2:1: invalid-syntax: unexpected EOF while parsing [pyflakes-issue]"
        ]
    );
}

#[test]
fn a_finding_on_line_zero_points_at_the_run_key() {
    let tools = Canned::findings(vec![pyflakes(0, 1, "problem decoding source")]);
    let text = one_step("run: print(1)\n        shell: python\n");
    assert_eq!(
        lint(&text, &tools),
        [
            "test.yaml:6:9: pyflakes reported issue in this script: 0:1: problem decoding source [pyflakes-issue]"
        ]
    );
}

#[test]
fn a_failure_is_reported_at_the_run_key() {
    let tools = Canned::outcome(PythonOutcome::Failed("pyflakes was killed".to_owned()));
    let text = one_step("run: print(1)\n        shell: python\n");
    assert_eq!(
        lint(&text, &tools),
        ["test.yaml:6:9: pyflakes was killed [pyflakes-failed]"]
    );
}

#[test]
fn without_tools_nothing_is_reported() {
    let text = one_step("run: print(hello)\n        shell: python\n");
    assert_eq!(lint(&text, &crate::tools::NoTools), Vec::<String>::new());
}
