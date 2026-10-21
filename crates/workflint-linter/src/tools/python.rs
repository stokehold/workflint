//! The arguments of pyflakes and ruff, and how their output becomes findings.
//!
//! pyflakes reads the script on stdin without arguments, as in the reference
//! (`rule_pyflakes.go:126`), and prints each problem as `<stdin>:LINE:COL: MESSAGE`. Its output
//! is read as the reference reads it (`rule_pyflakes.go:144`). ruff runs only the pyflakes rules,
//! `F`, without reading any config file, and prints JSON.

use serde::Deserialize;
use workflint_diagnostics::quote::quote;
use workflint_rules::tools::{ColumnUnit, PythonFinding, PythonOutcome, PythonTool};

use super::process_failure;
use crate::tool::ToolOutcome;

/// The arguments of ruff: the pyflakes rules, no config file and no cache, JSON on stdout, and
/// the script on stdin.
const RUFF_ARGS: [&str; 8] = [
    "check",
    "--isolated",
    "--no-cache",
    "--select",
    "F",
    "--output-format",
    "json",
    "-",
];

/// The arguments of `tool`.
pub(super) fn args(tool: PythonTool) -> &'static [&'static str] {
    match tool {
        PythonTool::Pyflakes => &[],
        PythonTool::Ruff => &RUFF_ARGS,
    }
}

/// Turns the outcome of a process of `tool` into findings or the message of a failure.
pub(super) fn outcome(tool: PythonTool, outcome: ToolOutcome) -> PythonOutcome {
    match (tool, outcome) {
        (PythonTool::Pyflakes, ToolOutcome::Exited { code, stdout, .. }) => pyflakes(code, &stdout),
        (
            PythonTool::Ruff,
            ToolOutcome::Exited {
                code,
                stdout,
                stderr,
            },
        ) => ruff(code, &stdout, &stderr),
        (tool, outcome) => PythonOutcome::Failed(process_failure(tool.name(), outcome)),
    }
}

/// The marker before each problem in pyflakes' output.
const MARKER: &[u8] = b"<stdin>:";

/// Reads the output of pyflakes, stdout and stderr in the order it wrote them, which ended with
/// exit status `code`.
///
/// Each problem starts after `<stdin>:` and ends at the next line break. A syntax error is the
/// last problem, and pyflakes prints its line and a caret line after it, which have no
/// `<stdin>:`, so the reading stops there. Its column counts characters, where the column of any
/// other problem counts bytes.
fn pyflakes(code: i32, output: &[u8]) -> PythonOutcome {
    let mut findings = Vec::new();
    let mut rest = output;
    while !rest.is_empty() {
        let Some(start) = find(rest, MARKER) else {
            if let Some(last) = findings.last_mut() {
                let last: &mut PythonFinding = last;
                last.unit = ColumnUnit::Chars;
            }
            break;
        };
        let after = rest.get(start + MARKER.len()..).unwrap_or_default();
        let Some(end) = after.iter().position(|&byte| byte == b'\n') else {
            return PythonOutcome::Failed(format!(
                "error message from pyflakes does not end with \\n nor \\r\\n while checking this \
                 script. output: {}",
                quote(&String::from_utf8_lossy(output))
            ));
        };
        let line = after.get(..end).unwrap_or_default();
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        findings.push(pyflakes_finding(&String::from_utf8_lossy(line)));
        rest = after.get(end + 1..).unwrap_or_default();
    }
    if findings.is_empty() && code != 0 {
        return PythonOutcome::Failed(format!(
            "pyflakes did not run successfully while checking this script: pyflakes exited with \
             status {code} without reporting a problem. output: {}",
            quote(&String::from_utf8_lossy(output))
        ));
    }
    PythonOutcome::Findings(findings)
}

/// A problem of pyflakes: `text` is `LINE:COL: MESSAGE`, `LINE: MESSAGE`, or a message without
/// a position, which goes on line 0.
fn pyflakes_finding(text: &str) -> PythonFinding {
    let (line, rest) = leading_number(text);
    let column = line
        .and_then(|_| rest.strip_prefix(':'))
        .and_then(|rest| leading_number(rest).0);
    PythonFinding {
        line: line.unwrap_or(0),
        column,
        end: None,
        unit: ColumnUnit::Bytes,
        text: text.to_owned(),
    }
}

/// The number at the start of `text`, if it starts with a digit, and the text after it.
fn leading_number(text: &str) -> (Option<i64>, &str) {
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    let (number, rest) = text.split_at(digits);
    (number.parse().ok(), rest)
}

