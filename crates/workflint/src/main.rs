//! The `workflint` command-line tool.
//!
//! It parses the command line with [`flags`], picks the inputs: the files, directories, and
//! globs given as arguments, the workflow files of the project without arguments, or stdin for
//! a lone `-`. It lints them with [`Linter`], prints the diagnostics in the order of the inputs,
//! through the `-format` template of [`template`], or as SARIF with `-format sarif`, and picks
//! the exit code (`docs/cli.md`). The conformance suite runs it with the command line in
//! `tests/conformance/README.md`.

mod color;
mod flags;
mod log;
mod sarif;
mod template;
mod usage;
mod version;

use std::ffi::OsString;
use std::io::{self, BufWriter, ErrorKind, IsTerminal, Write};
use std::process::ExitCode;
use std::sync::Arc;

use workflint_diagnostics::quote::quote;
use workflint_diagnostics::{
    Color, Diagnostic, Severity, SourceFile, render_default, render_oneline,
};
use workflint_gotemplate::Value;
use workflint_linter::config::{Config, init_config};
use workflint_linter::tool::{ResolveError, RunOptions, ToolCommand, ToolPool};
use workflint_linter::{
    IgnorePattern, Input, Linter, PythonLinter, Tools, has_internal_error, path, registry,
};

use crate::flags::{Options, Parsed};
use crate::template::{Formatter, PrintError};

/// No file has a diagnostic.
const EXIT_NO_PROBLEM: u8 = 0;
/// At least one file has a diagnostic.
const EXIT_PROBLEM_FOUND: u8 = 1;
/// The command line is invalid.
const EXIT_INVALID_OPTION: u8 = 2;
/// A file, stdin, or a config file could not be read, the workflow files of a run without
/// arguments, of a directory argument, or of a glob argument could not be found, a config file, an `-ignore` pattern, or the `-format` template
/// is invalid, the shellcheck that `-shellcheck` names, the pyflakes that `-pyflakes` names, or
/// the Python linter that a config file names was not found, `-init-config` failed, the template
/// failed while it ran, the output could not be written, or the lint of a file hit a bug in
/// workflint and reported an `internal-error` diagnostic.
const EXIT_FAILURE: u8 = 3;

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let (options, paths) = match flags::parse(&args) {
        Ok(Parsed::Help) => {
            eprint!("{}", usage::text());
            return ExitCode::from(EXIT_NO_PROBLEM);
        }
        Ok(Parsed::Run(options, paths)) => (options, paths),
        Err(message) => {
            eprint!("{message}\n{}", usage::text());
            return ExitCode::from(EXIT_INVALID_OPTION);
        }
    };
    if options.version {
        let installed_from = std::env::var("WORKFLINT_INSTALLED_FROM").ok();
        print!("{}", version::lines(installed_from.as_deref()));
        return ExitCode::from(EXIT_NO_PROBLEM);
    }
    let quiet = options.quiet || options.silent;
    if !quiet {
        log::init(if options.debug {
            tracing::Level::DEBUG
        } else if options.verbose {
            tracing::Level::INFO
        } else {
            tracing::Level::WARN
        });
    }
    let ignore = match ignore_patterns(&options) {
        Ok(ignore) => ignore,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(EXIT_FAILURE);
        }
    };
    // As in actionlint, an invalid template stops the run before `-init-config`.
    let formatter = match formatter(&options) {
        Ok(formatter) => formatter,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(EXIT_FAILURE);
        }
    };
    let cwd = path::working_dir();
    if options.init_config {
        // As in actionlint, the arguments and the config file are not read.
        return init(&cwd);
    }
    let config_file = match config_file(&options) {
        Ok(config_file) => config_file,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(EXIT_FAILURE);
        }
    };
    let (tools, python_linter) = match tools(&options, quiet) {
        Ok(tools) => tools,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(EXIT_FAILURE);
        }
    };
    let stdout = io::stdout();
    let color = color::enabled(
        options.color,
        options.no_color,
        |name| std::env::var(name).ok(),
        stdout.is_terminal(),
    );
    let format = Format::new(&options, color, formatter.as_ref());
    let linter = linter(cwd, ignore, tools, python_linter, config_file, &options);
    if let Some(formatter) = &formatter {
        formatter.set_kinds(linter.kinds());
    }
    let inputs = match inputs(&linter, paths, &options.stdin_filename) {
        Ok(inputs) => inputs,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(EXIT_FAILURE);
        }
    };
    let mut err = io::stderr();
    let outcome = if color {
        // Makes the escape sequences work on the consoles of older Windows versions.
        let mut out = BufWriter::new(anstream::AutoStream::always(stdout.lock()));
        run(&linter, &inputs, format, &mut out, &mut err)
    } else {
        let mut out = BufWriter::new(stdout.lock());
        run(&linter, &inputs, format, &mut out, &mut err)
    };
    if let Some(counts) = &outcome.counts {
        tracing::info!("Found {} errors in {} files", counts.total(), inputs.len());
        if options.summary && !quiet {
            eprintln!("{}", counts.summary(inputs.len()));
        }
    }
    ExitCode::from(outcome.code)
}

