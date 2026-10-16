//! Validation of branch, tag, and path filter patterns.
//!
//! [`validate_ref_glob`] checks a pattern of `branches`, `branches-ignore`,
//! `tags`, or `tags-ignore`, and [`validate_path_glob`] checks a pattern of
//! `paths` or `paths-ignore`. Both port the reference's `glob.go` and return
//! the reference's messages. They only validate: nothing here matches a
//! pattern against a name.
//!
//! The reference scans a pattern with Go's `text/scanner`. This module copies
//! the parts of that scanner it depends on: a skipped byte order mark at the
//! start, an error for a NUL character, and the column of each error.
//!
//! ```
//! use workflint_syntax::glob::validate_ref_glob;
//!
//! let problems = validate_ref_glob("v[2-1]");
//! assert_eq!(problems.len(), 1);
//! assert!(problems[0].message.contains("start of range '2' (50)"));
//! assert_eq!(problems[0].range, Some(4..5));
//! ```

use std::ops::Range;

use workflint_diagnostics::quote::{quote, quote_rune};

/// A problem in a glob pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlobProblem {
    /// The message, byte for byte the reference's.
    pub message: String,
    /// The byte range in the pattern of the character that the problem
    /// refers to, or `None` where the reference reports column 0: for an
    /// empty pattern, for a newline and every problem after it, and for the
    /// two checks of a whole path pattern.
    pub range: Option<Range<usize>>,
}

/// Validates a branch or tag filter pattern and returns its problems in
/// order.
///
/// Besides the glob syntax, the pattern must be a valid Git ref name: no
/// spaces, `~`, `^`, or `:`, no `/` at the start, and no `/` or `.` at the
/// end.
#[must_use]
pub fn validate_ref_glob(pattern: &str) -> Vec<GlobProblem> {
    Validator::new(pattern, true).validate()
}

/// Validates a path filter pattern and returns its problems in order.
///
/// A pattern with whitespace at either end, or one that is `.` or `..` or
/// starts with `./` or `../` after an optional `!`, reports only those
/// problems. The glob syntax is checked only when neither applies.
#[must_use]
pub fn validate_path_glob(pattern: &str) -> Vec<GlobProblem> {
    let trimmed = pattern.trim();
    let mut problems = Vec::new();
    if trimmed != pattern {
        problems.push(GlobProblem {
            message: "leading and trailing spaces are not allowed in glob path".to_owned(),
            range: None,
        });
    }
    let path = trimmed.strip_prefix('!').unwrap_or(trimmed);
    if matches!(path, "." | "..") || path.starts_with("./") || path.starts_with("../") {
        problems.push(GlobProblem {
            message: "'.' and '..' are not allowed in glob path".to_owned(),
            range: None,
        });
    }
    if !problems.is_empty() {
        return problems;
    }
    Validator::new(pattern, false).validate()
}

const PRECEDING_SPECIAL: &str = "the preceding character must not be special character";
const CANNOT_CONTAIN: &str = "ref name cannot contain spaces, ~, ^, :, [, ?, *";

/// The scanner and the validator of `glob.go` in one.
///
/// Like Go's `text/scanner`, the scanner reads one character ahead: when
/// [`Validator::next`] returns a character, the following one has been read
/// already, and reading it can report a NUL error.
struct Validator<'a> {
    pattern: &'a str,
    is_ref: bool,
    /// Whether the previous character may be followed by `?` or `+`.
    prec: bool,
    problems: Vec<GlobProblem>,
    /// The byte offset after the lookahead character.
    offset: usize,
    lookahead: Lookahead,
    /// The range of the last character that [`Validator::next`] returned.
    last: Option<Range<usize>>,
    /// Whether [`Validator::next`] has returned a newline. The reference
    /// reports column 0 from then on.
    after_newline: bool,
}

#[derive(Clone)]
enum Lookahead {
    /// Nothing is read yet.
    Unread,
    /// The pattern ends.
    End,
    /// A character and its byte range.
    Char(char, Range<usize>),
}

impl<'a> Validator<'a> {
    fn new(pattern: &'a str, is_ref: bool) -> Self {
        Self {
            pattern,
            is_ref,
            prec: false,
            problems: Vec::new(),
            offset: 0,
            lookahead: Lookahead::Unread,
            last: None,
            after_newline: false,
        }
    }

    fn validate(mut self) -> Vec<GlobProblem> {
        if self.pattern.is_empty() {
            self.error("glob pattern cannot be empty".to_owned());
            return self.problems;
        }

        match self.peek() {
            Some('/') if self.is_ref => {
                self.next();
                self.invalid_ref_char('/', "ref name must not start with /");
                self.prec = true;
            }
            Some('!') => {
                self.next();
                if self.peek().is_none() {
                    self.unexpected(
                        Some('!'),
                        "! at first character (negate pattern)",
                        "at least one character must follow !",
                    );
                    return self.problems;
                }
                self.prec = false;
            }
            _ => {}
        }

        while self.validate_next() {}
        self.problems
    }

