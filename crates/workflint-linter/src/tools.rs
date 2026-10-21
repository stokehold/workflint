//! The external tools of a run, which check the scripts that rules collect (ADR 0012).
//!
//! [`Tools`] implements [`ExternalTools`] for the rules. It runs shellcheck, pyflakes, and ruff
//! through a [`ToolPool`], turns their output into findings, and turns a run that gives no
//! findings into the message of a `shellcheck-failed` or `pyflakes-failed` diagnostic. Each
//! script runs once per tool and run: the first file that needs it runs the tool, and every
//! other file with the same script gets the same outcome, also while the first run is still
//! going. [`python`] reads the output of pyflakes and ruff.

mod python;

use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::thread;

use serde::Deserialize;
use workflint_diagnostics::quote::quote;
use workflint_rules::PythonLinter;
use workflint_rules::tools::{
    ExternalTools, PythonOutcome, PythonTool, ScriptShell, ShellScript, ShellcheckFinding,
    ShellcheckOutcome,
};

use crate::tool::{RunOptions, ToolCommand, ToolOutcome, ToolPool};

/// The external tools of a run. [`Tools::none`] runs none.
#[derive(Debug, Default)]
pub struct Tools {
    shellcheck: Option<Runner<(ScriptShell, String), ShellcheckOutcome>>,
    pyflakes: Option<Runner<String, PythonOutcome>>,
    ruff: Option<Runner<String, PythonOutcome>>,
    /// Whether the warning that no Python linter is installed was logged.
    warned_python: AtomicBool,
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
            shellcheck: Some(Runner::new("shellcheck", command, pool, options)),
            ..self
        }
    }

    /// Runs pyflakes with `command` in `pool`, with `options` for each process. pyflakes prints
    /// some problems to stdout and others to stderr, so its output is always read as one stream.
    #[must_use]
    pub fn with_pyflakes(
        self,
        command: ToolCommand,
        pool: Arc<ToolPool>,
        mut options: RunOptions,
    ) -> Self {
        options.combine_output = true;
        Self {
            pyflakes: Some(Runner::new("pyflakes", command, pool, options)),
            ..self
        }
    }

    /// Runs ruff with `command` in `pool`, with `options` for each process.
    #[must_use]
    pub fn with_ruff(self, command: ToolCommand, pool: Arc<ToolPool>, options: RunOptions) -> Self {
        Self {
            ruff: Some(Runner::new("ruff", command, pool, options)),
            ..self
        }
    }

    /// The Python linter that runs for `linter`, or `None` if `linter` is `off` or names a tool
    /// that is not here. `auto` picks ruff, else pyflakes.
    #[must_use]
    pub fn python_tool(&self, linter: PythonLinter) -> Option<PythonTool> {
        let ruff = self.ruff.as_ref().map(|_| PythonTool::Ruff);
        let pyflakes = self.pyflakes.as_ref().map(|_| PythonTool::Pyflakes);
        match linter {
            PythonLinter::Auto => ruff.or(pyflakes),
            PythonLinter::Ruff => ruff,
            PythonLinter::Pyflakes => pyflakes,
            PythonLinter::Off => None,
        }
    }
}

impl ExternalTools for Tools {
    fn has_shellcheck(&self) -> bool {
        self.shellcheck.is_some()
    }

    fn shellcheck(&self, scripts: &[ShellScript]) -> Vec<Arc<ShellcheckOutcome>> {
        let Some(runner) = &self.shellcheck else {
            return Vec::new();
        };
        runner.check(
            scripts,
            |script| (script.shell, script.text.clone()),
            |script| {
                let args = script.shell.shellcheck_args();
                tracing::debug!(
                    "Running {} {} on a {} script",
                    runner.command.program().display(),
                    args.join(" "),
                    script.shell.name()
                );
                outcome(runner.run(&args[..], &script.text))
            },
            || ShellcheckOutcome::Failed("shellcheck was not run on this script".to_owned()),
        )
    }

    fn python(&self, linter: PythonLinter, scripts: &[String]) -> Vec<Arc<PythonOutcome>> {
        let (tool, runner) = match self.python_tool(linter) {
            Some(PythonTool::Ruff) => (PythonTool::Ruff, &self.ruff),
            Some(PythonTool::Pyflakes) => (PythonTool::Pyflakes, &self.pyflakes),
            None => {
                if linter == PythonLinter::Auto && !self.warned_python.swap(true, Ordering::Relaxed)
                {
                    tracing::warn!(
                        "neither ruff nor pyflakes was found on PATH, so the scripts of \
                         shell: python steps are not checked. Install ruff or pyflakes, or pass \
                         -pyflakes= to turn the check off"
                    );
                }
                return Vec::new();
            }
        };
        let Some(runner) = runner else {
            return Vec::new();
        };
        let args = python::args(tool);
        runner.check(
            scripts,
            Clone::clone,
            |script| {
                tracing::debug!(
                    "Running {} {} on a Python script",
                    runner.command.program().display(),
                    args.join(" ")
                );
                python::outcome(tool, runner.run(args, script))
            },
            || PythonOutcome::Failed(format!("{} was not run on this script", tool.name())),
        )
    }
}

