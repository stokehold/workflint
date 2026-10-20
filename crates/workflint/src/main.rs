//! The `workflint` command-line tool.
//!
//! This is the minimal binary that the conformance suite runs, with the command line in
//! `tests/conformance/README.md`. It lints the files given as arguments in parallel with the YAML
//! layer, the workflow parser, and the rules, and prints the diagnostics in the order of the
//! arguments. M5.6 replaces it with the full command-line interface.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt::Display;
use std::fs;
use std::io::{self, BufWriter, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rayon::prelude::*;
use workflint_diagnostics::quote::quote;
use workflint_diagnostics::{
    Color, Diagnostic, FileId, SourceFile, Span, render_default, render_oneline, sort_and_dedup,
};
use workflint_linter::{FileMetadata, FileRecord, LocalCache, ProjectRoots, path};
use workflint_rules::local::{LocalMetadata, NoProject};
use workflint_yaml::rules::YAML_SYNTAX_ERROR;

/// No file has a diagnostic.
const EXIT_NO_PROBLEM: u8 = 0;
/// At least one file has a diagnostic.
const EXIT_PROBLEM_FOUND: u8 = 1;
/// The command line is invalid.
const EXIT_INVALID_OPTION: u8 = 2;
/// A file could not be read, or the output could not be written.
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

    // As in actionlint, a file that cannot be read stops the run before anything is printed.
    let cwd = path::working_dir();
    let mut roots = ProjectRoots::default();
    let mut caches = BTreeMap::new();
    let inputs: Vec<Input> = paths
        .iter()
        .map(|path| {
            let path = Path::new(path);
            let root = roots.find(path, &cwd);
            if let Some(root) = &root {
                caches
                    .entry(root.clone())
                    .or_insert_with(|| LocalCache::new(root.clone()));
            }
            Input {
                path,
                absolute: path::absolute(&cwd, path),
                root,
            }
        })
        .collect();
    let format = if oneline {
        Format::Oneline
    } else {
        Format::Default
    };
    let files = match lint(&inputs, &caches, &cwd, format) {
        Ok(files) => files,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(EXIT_FAILURE);
        }
    };

    let mut out = BufWriter::new(io::stdout().lock());
    let written = files
        .iter()
        .try_for_each(|rendered| match &rendered.output {
            Ok(output) => out.write_all(output),
            Err(err) => Err(io::Error::new(err.kind(), err.to_string())),
        })
        .and_then(|()| out.flush());
    let code = if files.iter().any(|rendered| rendered.problems) {
        EXIT_PROBLEM_FOUND
    } else {
        EXIT_NO_PROBLEM
    };
    match written {
        // A reader that stops early, such as `head`, is not an error. actionlint dies of SIGPIPE
        // there without a message.
        Err(err) if err.kind() == ErrorKind::BrokenPipe => ExitCode::from(code),
        Err(err) => {
            eprintln!("could not write the output: {err}");
            ExitCode::from(EXIT_FAILURE)
        }
        Ok(()) => ExitCode::from(code),
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

/// Lints the files of `inputs` in parallel, with the cache of each project in `caches`, and
/// returns their output in `format`, in the order of `inputs`. The error is the message to print
/// for the first file that cannot be read.
///
/// Each file is linted as if no other file had used the caches of local actions and local
/// reusable workflows. Replaying the files' uses of the caches in order then finds the files
/// whose diagnostics depend on the files before them, which are linted again (`LocalCache`).
/// A file that did not use a cache is rendered at once, so that its text and diagnostics are
/// freed.
fn lint(
    inputs: &[Input<'_>],
    caches: &BTreeMap<PathBuf, LocalCache>,
    cwd: &Path,
    format: Format,
) -> Result<Vec<Rendered>, String> {
    let cache_of = |input: &Input<'_>| input.root.as_ref().and_then(|root| caches.get(root));
    let linted: Result<Vec<Linted>, String> = inputs
        .par_iter()
        .enumerate()
        .map(|(index, input)| {
            let (file, decode_error) = read(index, input.path, cwd)?;
            if let Some(diagnostic) = decode_error {
                return Ok(Linted::Done(format.render(&file, &[diagnostic])));
            }
            let Some(cache) = cache_of(input) else {
                return Ok(Linted::Done(
                    format.render(&file, &check(&file, &NoProject)),
                ));
            };
            let local = cache.for_file(&input.absolute);
            let diagnostics = check(&file, &local);
            let record = local.into_record();
            Ok(if record.is_empty() {
                Linted::Done(format.render(&file, &diagnostics))
            } else {
                Linted::Pending {
                    file,
                    diagnostics,
                    cache,
                    record,
                }
            })
        })
        .collect();
    let again: Vec<Again> = inputs
        .iter()
        .zip(linted?)
        .map(|(input, linted)| match linted {
            Linted::Done(rendered) => Again::Done(rendered),
            Linted::Pending {
                file,
                diagnostics,
                cache,
                record,
            } => match cache.commit(record) {
                Some(prior) => Again::Lint(file, cache.for_file_after(&input.absolute, prior)),
                None => Again::Render(file, diagnostics),
            },
        })
        .collect();
    Ok(again
        .into_par_iter()
        .map(|again| match again {
            Again::Lint(file, local) => format.render(&file, &check(&file, &local)),
            Again::Render(file, diagnostics) => format.render(&file, &diagnostics),
            Again::Done(rendered) => rendered,
        })
        .collect())
}

/// A file given on the command line.
struct Input<'a> {
    /// The path as given.
    path: &'a Path,
    /// The path made absolute and cleaned.
    absolute: PathBuf,
    /// The project root, if the file is in a project.
    root: Option<PathBuf>,
}

/// A file linted as if no other file had used the cache of its project.
enum Linted<'c> {
    /// The file did not use a cache, so its output is final.
    Done(Rendered),
    /// The file used `cache`, so its diagnostics may depend on the files before it.
    Pending {
        file: SourceFile,
        /// The sorted diagnostics.
        diagnostics: Vec<Diagnostic>,
        cache: &'c LocalCache,
        record: FileRecord,
    },
}

