#![allow(clippy::unwrap_used)]

use std::cell::RefCell;
use std::fmt::Write as _;
use std::sync::Arc;

use workflint_diagnostics::{Color, FileId, SourceFile, render_oneline, sort_and_dedup};

use super::*;
use crate::tools::ExternalTools;
use crate::visit;

/// Gives every script the same outcome and records the scripts it was given.
#[derive(Debug)]
struct Canned {
    outcome: Arc<ShellcheckOutcome>,
    scripts: RefCell<Vec<ShellScript>>,
}

impl Canned {
    fn findings(findings: Vec<ShellcheckFinding>) -> Self {
        Self::outcome(ShellcheckOutcome::Findings(findings))
    }

    fn outcome(outcome: ShellcheckOutcome) -> Self {
        Self {
            outcome: Arc::new(outcome),
            scripts: RefCell::new(Vec::new()),
        }
    }

    fn scripts(&self) -> Vec<(&'static str, String)> {
        self.scripts
            .borrow()
            .iter()
            .map(|script| (script.shell.name(), script.text.clone()))
            .collect()
    }
}

impl ExternalTools for Canned {
    fn has_shellcheck(&self) -> bool {
        true
    }

    fn shellcheck(&self, scripts: &[ShellScript]) -> Vec<Arc<ShellcheckOutcome>> {
        self.scripts.borrow_mut().extend_from_slice(scripts);
        scripts.iter().map(|_| Arc::clone(&self.outcome)).collect()
    }
}

/// SC2086 at `line` and `column` of the text on stdin, `length` characters long.
fn sc2086(line: i64, column: i64, length: i64) -> ShellcheckFinding {
    ShellcheckFinding {
        line,
        column,
        end_line: Some(line),
        end_column: Some(column + length),
        level: "info".to_owned(),
        code: 2086,
        message: "Double quote to prevent globbing and word splitting.".to_owned(),
    }
}