/// Runs `-init-config` in the working directory `cwd`.
fn init(cwd: &std::path::Path) -> ExitCode {
    match init_config(cwd) {
        Ok(path) => {
            println!(
                "Config file was generated at {}",
                quote(&path.to_string_lossy())
            );
            ExitCode::from(EXIT_NO_PROBLEM)
        }
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(EXIT_FAILURE)
        }
    }
}

/// The `-format` value that picks SARIF output instead of a template.
const SARIF: &str = "sarif";

/// The tool that SARIF output names.
const SARIF_TOOL: sarif::Tool<'static> = sarif::Tool {
    name: "workflint",
    version: version::VERSION,
    information_uri: "https://github.com/stokehold/workflint",
};

/// Parses the template of `-format`, if it is given and is not the name of a format. The error
/// is the message to print.
fn formatter(options: &Options) -> Result<Option<Formatter>, String> {
    if options.format.is_empty() || options.format == SARIF {
        return Ok(None);
    }
    Formatter::new(&options.format).map(Some)
}

/// Reads the config file of `-config-file`, if it is given. The error is the message to print.
fn config_file(options: &Options) -> Result<Option<Config>, String> {
    if options.config_file.is_empty() {
        return Ok(None);
    }
    Config::read(std::path::Path::new(&options.config_file))
        .map(Some)
        .map_err(|err| err.to_string())
}

/// Returns the inputs of the run for the arguments `paths`, as actionlint picks them
/// (`command.go:102-110`): the workflow files of the project without arguments, stdin named
/// `stdin_filename` when `-` is the only argument, and else the files that the arguments name,
/// with directories and globs expanded (ADR 0036). The error is the message to print.
fn inputs(linter: &Linter, paths: &[OsString], stdin_filename: &str) -> Result<Vec<Input>, String> {
    match paths {
        [] => linter.discover().map_err(|err| err.to_string()),
        [only] if only == "-" => Input::stdin(stdin_filename, io::stdin().lock())
            .map(|input| vec![input])
            .map_err(|err| err.to_string()),
        _ => linter.expand(paths).map_err(|err| err.to_string()),
    }
}

/// The linter of the run, with the flags and the config file of `-config-file`.
fn linter(
    cwd: std::path::PathBuf,
    ignore: Vec<IgnorePattern>,
    tools: Tools,
    python_linter: Option<PythonLinter>,
    config_file: Option<Config>,
    options: &Options,
) -> Linter {
    let mut linter = Linter::new(cwd).with_ignore(ignore).with_tools(tools);
    if let Some(python_linter) = python_linter {
        linter = linter.with_python_linter(python_linter);
    }
    if let Some(config) = config_file {
        linter = linter.with_config_file(config);
    }
    if options.shellcheck_cache {
        linter = linter.with_shellcheck_cache();
    }
    linter
}

/// Compiles the `-ignore` patterns. The error is the message to print.
fn ignore_patterns(options: &Options) -> Result<Vec<IgnorePattern>, String> {
    options
        .ignore
        .iter()
        .map(|pattern| {
            IgnorePattern::new(pattern).map_err(|err| {
                format!(
                    "invalid regular expression for ignore pattern {}: {err}",
                    quote(pattern)
                )
            })
        })
        .collect()
}

