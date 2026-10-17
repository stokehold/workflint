//! The `workflint` command-line tool.
//!
//! This is the minimal binary that the conformance suite runs, with the command line in
//! `tests/conformance/README.md`. It lints the files given as arguments with the YAML layer and
//! the workflow parser, and prints the diagnostics. M5.6 replaces it with the full command-line
//! interface.

use std::ffi::{OsStr, OsString};
use std::fmt::Display;
use std::fs;
use std::io::{self, BufWriter, ErrorKind, Write};
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use workflint_diagnostics::quote::quote;
use workflint_diagnostics::{
    Color, Diagnostic, FileId, SourceFile, Span, render_default, render_oneline, sort_and_dedup,
};
use workflint_yaml::rules::YAML_SYNTAX_ERROR;

/// No file has a diagnostic.
const EXIT_NO_PROBLEM: u8 = 0;
/// At least one file has a diagnostic.
const EXIT_PROBLEM_FOUND: u8 = 1;
/// The command line is invalid.
const EXIT_INVALID_OPTION: u8 = 2;
/// A file could not be read, or the output could not be written.
const EXIT_FAILURE: u8 = 3;

/// The usage text. Roadmap task M5.1 adds standard input.
const USAGE: &str = "Usage: workflint [-oneline] [-no-color] [-shellcheck=PATH] [-pyflakes=PATH] \
                     [-config-file PATH] FILES...\n\
                     This version of workflint does not read standard input yet, so FILES \
                     cannot be a lone -.";

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let (oneline, paths) = match parse_args(&args) {
        Ok((_, [])) => {
            eprintln!(
                "no files given: this version of workflint lints only the files it is given\n{USAGE}"
            );
            return ExitCode::from(EXIT_INVALID_OPTION);
        }
        // actionlint reads standard input for a lone `-`. A `-` next to other files is a file
        // name.
        Ok((_, [path])) if path == "-" => {
            eprintln!("reading standard input is not supported yet\n{USAGE}");
            return ExitCode::from(EXIT_INVALID_OPTION);
        }
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("{message}\n{USAGE}");
            return ExitCode::from(EXIT_INVALID_OPTION);
        }
    };

    let cwd = working_dir();
    // As in actionlint, a file that cannot be read stops the run before anything is printed.
    let mut files = Vec::with_capacity(paths.len());
    for (index, path) in paths.iter().enumerate() {
        match lint(index, path, &cwd) {
            Ok(file) => files.push(file),
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(EXIT_FAILURE);
            }
        }
    }

    let mut out = BufWriter::new(io::stdout().lock());
    let written = files
        .iter()
        .try_for_each(|(file, diagnostics)| {
            if oneline {
                render_oneline(&mut out, file, diagnostics, Color::Off)
            } else {
                render_default(&mut out, file, diagnostics, Color::Off)
            }
        })
        .and_then(|()| out.flush());
    match written {
        Ok(()) => {}
        // The reader closed the pipe, as `head` does. actionlint dies of SIGPIPE without a
        // message. workflint stops writing and exits with the lint result.
        Err(err) if err.kind() == ErrorKind::BrokenPipe => {}
        Err(err) => {
            eprintln!("could not write the output: {err}");
            return ExitCode::from(EXIT_FAILURE);
        }
    }

    if files.iter().any(|(_, diagnostics)| !diagnostics.is_empty()) {
        ExitCode::from(EXIT_PROBLEM_FOUND)
    } else {
        ExitCode::from(EXIT_NO_PROBLEM)
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

/// Returns the working directory as Go's `os.Getwd` does, which actionlint makes paths relative
/// to.
///
/// On Unix that is `PWD` when it is absolute and names the working directory, so a path through
/// a symbolic link that the shell followed stays as the user typed it. Otherwise it is the
/// directory the system reports, and `.` if that fails, as in actionlint.
fn working_dir() -> PathBuf {
    #[cfg(unix)]
    if let Some(pwd) = std::env::var_os("PWD").map(PathBuf::from)
        && pwd.is_absolute()
        && same_dir(&pwd, Path::new("."))
    {
        return pwd;
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Whether `a` and `b` are the same directory, compared by device and inode as Go's
/// `os.SameFile` does.
#[cfg(unix)]
fn same_dir(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (fs::metadata(a), fs::metadata(b)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

/// Returns `target` relative to `base` as Go's `filepath.Rel` does, or `None` where `Rel`
/// fails.
///
/// The paths are compared as text after `..` and `.` are removed, without asking the file
/// system. `Rel` fails when only one path is absolute, when the drives differ, or when the
/// answer would have to name the directory above a relative `base`. On Windows, names compare
/// without case.
fn relative_path(base: &Path, target: &Path) -> Option<PathBuf> {
    let base = clean(base);
    let target = clean(target);
    if base.rooted != target.rooted || !same_name(base.prefix, target.prefix) {
        return None;
    }
    let common = base
        .names
        .iter()
        .zip(&target.names)
        .take_while(|(a, b)| same_name(Some(a), Some(b)))
        .count();
    let up = base.names.get(common..).unwrap_or_default();
    if up.first().is_some_and(|name| *name == "..") {
        return None;
    }
    let mut relative: PathBuf = up.iter().map(|_| "..").collect();
    relative.extend(target.names.get(common..).unwrap_or_default());
    if relative.as_os_str().is_empty() {
        relative.push(".");
    }
    Some(relative)
}

/// A path after the lexical cleaning of Go's `filepath.Clean`.
struct CleanPath<'a> {
    /// The Windows drive or share, such as `C:`.
    prefix: Option<&'a OsStr>,
    /// Whether the path starts at the root of its drive.
    rooted: bool,
    /// The names after the root. Only a relative path keeps `..`, and only at its start.
    names: Vec<&'a OsStr>,
}

/// Removes `.` and each `..` that follows a name, as Go's `filepath.Clean` does. A `..` at the
/// root is dropped.
fn clean(path: &Path) -> CleanPath<'_> {
    let mut clean = CleanPath {
        prefix: None,
        rooted: false,
        names: Vec::new(),
    };
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => clean.prefix = Some(prefix.as_os_str()),
            Component::RootDir => clean.rooted = true,
            Component::CurDir => {}
            Component::ParentDir => match clean.names.last() {
                Some(last) if *last != ".." => {
                    clean.names.pop();
                }
                _ if clean.rooted => {}
                _ => clean.names.push(component.as_os_str()),
            },
            Component::Normal(name) => clean.names.push(name),
        }
    }
    clean
}

/// Whether two path names are equal, ignoring case on Windows as Go's `filepath.Rel` does.
fn same_name(a: Option<&OsStr>, b: Option<&OsStr>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) if cfg!(windows) => {
            a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
        }
        _ => a == b,
    }
}

