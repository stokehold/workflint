//! The interface through which rules have external tools check scripts (ADR 0012).
//!
//! Rules spawn no processes (ADR 0006). The `shellcheck` rule collects the scripts of a file and
//! passes them to the [`ExternalTools`] that the [`Context`](crate::Context) holds. The linter
//! implements it: it runs the tool on each script and returns the findings, and the rule maps
//! them back into the workflow through the source map of the `run:` value. [`NoTools`] runs
//! nothing, so the rule reports nothing.

use std::fmt;
use std::sync::Arc;

/// The shell that shellcheck checks a script for, the value of its `--shell` argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScriptShell {
    /// `bash`.
    Bash,
    /// `sh`.
    Sh,
}

impl ScriptShell {
    /// The name that shellcheck's `--shell` takes.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Sh => "sh",
        }
    }

    /// The arguments of shellcheck for a script of this shell, the reference's
    /// (`rule_shellcheck.go:193`). The script comes on stdin. The shellcheck rules that `-e`
    /// turns off report scripts that work once `${{ }}` is replaced by `_`, or that read the
    /// variables of `env:` (`rule_shellcheck.go:176`).
    #[must_use]
    pub const fn shellcheck_args(self) -> [&'static str; 9] {
        [
            "--norc",
            "-f",
            "json",
            "-x",
            "--shell",
            self.name(),
            "-e",
            "SC1091,SC2194,SC2050,SC2153,SC2154,SC2157,SC2043",
            "-",
        ]
    }
}

/// A script for shellcheck: its shell and the text to pass on stdin, which is the `run:` value
/// after the reference's preprocessing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShellScript {
    /// The shell.
    pub shell: ScriptShell,
    /// The text for stdin.
    pub text: String,
}

/// One finding of shellcheck, from its JSON output. Lines and columns count in the text on
/// stdin, from 1. A column counts code points, and a tab moves it to the next multiple of 8,
/// plus 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellcheckFinding {
    /// `line`.
    pub line: i64,
    /// `column`.
    pub column: i64,
    /// `endLine`, if the output has it.
    pub end_line: Option<i64>,
    /// `endColumn`, which is just after the end of the finding, if the output has it.
    pub end_column: Option<i64>,
    /// `level`, such as `info` or `error`.
    pub level: String,
    /// `code`, the number after `SC`.
    pub code: i64,
    /// `message`.
    pub message: String,
}

/// What shellcheck gave for one script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellcheckOutcome {
    /// shellcheck ran and printed these findings.
    Findings(Vec<ShellcheckFinding>),
    /// shellcheck did not run, did not finish, or printed output that is not its JSON. The
    /// message says what happened.
    Failed(String),
}

/// The external tools that check the scripts of a workflow.
pub trait ExternalTools: fmt::Debug {
    /// Whether shellcheck runs. When it does not, the `shellcheck` rule collects no script.
    fn has_shellcheck(&self) -> bool;

    /// Runs shellcheck on each of `scripts` and returns one outcome per script, in the order of
    /// `scripts`. Returns an empty list when shellcheck does not run.
    fn shellcheck(&self, scripts: &[ShellScript]) -> Vec<Arc<ShellcheckOutcome>>;
}

/// No external tool: shellcheck does not run.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoTools;

impl ExternalTools for NoTools {
    fn has_shellcheck(&self) -> bool {
        false
    }

    fn shellcheck(&self, _scripts: &[ShellScript]) -> Vec<Arc<ShellcheckOutcome>> {
        Vec::new()
    }
}