/// The external tools of the run, and the Python linter that `-pyflakes` picks over the
/// `python.linter` of the config files.
///
/// shellcheck runs unless `-shellcheck=` turns it off. A shellcheck that `-shellcheck` names but
/// that is not found is an error, which is the message to print. When the flag is not given and
/// `shellcheck` is not on `PATH`, the run goes on without it, and a warning says so unless
/// `quiet` is set.
///
/// `-pyflakes=` turns the Python linter off, and `-pyflakes` with a value runs that pyflakes,
/// which must be found. Without the flag, the run has the `ruff` and the `pyflakes` that are on
/// `PATH`, and each project's `python.linter` picks one. The linter warns once if no linter is
/// found for a script that `auto` would check, and stops the run if a config file names a
/// linter that is not found.
fn tools(options: &Options, quiet: bool) -> Result<(Tools, Option<PythonLinter>), String> {
    let (shellcheck, not_found) = match shellcheck(options, quiet)? {
        Ok(command) => (Some(command), None),
        Err(not_found) => (None, not_found),
    };
    let (python_linter, pyflakes, ruff) = match options.pyflakes.as_deref() {
        Some("") => (Some(PythonLinter::Off), None, None),
        Some(spec) => {
            let command = ToolCommand::resolve(spec)
                .map_err(|err| format!("pyflakes given with -pyflakes was not found: {err}"))?;
            (Some(PythonLinter::Pyflakes), Some(command), None)
        }
        None => (
            None,
            found("pyflakes", ToolCommand::resolve("pyflakes")),
            found("ruff", ToolCommand::resolve("ruff")),
        ),
    };
    let mut tools = Tools::none();
    if let Some(reason) = not_found {
        tools = tools.with_shellcheck_not_found(reason);
    }
    if shellcheck.is_none() && pyflakes.is_none() && ruff.is_none() {
        return Ok((tools, python_linter));
    }
    let pool = match options.jobs {
        Some(jobs) => ToolPool::new(jobs),
        None => ToolPool::with_available_parallelism(),
    }
    .map_err(|err| format!("could not run the external tools: {err}"))?;
    let pool = Arc::new(pool);
    if let Some(command) = shellcheck {
        tools = tools.with_shellcheck(command, Arc::clone(&pool), RunOptions::default());
        if let Some(dir) = workflint_linter::default_cache_dir() {
            tools = tools.with_cache_dir(&dir);
        } else {
            tracing::debug!("No cache directory was found, so the shellcheck cache is not used");
        }
    }
    if let Some(command) = pyflakes {
        tools = tools.with_pyflakes(command, Arc::clone(&pool), RunOptions::default());
    }
    if let Some(command) = ruff {
        tools = tools.with_ruff(command, pool, RunOptions::default());
    }
    Ok((tools, python_linter))
}

/// The shellcheck of the run, as [`tools`] picks it, or why the default one was not found.
fn shellcheck(
    options: &Options,
    quiet: bool,
) -> Result<Result<ToolCommand, Option<String>>, String> {
    let spec = options.shellcheck.as_deref().unwrap_or("shellcheck");
    if spec.is_empty() {
        return Ok(Err(None));
    }
    match ToolCommand::resolve(spec) {
        Ok(command) => Ok(Ok(command)),
        Err(err) if options.shellcheck.is_some() => Err(format!(
            "shellcheck given with -shellcheck was not found: {err}"
        )),
        Err(err) => {
            tracing::info!("Rule \"shellcheck\" was disabled: {err}");
            if !quiet {
                eprintln!(
                    "warning: shellcheck was not found on PATH, so the scripts of run: steps are \
                     not checked. Install shellcheck, or pass -shellcheck= to turn the check off"
                );
            }
            Ok(Err(Some(err.to_string())))
        }
    }
}

/// The command of `resolved`, the lookup of the tool `name` on `PATH`, or `None` if it was not
/// found.
fn found(name: &str, resolved: Result<ToolCommand, ResolveError>) -> Option<ToolCommand> {
    resolved
        .inspect_err(|err| tracing::debug!("{name} is not used: {err}"))
        .ok()
}

/// The result of a run.
struct Outcome {
    code: u8,
    /// The number of diagnostics by severity, if the files were linted.
    counts: Option<Counts>,
}

/// The number of diagnostics of each severity.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Counts {
    errors: usize,
    warnings: usize,
    info: usize,
}

impl Counts {
    fn of(diagnostics: &[Diagnostic]) -> Self {
        let mut counts = Self::default();
        for diagnostic in diagnostics {
            match diagnostic.severity {
                Severity::Error => counts.errors += 1,
                Severity::Warning => counts.warnings += 1,
                Severity::Info => counts.info += 1,
            }
        }
        counts
    }

    fn add(self, other: Self) -> Self {
        Self {
            errors: self.errors + other.errors,
            warnings: self.warnings + other.warnings,
            info: self.info + other.info,
        }
    }

    fn total(self) -> usize {
        self.errors + self.warnings + self.info
    }

