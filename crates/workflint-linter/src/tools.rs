//! The external tools of a run, which check the scripts that rules collect (ADR 0012).
//!
//! [`Tools`] implements [`ExternalTools`] for the rules. It runs shellcheck through a
//! [`ToolPool`], turns its JSON output into findings, and turns a run that gives no findings into
//! the message of a `shellcheck-failed` diagnostic. Each script with its shell runs once per run:
//! the first file that needs it runs shellcheck, and every other file with the same script gets
//! the same outcome, also while the first run is still going.

use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::thread;

use serde::Deserialize;
use workflint_diagnostics::quote::quote;
use workflint_rules::tools::{
    ExternalTools, ScriptShell, ShellScript, ShellcheckFinding, ShellcheckOutcome,
};

use crate::tool::{RunOptions, ToolCommand, ToolOutcome, ToolPool};

/// The external tools of a run. [`Tools::none`] runs none.
#[derive(Debug, Default)]
pub struct Tools {
    shellcheck: Option<Shellcheck>,
}

impl Tools {
    /// No external tool.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// Runs shellcheck with `command` in `pool`, with `options` for each process.
    #[must_use]
    pub fn with_shellcheck(
        self,
        command: ToolCommand,
        pool: Arc<ToolPool>,
        options: RunOptions,
    ) -> Self {
        Self {
            shellcheck: Some(Shellcheck {
                command,
                pool,
                options,
                outcomes: Mutex::new(HashMap::new()),
            }),
        }
    }
}

impl ExternalTools for Tools {
    fn has_shellcheck(&self) -> bool {
        self.shellcheck.is_some()
    }

    fn shellcheck(&self, scripts: &[ShellScript]) -> Vec<Arc<ShellcheckOutcome>> {
        self.shellcheck
            .as_ref()
            .map_or_else(Vec::new, |shellcheck| shellcheck.check(scripts))
    }
}

/// The outcome of one script, which the first file that needs it fills.
type Cell = Arc<OnceLock<Arc<ShellcheckOutcome>>>;

/// shellcheck, and the outcome of each script it has been given in the run.
struct Shellcheck {
    command: ToolCommand,
    pool: Arc<ToolPool>,
    options: RunOptions,
    /// The outcome of each script, by shell and text. The arguments depend only on the shell.
    outcomes: Mutex<HashMap<(ScriptShell, String), Cell>>,
}

impl fmt::Debug for Shellcheck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Shellcheck")
            .field("command", &self.command)
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

impl Shellcheck {
    /// Runs shellcheck on each of `scripts` that has no outcome yet, as many at once as the
    /// pool allows, and returns the outcome of each.
    fn check(&self, scripts: &[ShellScript]) -> Vec<Arc<ShellcheckOutcome>> {
        let cells: Vec<Cell> = {
            // A panic while the lock is held leaves the map as it was, so it stays usable.
            let mut outcomes = self.outcomes.lock().unwrap_or_else(PoisonError::into_inner);
            scripts
                .iter()
                .map(|script| {
                    Arc::clone(
                        outcomes
                            .entry((script.shell, script.text.clone()))
                            .or_default(),
                    )
                })
                .collect()
        };
        let next = AtomicUsize::new(0);
        let work = || {
            loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let (Some(script), Some(cell)) = (scripts.get(index), cells.get(index)) else {
                    break;
                };
                cell.get_or_init(|| Arc::new(self.run(script)));
            }
        };
        let workers = scripts.len().min(self.pool.limit().get());
        thread::scope(|scope| {
            for _ in 1..workers {
                // Without another thread, this one does all the work.
                let spawned = thread::Builder::new()
                    .name("workflint-shellcheck".to_owned())
                    .spawn_scoped(scope, work);
                if spawned.is_err() {
                    break;
                }
            }
            work();
        });
        cells
            .iter()
            .map(|cell| {
                cell.get().cloned().unwrap_or_else(|| {
                    Arc::new(ShellcheckOutcome::Failed(
                        "shellcheck was not run on this script".to_owned(),
                    ))
                })
            })
            .collect()
    }

    /// Runs shellcheck on `script`.
    fn run(&self, script: &ShellScript) -> ShellcheckOutcome {
        let args = script.shell.shellcheck_args();
        tracing::debug!(
            "Running {} {} on a {} script",
            self.command.program().display(),
            args.join(" "),
            script.shell.name()
        );
        outcome(
            self.pool
                .run(&self.command, &args, script.text.as_bytes(), &self.options),
        )
    }
}

/// One finding in shellcheck's JSON output. A missing field is zero or empty, as Go's
/// `json.Unmarshal` leaves it in the reference.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonFinding {
    #[serde(default)]
    line: i64,
    #[serde(default)]
    column: i64,
    end_line: Option<i64>,
    end_column: Option<i64>,
    #[serde(default)]
    level: String,
    #[serde(default)]
    code: i64,
    #[serde(default)]
    message: String,
}