/// Lints `text` with only the `shellcheck` rule and `tools`, and returns its diagnostics in the
/// one-line format.
fn lint(text: &str, tools: &dyn ExternalTools) -> Vec<String> {
    let file = SourceFile::new(FileId::new(0), "test.yaml", text).unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    let parsed = workflint_syntax::parse(&file, &document);
    let mut rules: Vec<Box<dyn Rule<'_>>> = vec![Box::new(Shellcheck::new())];
    let mut cx = Context::new(&file).with_tools(tools);
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

/// A workflow with one job on `ubuntu-latest` whose one step is `step`, indented to sit in the
/// list of steps.
fn one_step(step: &str) -> String {
    format!("on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - {step}")
}

const SC2086: &str = "Double quote to prevent globbing and word splitting [shellcheck-issue]";

#[test]
fn a_finding_in_a_literal_block_points_at_its_line_and_column() {
    let tools = Canned::findings(vec![sc2086(3, 6, 4)]);
    let text = one_step("run: |\n          echo hello\n          echo $FOO\n");
    assert_eq!(
        lint(&text, &tools),
        [format!(
            "test.yaml:8:16: shellcheck reported issue in this script: SC2086:info:2:6: {SC2086}"
        )]
    );
}

#[test]
fn a_finding_in_a_folded_block_points_at_its_line_and_column() {
    // `>` folds the two lines into `echo hello echo $FOO\n`, so shellcheck sees one line.
    let tools = Canned::findings(vec![sc2086(2, 17, 4)]);
    let text = one_step("run: >\n          echo hello\n          echo $FOO\n");
    assert_eq!(
        lint(&text, &tools),
        [format!(
            "test.yaml:8:16: shellcheck reported issue in this script: SC2086:info:1:17: {SC2086}"
        )]
    );
}

#[test]
fn a_finding_in_a_double_quoted_scalar_points_past_the_escapes() {
    // The script is `echo "a<tab>b"; echo $FOO`. The escapes `\"` and `\t` take two bytes each in
    // the file and one character in the script, and shellcheck counts the tab to column 9.
    let tools = Canned::findings(vec![sc2086(2, 18, 4)]);
    let text = one_step("run: \"echo \\\"a\\tb\\\"; echo $FOO\"\n");
    assert_eq!(
        lint(&text, &tools),
        [format!(
            "test.yaml:6:35: shellcheck reported issue in this script: SC2086:info:1:18: {SC2086}"
        )]
    );
}

#[test]
fn a_finding_in_a_plain_scalar_over_two_lines_points_at_the_second_line() {
    // A plain scalar folds its lines too: the script is `echo hello $FOO`.
    let tools = Canned::findings(vec![sc2086(2, 12, 4)]);
    let text = one_step("run: echo hello\n          $FOO\n");
    assert_eq!(
        lint(&text, &tools),
        [format!(
            "test.yaml:7:11: shellcheck reported issue in this script: SC2086:info:1:12: {SC2086}"
        )]
    );
}

#[test]
fn a_finding_after_an_expression_on_the_same_line_points_at_its_place_in_the_file() {
    // `${{ 'é' }}` is 11 bytes and 10 characters, and becomes 11 `_`, so shellcheck counts one
    // column more after it than the file has.
    let tools = Canned::findings(vec![sc2086(3, 18, 4)]);
    let text = one_step("run: |\n          true\n          echo ${{ 'é' }} $FOO\n");
    assert_eq!(
        lint(&text, &tools),
        [format!(
            "test.yaml:8:27: shellcheck reported issue in this script: SC2086:info:2:18: {SC2086}"
        )]
    );
}

#[test]
fn a_column_after_a_tab_counts_the_tab_to_the_next_multiple_of_eight() {
    // shellcheck puts `$FOO` after `echo\t` at column 9.
    let tools = Canned::findings(vec![sc2086(2, 9, 4)]);
    let text = one_step("run: \"echo\\t$FOO\"\n");
    assert_eq!(
        lint(&text, &tools),
        [format!(
            "test.yaml:6:21: shellcheck reported issue in this script: SC2086:info:1:9: {SC2086}"
        )]
    );
}

#[test]
fn a_finding_on_the_added_first_line_points_at_the_run_key() {
    let tools = Canned::findings(vec![sc2086(1, 1, 3)]);
    let text = one_step("run: echo hello\n");
    assert_eq!(
        lint(&text, &tools),
        [format!(
            "test.yaml:6:9: shellcheck reported issue in this script: SC2086:info:0:1: {SC2086}"
        )]
    );
}

#[test]
fn a_finding_past_the_end_points_after_the_last_character() {
    let finding = ShellcheckFinding {
        line: 4,
        column: 1,
        end_line: None,
        end_column: None,
        level: "error".to_owned(),
        code: 1009,
        message: "The mentioned syntax error was in this if expression.".to_owned(),
    };
    let tools = Canned::findings(vec![finding]);
    let text = one_step("run: |\n          if true; then\n");
    assert_eq!(
        lint(&text, &tools),
        [
            "test.yaml:7:24: shellcheck reported issue in this script: SC1009:error:3:1: The \
          mentioned syntax error was in this if expression [shellcheck-issue]"
        ]
    );
}

#[test]
fn only_one_final_period_is_removed_from_the_message() {
    let mut finding = sc2086(2, 1, 1);
    "Wait...".clone_into(&mut finding.message);
    let tools = Canned::findings(vec![finding]);
    assert_eq!(
        lint(&one_step("run: x\n"), &tools),
        [
            "test.yaml:6:14: shellcheck reported issue in this script: SC2086:info:1:1: Wait.. \
          [shellcheck-issue]"
        ]
    );
}

#[test]
fn a_failure_is_reported_at_the_run_key() {
    let tools = Canned::outcome(ShellcheckOutcome::Failed("it broke".to_owned()));
    assert_eq!(
        lint(&one_step("run: echo hello\n"), &tools),
        ["test.yaml:6:9: it broke [shellcheck-failed]"]
    );
}

#[test]
fn without_shellcheck_no_script_is_collected() {
    let file = SourceFile::new(FileId::new(0), "test.yaml", one_step("run: echo $FOO\n")).unwrap();
    let document = workflint_yaml::parse(&file).document.unwrap();
    let parsed = workflint_syntax::parse(&file, &document);
    let mut rules: Vec<Box<dyn Rule<'_>>> = vec![Box::new(Shellcheck::new())];
    let mut cx = Context::new(&file);
    visit(&parsed.workflow, &mut rules, &mut cx);
    assert_eq!(cx.into_diagnostics(), []);
}

#[test]
fn expressions_become_underscores_and_the_shell_options_come_first() {
    let tools = Canned::findings(Vec::new());
    let text = "on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ matrix.os }}' ${{ x }}y ${{ open
      - run: |
          if ${{ a }}; then
            echo ok
          fi
        shell: sh
";
    assert_eq!(lint(text, &tools), Vec::<String>::new());
    assert_eq!(
        tools.scripts(),
        [
            (
                "bash",
                "set -eo pipefail\necho '________________' ________y ${{ open\n".to_owned()
            ),
            (
                "sh",
                "set -e\nif ________; then\n  echo ok\nfi\n\n".to_owned()
            ),
        ]
    );
}

#[test]
fn sanitizing_keeps_the_length_in_bytes() {
    assert_eq!(sanitize("a ${{ 'é' }} b"), "a ___________ b");
    assert_eq!(sanitize("${{}}"), "_____");
    assert_eq!(sanitize("${{ a }"), "${{ a }");
    assert_eq!(sanitize("no expression"), "no expression");
}

#[test]
fn the_shell_comes_from_the_step_then_the_job_then_the_workflow_then_the_runner() {
    let tools = Canned::findings(Vec::new());
    let text = "on: push
defaults:
  run:
    shell: pwsh
jobs:
  job-default:
    runs-on: ubuntu-latest
    defaults:
      run:
        shell: sh
    steps:
      - run: job default sh
      - run: step bash
        shell: bash
  workflow-default:
    runs-on: ubuntu-latest
    steps:
      - run: workflow default pwsh
      - run: step sh with options
        shell: sh -e {0}
";
    assert_eq!(lint(text, &tools), Vec::<String>::new());
    let scripts: Vec<_> = tools
        .scripts()
        .into_iter()
        .map(|(shell, text)| (shell, text.lines().nth(1).unwrap().to_owned()))
        .collect();
    assert_eq!(
        scripts,
        [
            ("sh", "job default sh".to_owned()),
            ("bash", "step bash".to_owned()),
            ("sh", "step sh with options".to_owned()),
        ]
    );
}

#[test]
fn a_windows_runner_means_pwsh_unless_a_shell_is_set() {
    let tools = Canned::findings(Vec::new());
    let text = "on: push
jobs:
  windows:
    runs-on: [self-hosted, Windows]
    steps:
      - run: pwsh by runner
      - run: bash by step
        shell: bash
  windows-latest:
    runs-on: windows-latest
    defaults:
      run:
        shell: bash
    steps:
      - run: bash by job
  after-windows:
    runs-on: ubuntu-latest
    steps:
      - run: bash by default
";
    assert_eq!(lint(text, &tools), Vec::<String>::new());
    let scripts: Vec<_> = tools
        .scripts()
        .into_iter()
        .map(|(_, text)| text.lines().nth(1).unwrap().to_owned())
        .collect();
    assert_eq!(scripts, ["bash by step", "bash by job", "bash by default"]);
}

#[test]
fn only_bash_and_sh_are_checked_case_sensitively() {
    let tools = Canned::findings(Vec::new());
    let mut text =
        String::from("on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n");
    for shell in [
        "Bash",
        "bash-x {0}",
        "python",
        "''",
        "{a: b}",
        "shx",
        "bash {0}",
    ] {
        write!(text, "      - run: {shell}\n        shell: {shell}\n").unwrap();
    }
    lint(&text, &tools);
    let shells: Vec<_> = tools
        .scripts()
        .into_iter()
        .map(|(shell, _)| shell)
        .collect();
    assert_eq!(shells, ["bash"]);
}

#[test]
fn every_script_of_the_file_goes_to_the_tools_in_one_call_in_source_order() {
    let tools = Canned::findings(vec![sc2086(2, 1, 1)]);
    let text = "on: push
jobs:
  b:
    runs-on: ubuntu-latest
    steps:
      - run: one
  a:
    runs-on: ubuntu-latest
    steps:
      - run: two
      - uses: actions/checkout@v4
      - run: three
";
    assert_eq!(lint(text, &tools).len(), 3);
    let scripts: Vec<_> = tools
        .scripts()
        .into_iter()
        .map(|(_, text)| text.lines().nth(1).unwrap().to_owned())
        .collect();
    assert_eq!(scripts, ["one", "two", "three"]);
}

#[test]
fn shell_scripts_lists_the_scripts_that_the_rule_gives_the_tools() {
    let text = "on: push
jobs:
  a:
    runs-on: ubuntu-latest
    steps:
      - run: echo ${{ github.sha }}
      - run: echo one
        shell: sh
      - run: Write-Host x
        shell: pwsh
  b:
    runs-on: ubuntu-latest
    steps:
      - run: echo two
";
    let canned = Canned::findings(Vec::new());
    lint(text, &canned);
    let file = SourceFile::new(FileId::new(0), "test.yaml", text).unwrap();
    let listed: Vec<(&'static str, String)> = crate::shell_scripts(&file)
        .iter()
        .map(|script| (script.shell.name(), script.text.clone()))
        .collect();
    assert_eq!(listed, canned.scripts());
    assert_eq!(listed.len(), 3);
    let broken = SourceFile::new(FileId::new(0), "test.yaml", "on: [").unwrap();
    assert_eq!(crate::shell_scripts(&broken), Vec::new());
}