    /// Validates the next character, or the next escape or character match,
    /// and returns whether more characters follow.
    fn validate_next(&mut self) -> bool {
        let mut c = self.next();
        let mut prec = true;

        match c {
            Some('\\') => match self.peek() {
                Some('[' | '?' | '*') => {
                    c = self.next();
                    if self.is_ref
                        && let Some(escaped) = c
                    {
                        // The reference reports the character after the
                        // escaped one instead
                        // (docs/differences.md#glob-escape-character).
                        self.invalid_ref_char(escaped, CANNOT_CONTAIN);
                    }
                }
                Some('+' | '\\' | '!') => c = self.next(),
                _ if self.is_ref => {
                    self.invalid_ref_char(
                        '\\',
                        "only special characters [, ?, +, *, \\, ! can be escaped with \\",
                    );
                    c = self.next();
                }
                // A path can contain `\`.
                _ => {}
            },
            Some('?') => {
                if !self.prec {
                    self.unexpected(c, "special character ? (zero or one)", PRECEDING_SPECIAL);
                }
                prec = false;
            }
            Some('+') => {
                if !self.prec {
                    self.unexpected(c, "special character + (one or more)", PRECEDING_SPECIAL);
                }
                prec = false;
            }
            Some('*') => prec = false,
            Some('[') => {
                if !self.character_match() {
                    return false;
                }
            }
            Some('\r') => {
                if self.peek() == Some('\n') {
                    c = self.next();
                }
                self.unexpected(c, "", "newline cannot be contained");
            }
            Some('\n') => self.unexpected(c, "", "newline cannot be contained"),
            Some(space @ (' ' | '\t' | '~' | '^' | ':')) if self.is_ref => {
                self.invalid_ref_char(space, CANNOT_CONTAIN);
            }
            _ => {}
        }
        self.prec = prec;

        if self.peek().is_none() {
            if self.is_ref
                && let Some(end @ ('/' | '.')) = c
            {
                self.invalid_ref_char(end, "ref name must not end with / and .");
            }
            return false;
        }
        true
    }

    /// Validates a character match after its `[`. Returns `false` if the
    /// pattern ends before the `]`.
    fn character_match(&mut self) -> bool {
        if self.peek() == Some(']') {
            let c = self.next();
            self.unexpected(
                c,
                "content of character match []",
                "character match must not be empty",
            );
            return true;
        }

        let mut chars = 0;
        let c = loop {
            let c = self.next();
            match c {
                Some(']') => break c,
                None => {
                    self.unexpected(c, "end of character match []", "missing ]");
                    return false;
                }
                Some(start) => {
                    if self.peek() != Some('-') {
                        chars += 1;
                        continue;
                    }
                    // A range such as `0-9` holds one or more characters, and
                    // only a count of exactly 1 is reported below.
                    chars += 2;
                    self.next();
                    match self.peek() {
                        Some(']') => {
                            let c = self.next();
                            self.unexpected(c, "character range in []", "end of range is missing");
                            break c;
                        }
                        None => {}
                        Some(_) => {
                            if let Some(end) = self.next()
                                && start > end
                            {
                                let why = format!(
                                    "start of range {} ({}) is larger than end of range {} ({})",
                                    quote_rune(start),
                                    u32::from(start),
                                    quote_rune(end),
                                    u32::from(end),
                                );
                                self.unexpected(Some(end), "character range in []", &why);
                            }
                        }
                    }
                }
            }
        };

        if chars == 1 {
            self.unexpected(
                c,
                "character match []",
                "character match with single character is useless. simply use x instead of [x]",
            );
        }
        true
    }

    /// Returns the next character without consuming it, or `None` at the end.
    fn peek(&mut self) -> Option<char> {
        if matches!(self.lookahead, Lookahead::Unread) {
            self.lookahead = self.read();
            if let Lookahead::Char('\u{feff}', range) = &self.lookahead {
                // Go's scanner skips a byte order mark at the start. It still
                // counts as a column, so it stands in for the last character.
                self.last = Some(range.clone());
                self.lookahead = self.read();
            }
        }
        match self.lookahead {
            Lookahead::Char(c, _) => Some(c),
            Lookahead::Unread | Lookahead::End => None,
        }
    }

    /// Consumes and returns the next character, or returns `None` at the end.
    fn next(&mut self) -> Option<char> {
        self.peek();
        let Lookahead::Char(c, range) = self.lookahead.clone() else {
            return None;
        };
        self.last = Some(range);
        if c == '\n' {
            self.after_newline = true;
        }
        self.lookahead = self.read();
        Some(c)
    }

    /// Reads the character at the offset as the new lookahead.
    fn read(&mut self) -> Lookahead {
        let start = self.offset;
        let Some(c) = self.pattern[start..].chars().next() else {
            return Lookahead::End;
        };
        self.offset += c.len_utf8();
        if c == '\0' {
            let message = format!(
                "error while scanning glob pattern {}: invalid character NUL",
                quote(self.pattern)
            );
            self.error(message);
        }
        Lookahead::Char(c, start..self.offset)
    }

    /// Records a problem at the last character that [`Validator::next`]
    /// returned.
    fn error(&mut self, message: String) {
        let range = if self.after_newline {
            None
        } else {
            self.last.clone()
        };
        self.problems.push(GlobProblem { message, range });
    }

    /// Records a syntax error about `c`, or about the end of the pattern if
    /// `c` is `None`, found while checking `what`.
    fn unexpected(&mut self, c: Option<char>, what: &str, why: &str) {
        let unexpected = match c {
            Some(c) => format!("unexpected character {}", quote_rune(c)),
            None => "unexpected EOF".to_owned(),
        };
        let while_checking = if what.is_empty() {
            String::new()
        } else {
            format!(" while checking {what}")
        };
        self.error(format!(
            "invalid glob pattern. {unexpected}{while_checking}. {why}"
        ));
    }

    /// Records a character that a Git ref name cannot contain.
    fn invalid_ref_char(&mut self, c: char, why: &str) {
        // The reference prints a printable character as is and quotes any
        // other with `%q`. Only ASCII characters get here, and in ASCII Go's
        // printable characters are the graphic ones and the space.
        let shown = if c.is_ascii_graphic() || c == ' ' {
            format!("'{c}'")
        } else {
            quote_rune(c)
        };
        self.error(format!(
            "character {shown} is invalid for branch and tag names. {why}. see `man git-check-ref-format` for more details. note that regular expression is unavailable"
        ));
    }
}

#[cfg(test)]
mod tests;