/// The index of the first `needle` in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// One finding in ruff's JSON output.
#[derive(Deserialize)]
struct RuffFinding {
    code: Option<String>,
    #[serde(default)]
    message: String,
    location: Option<RuffLocation>,
    end_location: Option<RuffLocation>,
}

/// A position in ruff's JSON output. Both count from 1, and the column counts characters.
#[derive(Deserialize, Clone, Copy)]
struct RuffLocation {
    row: i64,
    column: i64,
}

/// Reads the JSON that ruff printed to stdout, with exit status `code`. ruff exits with 1 when
/// it reports a problem and with 2 when it fails.
fn ruff(code: i32, stdout: &[u8], stderr: &[u8]) -> PythonOutcome {
    if code != 0 && code != 1 {
        return PythonOutcome::Failed(format!(
            "ruff did not run successfully while checking this script: ruff exited with status \
             {code}. stderr: {}",
            quote(&String::from_utf8_lossy(stderr))
        ));
    }
    match serde_json::from_slice::<Option<Vec<RuffFinding>>>(stdout) {
        Ok(findings) => PythonOutcome::Findings(
            findings
                .unwrap_or_default()
                .into_iter()
                .map(ruff_finding)
                .collect(),
        ),
        Err(err) => PythonOutcome::Failed(format!(
            "could not parse JSON output from ruff: {err}: stdout={}",
            quote(&String::from_utf8_lossy(stdout))
        )),
    }
}

/// The finding of `finding`, whose text is `LINE:COL: ` and the code and message as ruff's
/// `concise` output prints them: `F821 Undefined name` for a rule, and
/// `invalid-syntax: Expected ')'` for a syntax error.
fn ruff_finding(finding: RuffFinding) -> PythonFinding {
    let location = finding
        .location
        .unwrap_or(RuffLocation { row: 0, column: 0 });
    let message = match &finding.code {
        Some(code) if is_rule_code(code) => format!("{code} {}", finding.message),
        Some(code) => format!("{code}: {}", finding.message),
        None => finding.message,
    };
    PythonFinding {
        line: location.row,
        column: Some(location.column),
        end: finding.end_location.map(|end| (end.row, end.column)),
        unit: ColumnUnit::Chars,
        text: format!("{}:{}: {message}", location.row, location.column),
    }
}