/// Reads, decodes, and parses the file at `path`, and returns it with its sorted diagnostics.
///
/// As in actionlint, the diagnostics name the file by its path relative to `cwd` when
/// [`relative_path`] gives one, and by `path` as given otherwise. The error is the message to
/// print when the file cannot be read, and names the file by `path` as given.
fn lint(
    index: usize,
    path: &OsString,
    cwd: &Path,
) -> Result<(SourceFile, Vec<Diagnostic>), String> {
    let display = path.to_string_lossy();
    let fail = |err: &dyn Display| format!("could not read {}: {err}", quote(&display));
    let bytes = fs::read(path).map_err(|err| fail(&err))?;
    let id = FileId::new(u32::try_from(index).map_err(|err| fail(&err))?);
    let (text, decode_error) = match workflint_yaml::decode(&bytes) {
        Ok(text) => (text.into_owned(), None),
        Err(err) => (decoded_prefix(&bytes, err.offset()), Some(err)),
    };
    let name = match relative_path(cwd, Path::new(path)) {
        Some(relative) => relative.to_string_lossy().into_owned(),
        None => display.clone().into_owned(),
    };
    let file = SourceFile::new(id, &name, text).map_err(|err| fail(&err))?;
    let mut diagnostics = match decode_error {
        // As in actionlint, bytes that do not decode are a YAML syntax error. It points at the
        // end of the text that does decode.
        Some(err) => {
            let end = u32::try_from(file.text().len()).unwrap_or(u32::MAX);
            vec![Diagnostic::new(
                &YAML_SYNTAX_ERROR,
                Span::new(id, end, end),
                format!("could not parse as YAML: {}", err.problem()),
            )]
        }
        None => parse_file(&file),
    };
    sort_and_dedup(&file, &mut diagnostics);
    Ok((file, diagnostics))
}