    /// The line of `-summary` for a run over `files` files.
    fn summary(self, files: usize) -> String {
        let plural = |count: usize, one: &str, many: &str| {
            format!("{count} {}", if count == 1 { one } else { many })
        };
        format!(
            "{} in {}: {}, {}, {}",
            plural(self.total(), "problem", "problems"),
            plural(files, "file", "files"),
            plural(self.errors, "error", "errors"),
            plural(self.warnings, "warning", "warnings"),
            plural(self.info, "info", "info"),
        )
    }
}

/// Lints `inputs` with `linter`, writes their diagnostics in `format` to `out` in the order of
/// `inputs`, and returns the exit code and the number of diagnostics. Fatal messages go to
/// `err`.
///
/// As in actionlint, a file or a config file that cannot be read stops the run before anything
/// is written. A `-format` template runs once, over the diagnostics of every file. When it fails,
/// its output up to the failing action is written, and the run exits with code 3
/// (`docs/differences.md#template-execution-errors`).
fn run(
    linter: &Linter,
    inputs: &[Input],
    format: Format<'_>,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Outcome {
    let files = match linter.lint_with(inputs, |file, diagnostics| {
        format.render(&file, &diagnostics)
    }) {
        Ok(files) => files,
        Err(lint_error) => {
            // Nothing is left to report a failed write of the message to.
            let _ = writeln!(err, "{lint_error}");
            return Outcome {
                code: EXIT_FAILURE,
                counts: None,
            };
        }
    };
    let counts = files.iter().fold(Counts::default(), |total, rendered| {
        total.add(rendered.counts)
    });
    let code = if files.iter().any(|rendered| rendered.internal_error) {
        EXIT_FAILURE
    } else if counts.total() > 0 {
        EXIT_PROBLEM_FOUND
    } else {
        EXIT_NO_PROBLEM
    };
    let written = match format {
        Format::Sarif => {
            let results: Vec<sarif::Results> = files
                .into_iter()
                .filter_map(|rendered| match rendered.output {
                    Output::Sarif(results) => Some(results),
                    Output::Text(_) | Output::Fields(_) => None,
                })
                .collect();
            sarif::write(out, &SARIF_TOOL, registry::rules(), &results).and_then(|()| out.flush())
        }
        Format::Template(formatter) => {
            let fields = files
                .into_iter()
                .flat_map(|rendered| match rendered.output {
                    Output::Fields(fields) => fields,
                    Output::Text(_) | Output::Sarif(_) => Vec::new(),
                })
                .collect();
            let printed = formatter.print(out, fields);
            let flushed = out.flush();
            match printed {
                Err(PrintError::Template(message)) => {
                    let _ = writeln!(err, "{message}");
                    return Outcome {
                        code: EXIT_FAILURE,
                        counts: Some(counts),
                    };
                }
                Err(PrintError::Write(write_error)) => Err(write_error),
                Ok(()) => flushed,
            }
        }
        _ => files
            .iter()
            .try_for_each(|rendered| match &rendered.output {
                Output::Text(Ok(output)) => out.write_all(output),
                Output::Text(Err(err)) => Err(io::Error::new(err.kind(), err.to_string())),
                Output::Fields(_) | Output::Sarif(_) => Ok(()),
            })
            .and_then(|()| out.flush()),
    };
    let code = match written {
        // A reader that stops early, such as `head`, is not an error. actionlint dies of SIGPIPE
        // there without a message.
        Err(write_error) if write_error.kind() == ErrorKind::BrokenPipe => code,
        Err(write_error) => {
            let _ = writeln!(err, "could not write the output: {write_error}");
            EXIT_FAILURE
        }
        Ok(()) => code,
    };
    Outcome {
        code,
        counts: Some(counts),
    }
}

/// The output of one file.
struct Rendered {
    /// What the file adds to the output.
    output: Output,
    /// The number of diagnostics by severity.
    counts: Counts,
    /// Whether the file has an `internal-error` diagnostic.
    internal_error: bool,
}

/// What one file adds to the output.
enum Output {
    /// The rendered diagnostics, or the error of rendering them.
    Text(io::Result<Vec<u8>>),
    /// The template values of the diagnostics, for the `-format` template.
    Fields(Vec<Value>),
    /// The SARIF results of the diagnostics, for `-format sarif`.
    Sarif(sarif::Results),
}

