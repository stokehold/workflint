pub(crate) type Result<T, E = Error> = core::result::Result<T, E>;

/// A position in the input.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Mark {
    /// The byte offset into the input, including a leading byte order mark.
    pub index: u64,
    /// The 0-based line.
    pub line: u64,
    /// The 0-based column, counted in Unicode code points.
    pub column: u64,
}

impl std::fmt::Display for Mark {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {} column {}", self.line + 1, self.column + 1)
    }
}

#[derive(Debug)]
#[allow(clippy::struct_field_names)]
struct Problem {
    pub problem: &'static str,
    pub problem_mark: Mark,
    pub context: &'static str,
    pub context_mark: Mark,
}

#[derive(Debug)]
enum ErrorImpl {
    Reader {
        problem: &'static str,
        offset: usize,
        value: i32,
    },
    Scanner(Problem),
    Parser(Problem),
}

/// The stage of the parser that reported an [`Error`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The input contains a character that YAML does not allow.
    Reader,
    /// The input cannot be split into tokens.
    Scanner,
    /// The tokens do not form a valid YAML document.
    Parser,
}

/// A YAML syntax error, with libyaml's problem and context messages.
#[derive(Debug)]
pub struct Error(Box<ErrorImpl>);

impl Error {
    pub(crate) fn reader(problem: &'static str, offset: usize, value: i32) -> Self {
        Self(Box::new(ErrorImpl::Reader {
            problem,
            offset,
            value,
        }))
    }

    pub(crate) fn scanner(
        context: &'static str,
        context_mark: Mark,
        problem: &'static str,
        problem_mark: Mark,
    ) -> Self {
        Self(Box::new(ErrorImpl::Scanner(Problem {
            problem,
            problem_mark,
            context,
            context_mark,
        })))
    }

    pub(crate) fn parser(
        context: &'static str,
        context_mark: Mark,
        problem: &'static str,
        problem_mark: Mark,
    ) -> Self {
        Self(Box::new(ErrorImpl::Parser(Problem {
            problem,
            problem_mark,
            context,
            context_mark,
        })))
    }

    /// An error for a broken invariant of the scanner or parser, returned
    /// where upstream panicked. No input is known to cause it.
    pub(crate) fn internal(mark: Mark) -> Self {
        Self::scanner("", Mark::default(), "found an internal parser error", mark)
    }

    /// The stage of the parser that reported the error.
    pub fn kind(&self) -> ErrorKind {
        match &*self.0 {
            ErrorImpl::Reader { .. } => ErrorKind::Reader,
            ErrorImpl::Scanner(_) => ErrorKind::Scanner,
            ErrorImpl::Parser(_) => ErrorKind::Parser,
        }
    }

    /// Where the problem is. Reader errors have only a byte offset.
    pub fn problem_mark(&self) -> Option<Mark> {
        match &*self.0 {
            ErrorImpl::Reader { .. } => None,
            ErrorImpl::Scanner(p) | ErrorImpl::Parser(p) => Some(p.problem_mark),
        }
    }

    /// The byte offset of the problem in the input, a workflint addition.
    /// Unlike [`Error::problem_mark`], it exists for reader errors too.
    pub fn problem_offset(&self) -> u64 {
        match &*self.0 {
            ErrorImpl::Reader { offset, .. } => *offset as u64,
            ErrorImpl::Scanner(p) | ErrorImpl::Parser(p) => p.problem_mark.index,
        }
    }

    /// Where the construct that contains the problem starts, if the error has
    /// a context.
    pub fn context_mark(&self) -> Option<Mark> {
        match &*self.0 {
            ErrorImpl::Reader { .. } => None,
            ErrorImpl::Scanner(p) | ErrorImpl::Parser(p) => {
                if p.context.is_empty() {
                    None
                } else {
                    Some(p.context_mark)
                }
            }
        }
    }

    /// The problem message, for example `"found unexpected end of stream"`.
    pub fn problem(&self) -> &'static str {
        match &*self.0 {
            ErrorImpl::Reader { problem, .. } => problem,
            ErrorImpl::Scanner(p) | ErrorImpl::Parser(p) => p.problem,
        }
    }

    /// The context message, for example `"while scanning a quoted scalar"`.
    pub fn context(&self) -> Option<&'static str> {
        match &*self.0 {
            ErrorImpl::Reader { .. } => None,
            ErrorImpl::Scanner(p) | ErrorImpl::Parser(p) => {
                if p.context.is_empty() {
                    None
                } else {
                    Some(p.context)
                }
            }
        }
    }
}

impl std::error::Error for Error {}

impl core::fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ErrorKind::Reader => "Reader",
            ErrorKind::Scanner => "Scanner",
            ErrorKind::Parser => "Parser",
        })
    }
}

impl core::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            problem,
            problem_mark,
            context,
            context_mark,
        } = self;

        if self.context.is_empty() {
            write!(f, "{problem_mark}: {problem}")
        } else {
            write!(f, "{problem_mark}: {problem} {context} ({context_mark})")
        }
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} error: ", self.kind())?;
        match *self.0 {
            ErrorImpl::Reader {
                problem,
                offset,
                value,
            } => write!(f, "{problem} (offset {offset}, value {value})"),
            ErrorImpl::Scanner(ref p) | ErrorImpl::Parser(ref p) => {
                write!(f, "{p}")
            }
        }
    }
}