/// What is left to do for a file after the uses of the caches are replayed.
enum Again<'c> {
    /// Lint the file again with this view of its cache.
    Lint(SourceFile, FileMetadata<'c>),
    /// Render the diagnostics of the first lint.
    Render(SourceFile, Vec<Diagnostic>),
    Done(Rendered),
}

/// The output of one file.
struct Rendered {
    /// The rendered diagnostics, or the error of rendering them.
    output: io::Result<Vec<u8>>,
    /// Whether the file has a diagnostic.
    problems: bool,
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
        }
    }
}

/// Reads and decodes the file at `path`, and returns it with the YAML syntax error of bytes that
/// do not decode.
///
/// The file is named by its path relative to `cwd` when there is one. The error is the message to
/// print when the file cannot be read, and names the file as given.
fn read(index: usize, path: &Path, cwd: &Path) -> Result<(SourceFile, Option<Diagnostic>), String> {
    let given = path.to_string_lossy();
    let fail = |err: &dyn Display| format!("could not read {}: {err}", quote(&given));
    let bytes = fs::read(path).map_err(|err| fail(&err))?;
    let display = path::display_path(cwd, path);
    let display = display.to_string_lossy();
    let id = FileId::new(u32::try_from(index).map_err(|err| fail(&err))?);
    let (text, decode_error) = match workflint_yaml::decode(&bytes) {
        Ok(text) => (text.into_owned(), None),
        Err(err) => (decoded_prefix(&bytes, err.offset()), Some(err)),
    };
    let file = SourceFile::new(id, display.as_ref(), text).map_err(|err| fail(&err))?;
    // As in actionlint, bytes that do not decode are a YAML syntax error. It points at the end of
    // the text that does decode.
    let diagnostic = decode_error.map(|err| {
        let end = u32::try_from(file.text().len()).unwrap_or(u32::MAX);
        Diagnostic::new(
            &YAML_SYNTAX_ERROR,
            Span::new(id, end, end),
            format!("could not parse as YAML: {}", err.problem()),
        )
    });
    Ok((file, diagnostic))
}

/// Parses `file` with the YAML layer and, if it gives a document, with the workflow parser, runs
/// the rules on the workflow with the project's local metadata in `local`, and returns the sorted
/// diagnostics of all three.
fn check(file: &SourceFile, local: &dyn LocalMetadata) -> Vec<Diagnostic> {
    let mut diagnostics = workflint_rules::check_file(file, local);
    sort_and_dedup(file, &mut diagnostics);
    diagnostics
}

/// Decodes the longest prefix of `bytes` that ends at or before `end` and decodes without error.
fn decoded_prefix(bytes: &[u8], mut end: usize) -> String {
    loop {
        match bytes.get(..end).map(workflint_yaml::decode) {
            Some(Ok(text)) => return text.into_owned(),
            Some(Err(err)) if err.offset() < end => end = err.offset(),
            _ => return String::new(),
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

    #[test]
    fn decoded_prefix_stops_before_an_invalid_utf8_sequence() {
        // 0xC3 starts a two-byte sequence, which `x` does not continue.
        let bytes = b"a: b\xC3x";
        let err = workflint_yaml::decode(bytes).unwrap_err();
        assert_eq!(decoded_prefix(bytes, err.offset()), "a: b");
    }

    #[test]
    fn decoded_prefix_stops_before_a_lone_utf16_surrogate() {
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend("a: ".encode_utf16().flat_map(u16::to_le_bytes));
        bytes.extend(0xD800_u16.to_le_bytes());
        bytes.extend(u16::from(b'x').to_le_bytes());
        let err = workflint_yaml::decode(&bytes).unwrap_err();
        assert_eq!(decoded_prefix(&bytes, err.offset()), "a: ");
    }
}