/// The output format.
#[derive(Clone, Copy)]
enum Format<'a> {
    /// The source snippet follows each header line.
    Default(Color),
    /// `-oneline`: the header line only.
    Oneline(Color),
    /// `-format`: the template, without color.
    Template(&'a Formatter),
    /// `-format sarif`: one SARIF log for every file.
    Sarif,
    /// `-silent`: nothing.
    Silent,
}

impl<'a> Format<'a> {
    /// The format of `options`. `-silent` wins over `-format`, which wins over `-oneline`, as in
    /// actionlint (`linter.go:405`).
    fn new(options: &Options, color: bool, formatter: Option<&'a Formatter>) -> Self {
        let color = if color { Color::On } else { Color::Off };
        if options.silent {
            Self::Silent
        } else if options.format == SARIF {
            Self::Sarif
        } else if let Some(formatter) = formatter {
            Self::Template(formatter)
        } else if options.oneline {
            Self::Oneline(color)
        } else {
            Self::Default(color)
        }
    }

    fn render(self, file: &SourceFile, diagnostics: &[Diagnostic]) -> Rendered {
        let mut output = Vec::new();
        let written = match self {
            Self::Default(color) => render_default(&mut output, file, diagnostics, color),
            Self::Oneline(color) => render_oneline(&mut output, file, diagnostics, color),
            Self::Template(_) => {
                return Rendered {
                    output: Output::Fields(template::fields(file, diagnostics)),
                    counts: Counts::of(diagnostics),
                    internal_error: has_internal_error(diagnostics),
                };
            }
            Self::Sarif => {
                return Rendered {
                    output: Output::Sarif(sarif::Results::of(file, diagnostics)),
                    counts: Counts::of(diagnostics),
                    internal_error: has_internal_error(diagnostics),
                };
            }
            Self::Silent => Ok(()),
        };
        Rendered {
            // The output waits for the other files, so it keeps no spare capacity.
            output: Output::Text(written.map(|()| {
                output.shrink_to_fit();
                output
            })),
            counts: Counts::of(diagnostics),
            internal_error: has_internal_error(diagnostics),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    /// The rules, except that the lint of a file named `panics.yaml` panics.
    fn check_or_panic(
        file: &SourceFile,
        local: &dyn workflint_rules::local::LocalMetadata,
        tools: &dyn workflint_rules::tools::ExternalTools,
        config: &workflint_rules::RuleConfig,
    ) -> Vec<Diagnostic> {
        assert!(!file.path().ends_with("panics.yaml"), "a rule failed");
        workflint_rules::check_file(file, local, tools, config)
    }

    /// Runs `linter` on `paths` in `dir` with one-line output, and returns the exit code and
    /// what was written to stdout and stderr.
    fn run_oneline(linter: &Linter, paths: &[&str]) -> (u8, String, String) {
        let inputs: Vec<Input> = paths.iter().map(Input::file).collect();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let outcome = run(
            linter,
            &inputs,
            Format::Oneline(Color::Off),
            &mut out,
            &mut err,
        );
        (
            outcome.code,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    #[test]
    fn a_panic_in_one_file_prints_every_file_and_exits_with_3() {
        let dir = tempfile::tempdir().unwrap();
        let no_steps = "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n";
        for name in ["a.yaml", "panics.yaml", "z.yaml"] {
            std::fs::write(dir.path().join(name), no_steps).unwrap();
        }
        let paths = ["z.yaml", "panics.yaml", "a.yaml"];
        let linter = Linter::new(dir.path().to_path_buf());
        let (code, expected, _) = run_oneline(&linter, &paths);
        assert_eq!(code, EXIT_PROBLEM_FOUND);

        let (code, out, err) = run_oneline(&linter.with_check(check_or_panic), &paths);
        assert_eq!(code, EXIT_FAILURE);
        assert_eq!(err, "");
        let expected: Vec<&str> = expected.lines().collect();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(
            lines,
            [
                expected[0],
                "panics.yaml:1:1: workflint panicked while linting this file: a rule failed. \
                 This is a bug in workflint. Please report it at \
                 https://github.com/stokehold/workflint/issues [internal-error]",
                expected[2],
            ]
        );
    }

    #[test]
    fn an_unreadable_file_prints_nothing_and_exits_with_3() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.yaml"), "on: push\n").unwrap();
        let linter = Linter::new(dir.path().to_path_buf());
        let (code, out, err) = run_oneline(&linter, &["a.yaml", "missing.yaml"]);
        assert_eq!(code, EXIT_FAILURE);
        assert_eq!(out, "");
        assert!(
            err.starts_with("could not read \"missing.yaml\": "),
            "{err}"
        );
    }
}
