//! Parses the one-line output format, `path:line:col: message [code]`.
//!
//! workflint puts a rule ID in the brackets, the reference puts its kind
//! (`docs/differences.md#rule-id-in-brackets`). [`Diagnostic::code`] holds
//! whichever the tool printed.

use std::cmp::Ordering;
use std::fmt;
use std::sync::LazyLock;

use regex::Regex;

/// One error line from the output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// The file path, with `/` separators.
    pub path: String,
    pub line: u32,
    pub column: u32,
    pub message: String,
    /// The text in brackets at the end of the line.
    pub code: String,
}

impl Diagnostic {
    /// The order of `compareErrors` in the reference's `error.go`: path, line,
    /// column, then message.
    #[must_use]
    pub fn reference_order(&self, other: &Self) -> Ordering {
        self.path
            .cmp(&other.path)
            .then(self.line.cmp(&other.line))
            .then(self.column.cmp(&other.column))
            .then_with(|| self.message.cmp(&other.message))
    }
}

/// Formats the diagnostic as the reference's `Error.Error()` does.
impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}: {} [{}]",
            self.path, self.line, self.column, self.message, self.code
        )
    }
}

static LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(.+?):(\d+):(\d+): (.*) \[([^\[\]]+)\]$").expect("the pattern is valid")
});

/// Parses one output line. Backslashes in the path become `/`, as
/// `filepath.ToSlash` does in the reference's tests.
///
/// # Errors
///
/// Returns a message that quotes the line when it does not have the form
/// `path:line:col: message [code]`.
pub fn parse_line(line: &str) -> Result<Diagnostic, String> {
    let invalid = || format!("output line is not `path:line:col: message [code]`: {line:?}");
    let captures = LINE.captures(line).ok_or_else(invalid)?;
    let number = |index: usize| captures[index].parse::<u32>().map_err(|_| invalid());
    Ok(Diagnostic {
        path: captures[1].replace('\\', "/"),
        line: number(2)?,
        column: number(3)?,
        message: captures[4].to_owned(),
        code: captures[5].to_owned(),
    })
}

/// Parses every line of `stdout`.
///
/// # Errors
///
/// Returns the message of the first line that does not parse.
pub fn parse_output(stdout: &str) -> Result<Vec<Diagnostic>, String> {
    stdout.lines().map(parse_line).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_splits_into_path_position_message_and_code() {
        let diagnostic =
            parse_line("test.yaml:3:5: unexpected key \"foo\" [syntax-check]").unwrap();
        assert_eq!(
            diagnostic,
            Diagnostic {
                path: "test.yaml".into(),
                line: 3,
                column: 5,
                message: "unexpected key \"foo\"".into(),
                code: "syntax-check".into(),
            }
        );
    }

    #[test]
    fn a_message_with_brackets_and_colons_keeps_them() {
        let diagnostic = parse_line("a.yaml:1:2: got [x]: y:1:2: z [expression]").unwrap();
        assert_eq!(diagnostic.message, "got [x]: y:1:2: z");
        assert_eq!(diagnostic.code, "expression");
    }

    #[test]
    fn backslashes_in_the_path_become_slashes() {
        let diagnostic = parse_line(r"workflows\nested\a.yaml:1:2: msg [id]").unwrap();
        assert_eq!(diagnostic.path, "workflows/nested/a.yaml");
    }

    #[test]
    fn a_windows_drive_letter_stays_in_the_path() {
        let diagnostic = parse_line(r"C:\work\a.yaml:4:9: msg [id]").unwrap();
        assert_eq!(diagnostic.path, "C:/work/a.yaml");
        assert_eq!((diagnostic.line, diagnostic.column), (4, 9));
    }

    #[test]
    fn display_round_trips_a_line() {
        let line = "workflows/a.yaml:10:7: message [kind]";
        assert_eq!(parse_line(line).unwrap().to_string(), line);
    }

    #[test]
    fn a_line_without_a_code_does_not_parse() {
        let error = parse_line("test.yaml:1:1: message").unwrap_err();
        assert!(error.contains("\"test.yaml:1:1: message\""), "{error}");
    }

    #[test]
    fn a_line_without_a_position_does_not_parse() {
        assert!(parse_line("Hello, world!").is_err());
        assert!(parse_line("test.yaml:x:1: message [id]").is_err());
    }

    #[test]
    fn output_with_one_bad_line_does_not_parse() {
        let stdout = "test.yaml:1:1: a [x]\nwarning: something\n";
        assert!(parse_output(stdout).is_err());
    }

    #[test]
    fn empty_output_has_no_diagnostics() {
        assert_eq!(parse_output("").unwrap(), Vec::new());
    }

    #[test]
    fn crlf_line_endings_are_accepted() {
        let diagnostics = parse_output("a.yaml:1:1: a [x]\r\nb.yaml:2:2: b [y]\r\n").unwrap();
        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics[0].code, "x");
    }
}
