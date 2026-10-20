//! The `workflint` command-line tool.
//!
//! This is the minimal binary that the conformance suite runs, with the command line in
//! `tests/conformance/README.md`. It parses the flags, lints the files given as arguments with
//! [`Linter`], prints the diagnostics in the order of the arguments, and picks the exit code.
//! M5.6 replaces it with the full command-line interface.

use std::ffi::OsString;
use std::io::{self, BufWriter, ErrorKind, Write};
use std::process::ExitCode;

use workflint_diagnostics::quote::quote;
use workflint_diagnostics::{Color, Diagnostic, SourceFile, render_default, render_oneline};
use workflint_linter::{Input, Linter, has_internal_error, path};

/// No file has a diagnostic.
const EXIT_NO_PROBLEM: u8 = 0;
/// At least one file has a diagnostic.
const EXIT_PROBLEM_FOUND: u8 = 1;
/// The command line is invalid.
const EXIT_INVALID_OPTION: u8 = 2;
/// A file could not be read, the output could not be written, or the lint of a file hit a bug
/// in workflint and reported an `internal-error` diagnostic.
const EXIT_FAILURE: u8 = 3;

const USAGE: &str = "Usage: workflint [-oneline] [-no-color] [-shellcheck=PATH] [-pyflakes=PATH] \
                     [-config-file PATH] FILES...\n\
                     Reading a workflow from stdin with `-` is not supported yet.";

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let (oneline, paths) = match parse_args(&args) {
        Ok((_, [])) => {
            eprintln!(
                "no files given: this version of workflint lints only the files it is given\n{USAGE}"
            );
            return ExitCode::from(EXIT_INVALID_OPTION);
        }
        // actionlint reads stdin when `-` is the only argument.
        Ok((_, [only])) if only == "-" => {
            eprintln!("this version of workflint cannot read stdin\n{USAGE}");
            return ExitCode::from(EXIT_INVALID_OPTION);
        }
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("{message}\n{USAGE}");
            return ExitCode::from(EXIT_INVALID_OPTION);
        }
    };
    let format = if oneline {
        Format::Oneline
    } else {
        Format::Default
    };
    let inputs: Vec<Input> = paths.iter().map(Input::file).collect();
    let linter = Linter::new(path::working_dir());
    let mut out = BufWriter::new(io::stdout().lock());
    ExitCode::from(run(&linter, &inputs, format, &mut out, &mut io::stderr()))
}

/// Lints `inputs` with `linter`, writes their diagnostics in `format` to `out` in the order of
/// `inputs`, and returns the exit code. Fatal messages go to `err`.
///
/// As in actionlint, a file that cannot be read stops the run before anything is written.
fn run(
    linter: &Linter,
    inputs: &[Input],
    format: Format,
    out: &mut impl Write,
    err: &mut impl Write,
) -> u8 {
    let files = match linter.lint_with(inputs, |file, diagnostics| {
        format.render(&file, &diagnostics)
    }) {
        Ok(files) => files,
        Err(read_error) => {
            // Nothing is left to report a failed write of the message to.
            let _ = writeln!(err, "{read_error}");
            return EXIT_FAILURE;
        }
    };
    let written = files
        .iter()
        .try_for_each(|rendered| match &rendered.output {
            Ok(output) => out.write_all(output),
            Err(err) => Err(io::Error::new(err.kind(), err.to_string())),
        })
        .and_then(|()| out.flush());
    let code = if files.iter().any(|rendered| rendered.internal_error) {
        EXIT_FAILURE
    } else if files.iter().any(|rendered| rendered.problems) {
        EXIT_PROBLEM_FOUND
    } else {
        EXIT_NO_PROBLEM
    };
    match written {
        // A reader that stops early, such as `head`, is not an error. actionlint dies of SIGPIPE
        // there without a message.
        Err(write_error) if write_error.kind() == ErrorKind::BrokenPipe => code,
        Err(write_error) => {
            let _ = writeln!(err, "could not write the output: {write_error}");
            EXIT_FAILURE
        }
        Ok(()) => code,
    }
}

