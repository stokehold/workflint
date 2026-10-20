//! Executable resolution, as the reference's `resolveExternalCommand` (`process.go:123`) does it
//! with `execabs.LookPath` and `shellwords.Parse`.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Path, PathBuf};
use std::{env, fs};

use super::ToolCommand;

/// The reason a tool's executable could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    /// Neither the value nor the first word of it as a command line names an executable.
    NotFound {
        /// The value as given.
        name: String,
    },
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { name } => write!(f, "executable {name:?} was not found on PATH"),
        }
    }
}

impl std::error::Error for ResolveError {}

pub(super) fn resolve(spec: &str, path_var: Option<&OsStr>) -> Result<ToolCommand, ResolveError> {
    if let Some(program) = look_path(Path::new(spec), path_var) {
        return Ok(ToolCommand {
            program,
            args: Vec::new(),
        });
    }
    if let Some((first, rest)) = split_command_line(spec).split_first()
        && let Some(program) = look_path(Path::new(first), path_var)
    {
        return Ok(ToolCommand {
            program,
            args: rest.iter().map(OsString::from).collect(),
        });
    }
    Err(ResolveError::NotFound {
        name: spec.to_owned(),
    })
}

/// Go's `execabs.LookPath`. A name with a path separator is used as given. A bare name is looked
/// up in the absolute directories of `PATH`, never in the current directory.
///
/// Go's `LookPath` returns an error when the first match is in a relative `PATH` entry. This
/// skips the entry and keeps looking.
fn look_path(file: &Path, path_var: Option<&OsStr>) -> Option<PathBuf> {
    if file.as_os_str().is_empty() {
        return None;
    }
    if file.components().count() > 1 || file.has_root() {
        return find_executable(file);
    }
    env::split_paths(path_var?)
        .filter(|dir| dir.is_absolute())
        .find_map(|dir| find_executable(&dir.join(file)))
}

#[cfg(unix)]
fn find_executable(file: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::metadata(file).ok()?;
    (metadata.is_file() && metadata.permissions().mode() & 0o111 != 0).then(|| file.to_owned())
}

/// Go's `findExecutable` on Windows: the name as given if it has an extension, then the name with
/// each extension of `PATHEXT` appended.
#[cfg(not(unix))]
fn find_executable(file: &Path) -> Option<PathBuf> {
    let exists = |candidate: &Path| fs::metadata(candidate).is_ok_and(|m| !m.is_dir());
    if file.extension().is_some() && exists(file) {
        return Some(file.to_owned());
    }
    let pathext = env::var("PATHEXT").unwrap_or_default();
    let mut extensions: Vec<String> = pathext
        .split(';')
        .filter(|ext| !ext.is_empty())
        .map(|ext| {
            let ext = ext.to_ascii_lowercase();
            if ext.starts_with('.') {
                ext
            } else {
                format!(".{ext}")
            }
        })
        .collect();
    if extensions.is_empty() {
        extensions = [".com", ".exe", ".bat", ".cmd"].map(String::from).to_vec();
    }
    extensions.iter().find_map(|ext| {
        let mut candidate = file.as_os_str().to_owned();
        candidate.push(ext);
        let candidate = PathBuf::from(candidate);
        exists(&candidate).then_some(candidate)
    })
}

/// `shellwords.Parse` of `github.com/mattn/go-shellwords` with its default options: words split at
/// unquoted spaces, tabs and line breaks, single and double quotes, and backslash escapes outside
/// single quotes. Parsing stops at an unquoted `;`, `&`, `|`, `<` or `>`. An unterminated quote or
/// escape gives no words.
fn split_command_line(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let (mut escaped, mut single, mut double) = (false, false, false);
    for c in line.chars() {
        if escaped {
            word.push(c);
            escaped = false;
            in_word = true;
            continue;
        }
        match c {
            '\\' if single => word.push(c),
            '\\' => escaped = true,
            ' ' | '\t' | '\r' | '\n' if single || double => word.push(c),
            ' ' | '\t' | '\r' | '\n' => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            '\'' if !double => {
                single = !single;
                in_word = true;
            }
            '"' if !single => {
                double = !double;
                in_word = true;
            }
            ';' | '&' | '|' | '<' | '>' if !single && !double => break,
            _ => {
                word.push(c);
                in_word = true;
            }
        }
    }
    if escaped || single || double {
        return Vec::new();
    }
    if in_word {
        words.push(word);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::split_command_line;

    #[test]
    fn a_command_line_splits_at_unquoted_spaces() {
        assert_eq!(
            split_command_line("  tool  -a\t'b c' \"d 'e'\" f\\ g "),
            ["tool", "-a", "b c", "d 'e'", "f g"]
        );
    }

    #[test]
    fn an_empty_quoted_word_is_a_word() {
        assert_eq!(split_command_line("tool '' \"\""), ["tool", "", ""]);
    }

    #[test]
    fn backslashes_escape_outside_single_quotes() {
        assert_eq!(
            split_command_line(r#"tool 'a\b' "c\"d" e\'f"#),
            ["tool", r"a\b", "c\"d", "e'f"]
        );
    }

    #[test]
    fn parsing_stops_at_an_unquoted_operator() {
        assert_eq!(split_command_line("tool -x; rm -rf /"), ["tool", "-x"]);
        assert_eq!(split_command_line("tool 'a;b' | cat"), ["tool", "a;b"]);
    }

    #[test]
    fn an_unterminated_quote_gives_no_words() {
        assert_eq!(split_command_line("tool 'a"), Vec::<String>::new());
        assert_eq!(split_command_line("tool \"a"), Vec::<String>::new());
        assert_eq!(split_command_line("tool a\\"), Vec::<String>::new());
    }
}