/// The outcome of one script, which the first file that needs it fills.
type Cell<O> = Arc<OnceLock<Arc<O>>>;

/// One tool, and the outcome of each script it has been given in the run, by key.
struct Runner<K, O> {
    name: &'static str,
    command: ToolCommand,
    pool: Arc<ToolPool>,
    options: RunOptions,
    outcomes: Mutex<HashMap<K, Cell<O>>>,
}

impl<K, O> fmt::Debug for Runner<K, O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Runner")
            .field("name", &self.name)
            .field("command", &self.command)
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

impl<K: Eq + Hash, O: Send + Sync> Runner<K, O> {
    fn new(
        name: &'static str,
        command: ToolCommand,
        pool: Arc<ToolPool>,
        options: RunOptions,
    ) -> Self {
        Self {
            name,
            command,
            pool,
            options,
            outcomes: Mutex::new(HashMap::new()),
        }
    }

    /// Runs the tool with `args` and `stdin`.
    fn run<S: AsRef<std::ffi::OsStr>>(&self, args: &[S], stdin: &str) -> ToolOutcome {
        self.pool
            .run(&self.command, args, stdin.as_bytes(), &self.options)
    }

    /// Runs `run` on each of `scripts` whose `key` has no outcome yet, as many at once as the
    /// pool allows, and returns the outcome of each. `missing` is the outcome of a script whose
    /// run did not finish, which only a panic in another thread causes.
    fn check<T: Sync>(
        &self,
        scripts: &[T],
        key: impl Fn(&T) -> K,
        run: impl Fn(&T) -> O + Sync,
        missing: impl Fn() -> O,
    ) -> Vec<Arc<O>> {
        let cells: Vec<Cell<O>> = {
            // A panic while the lock is held leaves the map as it was, so it stays usable.
            let mut outcomes = self.outcomes.lock().unwrap_or_else(PoisonError::into_inner);
            scripts
                .iter()
                .map(|script| Arc::clone(outcomes.entry(key(script)).or_default()))
                .collect()
        };
        let next = AtomicUsize::new(0);
        let work = || {
            loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let (Some(script), Some(cell)) = (scripts.get(index), cells.get(index)) else {
                    break;
                };
                cell.get_or_init(|| Arc::new(run(script)));
            }
        };
        let workers = scripts.len().min(self.pool.limit().get());
        thread::scope(|scope| {
            for _ in 1..workers {
                // Without another thread, this one does all the work.
                let spawned = thread::Builder::new()
                    .name(format!("workflint-{}", self.name))
                    .spawn_scoped(scope, work);
                if spawned.is_err() {
                    break;
                }
            }
            work();
        });
        cells
            .iter()
            .map(|cell| cell.get().cloned().unwrap_or_else(|| Arc::new(missing())))
            .collect()
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
        outcome => ShellcheckOutcome::Failed(process_failure("shellcheck", outcome)),
    }
}

/// The message of a process of the tool `name` that gave no output to read: it failed without
/// output, was killed, timed out, could not start, or its pipes failed.
pub(crate) fn process_failure(name: &str, outcome: ToolOutcome) -> String {
    let stderr = |stderr: &[u8]| quote(&String::from_utf8_lossy(stderr));
    let detail = match outcome {
        ToolOutcome::Exited { code, .. } => format!("{name} exited with status {code}"),
        ToolOutcome::FailedWithoutOutput { code, stderr: err } => format!(
            "{name} exited with status {code} but stdout was empty. stderr: {}",
            stderr(&err)
        ),
        ToolOutcome::Signaled {
            signal: Some(signal),
            stderr: err,
        } => format!(
            "{name} was terminated by signal {signal}. stderr: {}",
            stderr(&err)
        ),
        ToolOutcome::Signaled {
            signal: None,
            stderr: err,
        } => format!("{name} was terminated. stderr: {}", stderr(&err)),
        ToolOutcome::TimedOut { timeout } => format!(
            "{name} did not finish in {} seconds and was killed",
            timeout.as_secs_f64()
        ),
        ToolOutcome::SpawnFailed(err) => format!("{name} could not be started: {err}"),
        ToolOutcome::IoFailed(err) => {
            format!("reading the output of {name} or writing the script to it failed: {err}")
        }
    };
    format!("{name} did not run successfully while checking this script: {detail}")
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