/// Parses the flags as Go's `flag` package does, and returns whether `-oneline` is set and the
/// file arguments.
///
/// A flag starts with `-` or `--`. A boolean flag takes its value only after `=`, and another
/// flag takes it after `=` or from the next argument. The flags end at `--` or at the first
/// argument that is not a flag, such as `-` or a file. `-shellcheck`, `-pyflakes`, and
/// `-config-file` are accepted and ignored. The error is the message to print.
fn parse_args(args: &[OsString]) -> Result<(bool, &[OsString]), String> {
    let mut oneline = false;
    let mut rest = args;
    while let Some((arg, tail)) = rest.split_first() {
        let arg = arg.to_string_lossy();
        let Some(name) = arg.strip_prefix('-').filter(|name| !name.is_empty()) else {
            break;
        };
        rest = tail;
        if name == "-" {
            break;
        }
        let name = name.strip_prefix('-').unwrap_or(name);
        if name.is_empty() || name.starts_with(['-', '=']) {
            return Err(format!("bad flag syntax: {arg}"));
        }
        let (name, value) = match name.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (name, None),
        };
        match name {
            "oneline" | "no-color" => {
                let on = match value {
                    Some(value) => parse_bool(value).ok_or_else(|| {
                        format!(
                            "invalid boolean value {} for -{name}: parse error",
                            quote(value)
                        )
                    })?,
                    None => true,
                };
                if name == "oneline" {
                    oneline = on;
                }
            }
            "shellcheck" | "pyflakes" | "config-file" => {
                if value.is_none() {
                    let Some((_, tail)) = rest.split_first() else {
                        return Err(format!("flag needs an argument: -{name}"));
                    };
                    rest = tail;
                }
            }
            _ => return Err(format!("flag provided but not defined: -{name}")),
        }
    }
    Ok((oneline, rest))
}

/// Parses a boolean flag value as Go's `strconv.ParseBool` does.
fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "1" | "t" | "T" | "TRUE" | "true" | "True" => Some(true),
        "0" | "f" | "F" | "FALSE" | "false" | "False" => Some(false),
        _ => None,
    }
}

/// The output of one file.
struct Rendered {
    /// The rendered diagnostics, or the error of rendering them.
    output: io::Result<Vec<u8>>,
    /// Whether the file has a diagnostic.
    problems: bool,
    /// Whether the file has an `internal-error` diagnostic.
    internal_error: bool,
}

/// The output format.
#[derive(Clone, Copy)]
enum Format {
    Default,
    Oneline,
}

impl Format {
    fn render(self, file: &SourceFile, diagnostics: &[Diagnostic]) -> Rendered {
        let mut output = Vec::new();
        let written = match self {
            Self::Default => render_default(&mut output, file, diagnostics, Color::Off),
            Self::Oneline => render_oneline(&mut output, file, diagnostics, Color::Off),
        };
        Rendered {
            // The output waits for the other files, so it keeps no spare capacity.
            output: written.map(|()| {
                output.shrink_to_fit();
                output
            }),
            problems: !diagnostics.is_empty(),
            internal_error: has_internal_error(diagnostics),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn parse(args: &[&str]) -> Result<(bool, Vec<String>), String> {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        parse_args(&args).map(|(oneline, files)| {
            let files = files
                .iter()
                .map(|file| file.to_string_lossy().into_owned())
                .collect();
            (oneline, files)
        })
    }

    #[test]
    fn a_lone_dash_is_a_file_and_ends_the_flags() {
        assert_eq!(
            parse(&["-oneline", "-", "-no-color"]),
            Ok((true, vec!["-".to_owned(), "-no-color".to_owned()]))
        );
    }

    #[test]
    fn three_dashes_or_a_name_starting_with_equals_is_bad_flag_syntax() {
        for arg in ["---oneline", "-=x", "--=x"] {
            assert_eq!(parse(&[arg]), Err(format!("bad flag syntax: {arg}")));
        }
    }

    #[test]
    fn a_later_boolean_flag_overrides_an_earlier_one() {
        assert_eq!(
            parse(&["-oneline", "-oneline=0", "a.yaml"]),
            Ok((false, vec!["a.yaml".to_owned()]))
        );
    }

    #[test]
    fn boolean_values_are_the_ones_go_accepts() {
        for value in ["1", "t", "T", "TRUE", "true", "True"] {
            assert_eq!(parse_bool(value), Some(true), "{value}");
        }
        for value in ["0", "f", "F", "FALSE", "false", "False"] {
            assert_eq!(parse_bool(value), Some(false), "{value}");
        }
        for value in ["", "yes", "tRUE", " true"] {
            assert_eq!(parse_bool(value), None, "{value}");
        }
    }

    #[test]
    fn a_string_flag_takes_the_next_argument_even_if_it_starts_with_a_dash() {
        assert_eq!(
            parse(&["-config-file", "-oneline", "a.yaml"]),
            Ok((false, vec!["a.yaml".to_owned()]))
        );
    }

    /// The rules, except that the lint of a file named `panics.yaml` panics.
    fn check_or_panic(
        file: &SourceFile,
        local: &dyn workflint_rules::local::LocalMetadata,
    ) -> Vec<Diagnostic> {
        assert!(!file.path().ends_with("panics.yaml"), "a rule failed");
        workflint_rules::check_file(file, local)
    }

    /// Runs `linter` on `paths` in `dir` with one-line output, and returns the exit code and
    /// what was written to stdout and stderr.
    fn run_oneline(linter: &Linter, paths: &[&str]) -> (u8, String, String) {
        let inputs: Vec<Input> = paths.iter().map(Input::file).collect();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run(linter, &inputs, Format::Oneline, &mut out, &mut err);
        (
            code,
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