/// Whether `code` is the code of a rule, such as `F821`: capital letters, then digits.
fn is_rule_code(code: &str) -> bool {
    let letters = code.bytes().take_while(u8::is_ascii_uppercase).count();
    let digits = code.get(letters..).unwrap_or_default();
    letters > 0 && !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use std::time::Duration;

    use super::*;

    fn exited(code: i32, stdout: &str) -> ToolOutcome {
        ToolOutcome::Exited {
            code,
            stdout: stdout.as_bytes().to_vec(),
            stderr: b"warning: something\n".to_vec(),
        }
    }

    fn findings(outcome: PythonOutcome) -> Vec<PythonFinding> {
        match outcome {
            PythonOutcome::Findings(findings) => findings,
            PythonOutcome::Failed(message) => panic!("failed: {message}"),
        }
    }

    fn failure(outcome: PythonOutcome) -> String {
        match outcome {
            PythonOutcome::Failed(message) => message,
            PythonOutcome::Findings(findings) => panic!("findings: {findings:?}"),
        }
    }

    fn finding(line: i64, column: Option<i64>, unit: ColumnUnit, text: &str) -> PythonFinding {
        PythonFinding {
            line,
            column,
            end: None,
            unit,
            text: text.to_owned(),
        }
    }

    #[test]
    fn each_pyflakes_line_is_a_finding_with_the_text_after_the_marker() {
        let output =
            "<stdin>:1:7: undefined name 'hello'\r\n<stdin>:3:1: 'os' imported but unused\n";
        assert_eq!(
            findings(outcome(PythonTool::Pyflakes, exited(1, output))),
            [
                finding(1, Some(7), ColumnUnit::Bytes, "1:7: undefined name 'hello'"),
                finding(
                    3,
                    Some(1),
                    ColumnUnit::Bytes,
                    "3:1: 'os' imported but unused"
                ),
            ]
        );
    }

    #[test]
    fn a_pyflakes_syntax_error_counts_characters_and_its_source_lines_are_skipped() {
        let output = "<stdin>:1:15: '(' was never closed\nx = \"é\"; print(\n              ^\n";
        assert_eq!(
            findings(outcome(PythonTool::Pyflakes, exited(1, output))),
            [finding(
                1,
                Some(15),
                ColumnUnit::Chars,
                "1:15: '(' was never closed"
            )]
        );
    }

    #[test]
    fn a_pyflakes_message_without_a_column_or_a_line_keeps_its_text() {
        let output = "<stdin>:2: invalid syntax\n<stdin>: problem decoding source\n";
        assert_eq!(
            findings(outcome(PythonTool::Pyflakes, exited(1, output))),
            [
                finding(2, None, ColumnUnit::Bytes, "2: invalid syntax"),
                finding(0, None, ColumnUnit::Bytes, " problem decoding source"),
            ]
        );
    }

    #[test]
    fn pyflakes_output_without_a_marker_and_status_0_is_no_finding() {
        for output in ["", "anything\n"] {
            assert_eq!(
                findings(outcome(PythonTool::Pyflakes, exited(0, output))),
                [],
                "{output:?}"
            );
        }
    }

    #[test]
    fn a_pyflakes_message_without_a_final_line_break_is_a_failure() {
        assert_eq!(
            failure(outcome(
                PythonTool::Pyflakes,
                exited(1, "<stdin>:1:1: oops")
            )),
            "error message from pyflakes does not end with \\n nor \\r\\n while checking this \
             script. output: \"<stdin>:1:1: oops\""
        );
    }

    #[test]
    fn pyflakes_failing_without_a_problem_is_a_failure_with_its_output() {
        assert_eq!(
            failure(outcome(
                PythonTool::Pyflakes,
                exited(1, "Traceback (most recent call last):\n")
            )),
            "pyflakes did not run successfully while checking this script: pyflakes exited with \
             status 1 without reporting a problem. output: \
             \"Traceback (most recent call last):\\n\""
        );
    }

    #[test]
    fn a_pyflakes_process_that_fails_names_pyflakes() {
        assert_eq!(
            failure(outcome(
                PythonTool::Pyflakes,
                ToolOutcome::TimedOut {
                    timeout: Duration::from_secs(60)
                }
            )),
            "pyflakes did not run successfully while checking this script: pyflakes did not \
             finish in 60 seconds and was killed"
        );
    }

    #[test]
    fn ruff_json_becomes_findings_in_the_concise_format() {
        let json = r#"[
            {"code":"F821","message":"Undefined name `hello`","location":{"row":1,"column":7},"end_location":{"row":1,"column":12},"name":"undefined-name"},
            {"code":"invalid-syntax","message":"Expected ')'","location":{"row":2,"column":1},"end_location":{"row":2,"column":1}},
            {"code":null,"message":"SyntaxError: bad","location":{"row":3,"column":2},"end_location":null}
        ]"#;
        let found = findings(outcome(PythonTool::Ruff, exited(1, json)));
        let texts: Vec<&str> = found.iter().map(|finding| finding.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "1:7: F821 Undefined name `hello`",
                "2:1: invalid-syntax: Expected ')'",
                "3:2: SyntaxError: bad",
            ]
        );
        assert_eq!(
            found[0],
            PythonFinding {
                line: 1,
                column: Some(7),
                end: Some((1, 12)),
                unit: ColumnUnit::Chars,
                text: "1:7: F821 Undefined name `hello`".to_owned(),
            }
        );
        assert_eq!(found[2].end, None);
    }

    #[test]
    fn ruff_without_findings_prints_an_empty_array() {
        assert_eq!(findings(outcome(PythonTool::Ruff, exited(0, "[]"))), []);
    }

    #[test]
    fn ruff_output_that_is_not_json_is_a_failure_with_the_output() {
        assert_eq!(
            failure(outcome(PythonTool::Ruff, exited(1, "oops\n"))),
            "could not parse JSON output from ruff: expected value at line 1 column 1: \
             stdout=\"oops\\n\""
        );
    }

    #[test]
    fn ruff_exiting_with_2_is_a_failure_with_its_stderr() {
        assert_eq!(
            failure(outcome(PythonTool::Ruff, exited(2, "[]"))),
            "ruff did not run successfully while checking this script: ruff exited with status \
             2. stderr: \"warning: something\\n\""
        );
    }

    #[test]
    fn rule_codes_are_letters_then_digits() {
        for (code, rule) in [
            ("F821", true),
            ("PLE0101", true),
            ("invalid-syntax", false),
            ("F", false),
            ("821", false),
        ] {
            assert_eq!(is_rule_code(code), rule, "{code}");
        }
    }
}