/// Parses `file` with the YAML layer and, if it gives a document, with the workflow parser, and
/// returns the diagnostics of both.
fn parse_file(file: &SourceFile) -> Vec<Diagnostic> {
    let parsed = workflint_yaml::parse(file);
    let mut diagnostics = parsed.diagnostics;
    if let Some(document) = &parsed.document {
        diagnostics.extend(workflint_syntax::parse(file, document).diagnostics);
    }
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

    /// Checks `relative_path` on `(base, target, want)` cases, where `None` means that Go's
    /// `filepath.Rel` returns an error.
    fn check_relative_paths(cases: &[(&str, &str, Option<&str>)]) {
        for &(base, target, want) in cases {
            assert_eq!(
                relative_path(Path::new(base), Path::new(target)),
                want.map(PathBuf::from),
                "{base:?} {target:?}"
            );
        }
    }

    #[test]
    fn relative_path_matches_go_filepath_rel() {
        // The cases of `reltests` in Go's `path/filepath/path_test.go`.
        check_relative_paths(&[
            ("a/b", "a/b", Some(".")),
            ("a/b/.", "a/b", Some(".")),
            ("a/b", "a/b/.", Some(".")),
            ("./a/b", "a/b", Some(".")),
            ("a/b", "./a/b", Some(".")),
            ("ab/cd", "ab/cde", Some("../cde")),
            ("ab/cd", "ab/c", Some("../c")),
            ("a/b", "a/b/c/d", Some("c/d")),
            ("a/b", "a/b/../c", Some("../c")),
            ("a/b/../c", "a/b", Some("../b")),
            ("a/b/c", "a/c/d", Some("../../c/d")),
            ("a/b", "c/d", Some("../../c/d")),
            ("a/b/c/d", "a/b", Some("../..")),
            ("a/b/c/d", "a/b/", Some("../..")),
            ("a/b/c/d/", "a/b", Some("../..")),
            ("a/b/c/d/", "a/b/", Some("../..")),
            ("../../a/b", "../../a/b/c/d", Some("c/d")),
            ("/a/b", "/a/b", Some(".")),
            ("/a/b/.", "/a/b", Some(".")),
            ("/a/b", "/a/b/.", Some(".")),
            ("/ab/cd", "/ab/cde", Some("../cde")),
            ("/ab/cd", "/ab/c", Some("../c")),
            ("/a/b", "/a/b/c/d", Some("c/d")),
            ("/a/b", "/a/b/../c", Some("../c")),
            ("/a/b/../c", "/a/b", Some("../b")),
            ("/a/b/c", "/a/c/d", Some("../../c/d")),
            ("/a/b", "/c/d", Some("../../c/d")),
            ("/a/b/c/d", "/a/b", Some("../..")),
            ("/a/b/c/d", "/a/b/", Some("../..")),
            ("/a/b/c/d/", "/a/b", Some("../..")),
            ("/a/b/c/d/", "/a/b/", Some("../..")),
            ("/../../a/b", "/../../a/b/c/d", Some("c/d")),
            (".", "a/b", Some("a/b")),
            (".", "..", Some("..")),
            ("", "../../.", Some("../..")),
            ("..", ".", None),
            ("..", "a", None),
            ("../..", "..", None),
            ("a", "/a", None),
            ("/a", "a", None),
        ]);
    }

    #[cfg(windows)]
    #[test]
    fn relative_path_matches_go_filepath_rel_on_windows() {
        // The cases of `winreltests` in Go's `path/filepath/path_test.go`.
        check_relative_paths(&[
            (r"C:a\b\c", "C:a/b/d", Some(r"..\d")),
            (r"C:\", r"D:\", None),
            ("C:", "D:", None),
            (r"C:\Projects", r"c:\projects\src", Some("src")),
            (r"C:\Projects", r"c:\projects", Some(".")),
            (r"C:\Projects\a\..", r"c:\projects", Some(".")),
            (r"\\host\share", r"\\host\share\file.txt", Some("file.txt")),
        ]);
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