/// Turns the outcome of a shellcheck process into findings or the message of a failure.
pub(crate) fn outcome(outcome: ToolOutcome) -> ShellcheckOutcome {
    let failed = |detail: String| {
        ShellcheckOutcome::Failed(format!(
            "shellcheck did not run successfully while checking this script: {detail}"
        ))
    };
    let stderr = |stderr: &[u8]| quote(&String::from_utf8_lossy(stderr));
    match outcome {
        ToolOutcome::Exited { stdout, .. } => {
            // The reference's `json.Unmarshal` reads `null` as no findings.
            match serde_json::from_slice::<Option<Vec<JsonFinding>>>(&stdout) {
                Ok(findings) => ShellcheckOutcome::Findings(
                    findings
                        .unwrap_or_default()
                        .into_iter()
                        .map(|finding| ShellcheckFinding {
                            line: finding.line,
                            column: finding.column,
                            end_line: finding.end_line,
                            end_column: finding.end_column,
                            level: finding.level,
                            code: finding.code,
                            message: finding.message,
                        })
                        .collect(),
                ),
                Err(err) => ShellcheckOutcome::Failed(format!(
                    "could not parse JSON output from shellcheck: {err}: stdout={}",
                    quote(&String::from_utf8_lossy(&stdout))
                )),
            }
        }
        ToolOutcome::FailedWithoutOutput { code, stderr: err } => failed(format!(
            "shellcheck exited with status {code} but stdout was empty. stderr: {}",
            stderr(&err)
        )),
        ToolOutcome::Signaled {
            signal: Some(signal),
            stderr: err,
        } => failed(format!(
            "shellcheck was terminated by signal {signal}. stderr: {}",
            stderr(&err)
        )),
        ToolOutcome::Signaled {
            signal: None,
            stderr: err,
        } => failed(format!(
            "shellcheck was terminated. stderr: {}",
            stderr(&err)
        )),
        ToolOutcome::TimedOut { timeout } => failed(format!(
            "shellcheck did not finish in {} seconds and was killed",
            timeout.as_secs_f64()
        )),
        ToolOutcome::SpawnFailed(err) => failed(format!("shellcheck could not be started: {err}")),
        ToolOutcome::IoFailed(err) => failed(format!(
            "reading the output of shellcheck or writing the script to it failed: {err}"
        )),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use std::io;
    use std::time::Duration;

    use super::*;

    fn exited(stdout: &str) -> ToolOutcome {
        ToolOutcome::Exited {
            code: 1,
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        }
    }

    fn failure(outcome: ToolOutcome) -> String {
        match super::outcome(outcome) {
            ShellcheckOutcome::Failed(message) => message,
            ShellcheckOutcome::Findings(findings) => panic!("findings: {findings:?}"),
        }
    }

    #[test]
    fn the_json_output_becomes_findings() {
        let json = r#"[{"file":"-","line":2,"endLine":2,"column":6,"endColumn":10,"level":"info","code":2086,"message":"Double quote.","fix":null}]"#;
        assert_eq!(
            super::outcome(exited(json)),
            ShellcheckOutcome::Findings(vec![ShellcheckFinding {
                line: 2,
                column: 6,
                end_line: Some(2),
                end_column: Some(10),
                level: "info".to_owned(),
                code: 2086,
                message: "Double quote.".to_owned(),
            }])
        );
    }

    #[test]
    fn an_empty_array_or_null_is_no_finding() {
        for json in ["[]", "null", " [ ]\n"] {
            assert_eq!(
                super::outcome(exited(json)),
                ShellcheckOutcome::Findings(Vec::new()),
                "{json}"
            );
        }
    }

    #[test]
    fn missing_fields_are_zero_or_empty() {
        assert_eq!(
            super::outcome(exited("[{}]")),
            ShellcheckOutcome::Findings(vec![ShellcheckFinding {
                line: 0,
                column: 0,
                end_line: None,
                end_column: None,
                level: String::new(),
                code: 0,
                message: String::new(),
            }])
        );
    }

    #[test]
    fn output_that_is_not_json_is_a_failure_with_the_output() {
        assert_eq!(
            failure(exited("oops\n")),
            "could not parse JSON output from shellcheck: expected value at line 1 column 1: \
             stdout=\"oops\\n\""
        );
        assert!(failure(exited("")).starts_with("could not parse JSON output from shellcheck: "));
    }

    #[test]
    fn each_failure_of_the_process_has_its_message() {
        let prefix = "shellcheck did not run successfully while checking this script: ";
        let cases = [
            (
                ToolOutcome::FailedWithoutOutput {
                    code: 2,
                    stderr: b"bad\n".to_vec(),
                },
                "shellcheck exited with status 2 but stdout was empty. stderr: \"bad\\n\"",
            ),
            (
                ToolOutcome::Signaled {
                    signal: Some(9),
                    stderr: Vec::new(),
                },
                "shellcheck was terminated by signal 9. stderr: \"\"",
            ),
            (
                ToolOutcome::Signaled {
                    signal: None,
                    stderr: Vec::new(),
                },
                "shellcheck was terminated. stderr: \"\"",
            ),
            (
                ToolOutcome::TimedOut {
                    timeout: Duration::from_millis(1500),
                },
                "shellcheck did not finish in 1.5 seconds and was killed",
            ),
            (
                ToolOutcome::SpawnFailed(io::Error::other("no such file")),
                "shellcheck could not be started: no such file",
            ),
            (
                ToolOutcome::IoFailed(io::Error::other("broken pipe")),
                "reading the output of shellcheck or writing the script to it failed: broken pipe",
            ),
        ];
        for (outcome, detail) in cases {
            assert_eq!(failure(outcome), format!("{prefix}{detail}"));
        }
    }
}
