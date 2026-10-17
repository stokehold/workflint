//! Vendored copy of libyaml-safer 0.3.0, reduced to its event parser.
//!
//! `ORIGIN.md` in this directory records the upstream version and every
//! change workflint made.

// These lints from the pedantic group are allowed here so that the diff
// against upstream stays readable (ADR 0018). The cast lints flag the integer
// conversions between mark fields, indentation levels, and character codes
// that upstream writes with `as`. The others are style lints. The list matches
// the one upstream allows in its own `lib.rs`.
#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::manual_range_contains,
    clippy::missing_errors_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    clippy::unnecessary_wraps
)]

#[macro_use]
mod macros;

mod error;
mod event;
mod parser;
mod reader;
mod scanner;
mod token;

pub(crate) use self::error::Result;
pub use self::error::{Error, ErrorKind, Mark};
pub use self::event::{Event, EventData, NodeMarks};
pub use self::parser::Parser;

pub(crate) const INPUT_BUFFER_SIZE: usize = 16384;

/// The version directive data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct VersionDirective {
    /// The major version number.
    pub major: i32,
    /// The minor version number.
    pub minor: i32,
}

/// The tag directive data.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct TagDirective {
    /// The tag handle.
    pub handle: String,
    /// The tag prefix.
    pub prefix: String,
}

/// The stream encoding.
///
/// The parser reads Rust strings, so the encoding is always UTF-8.
/// [`decode()`](crate::decode()) turns UTF-16 input into a string first.
#[derive(Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[non_exhaustive]
pub enum Encoding {
    /// The UTF-8 encoding.
    #[default]
    Utf8 = 1,
}

/// Where a comment sits on its line.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum CommentKind {
    /// Code precedes the comment on its line, as in `a: 1 # note`.
    Trailing,
    /// Only spaces, tabs, or a byte order mark precede the comment on its
    /// line.
    OwnLine,
}

/// Scalar styles.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[non_exhaustive]
pub enum ScalarStyle {
    /// The plain scalar style.
    Plain = 1,
    /// The single-quoted scalar style.
    SingleQuoted = 2,
    /// The double-quoted scalar style.
    DoubleQuoted = 3,
    /// The literal scalar style.
    Literal = 4,
    /// The folded scalar style.
    Folded = 5,
}

/// Sequence styles.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[non_exhaustive]
pub enum SequenceStyle {
    /// The block sequence style.
    Block = 1,
    /// The flow sequence style.
    Flow = 2,
}

/// Mapping styles.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[non_exhaustive]
pub enum MappingStyle {
    /// The block mapping style.
    Block = 1,
    /// The flow mapping style.
    Flow = 2,
}

#[cfg(test)]
mod tests {
    use super::scanner::Scanner;
    use super::token::{Token, TokenData};
    use super::*;

    #[test]
    #[allow(clippy::unwrap_used)]
    fn scanner_marks() {
        const INPUT: &str = "b:
c: true";
        let mut scanner = Scanner::new(INPUT);
        let mut events = Vec::new();
        loop {
            let token = scanner.peek_mut().unwrap();
            let token = Token {
                data: core::mem::replace(&mut token.data, TokenData::StreamEnd),
                ..*token
            };
            let end = token.data == TokenData::StreamEnd;
            events.push(token);
            scanner.skip_token();
            if end {
                break;
            }
        }
        let expected = &[
            Token {
                data: TokenData::StreamStart {
                    encoding: Encoding::Utf8,
                },
                start_mark: Mark {
                    index: 0,
                    line: 0,
                    column: 0,
                },
                end_mark: Mark {
                    index: 0,
                    line: 0,
                    column: 0,
                },
            },
            Token {
                data: TokenData::BlockMappingStart,
                start_mark: Mark {
                    index: 0,
                    line: 0,
                    column: 0,
                },
                end_mark: Mark {
                    index: 0,
                    line: 0,
                    column: 0,
                },
            },
            Token {
                data: TokenData::Key,
                start_mark: Mark {
                    index: 0,
                    line: 0,
                    column: 0,
                },
                end_mark: Mark {
                    index: 0,
                    line: 0,
                    column: 0,
                },
            },
            Token {
                data: TokenData::Scalar {
                    value: String::from("b"),
                    style: ScalarStyle::Plain,
                },
                start_mark: Mark {
                    index: 0,
                    line: 0,
                    column: 0,
                },
                end_mark: Mark {
                    index: 1,
                    line: 0,
                    column: 1,
                },
            },
            Token {
                data: TokenData::Value,
                start_mark: Mark {
                    index: 1,
                    line: 0,
                    column: 1,
                },
                end_mark: Mark {
                    index: 2,
                    line: 0,
                    column: 2,
                },
            },
            Token {
                data: TokenData::Key,
                start_mark: Mark {
                    index: 3,
                    line: 1,
                    column: 0,
                },
                end_mark: Mark {
                    index: 3,
                    line: 1,
                    column: 0,
                },
            },
            Token {
                data: TokenData::Scalar {
                    value: String::from("c"),
                    style: ScalarStyle::Plain,
                },
                start_mark: Mark {
                    index: 3,
                    line: 1,
                    column: 0,
                },
                end_mark: Mark {
                    index: 4,
                    line: 1,
                    column: 1,
                },
            },
            Token {
                data: TokenData::Value,
                start_mark: Mark {
                    index: 4,
                    line: 1,
                    column: 1,
                },
                end_mark: Mark {
                    index: 5,
                    line: 1,
                    column: 2,
                },
            },
            Token {
                data: TokenData::Scalar {
                    value: String::from("true"),
                    style: ScalarStyle::Plain,
                },
                start_mark: Mark {
                    index: 6,
                    line: 1,
                    column: 3,
                },
                end_mark: Mark {
                    index: 10,
                    line: 1,
                    column: 7,
                },
            },
            Token {
                data: TokenData::BlockEnd,
                start_mark: Mark {
                    index: 10,
                    line: 2,
                    column: 0,
                },
                end_mark: Mark {
                    index: 10,
                    line: 2,
                    column: 0,
                },
            },
            Token {
                data: TokenData::StreamEnd,
                start_mark: Mark {
                    index: 10,
                    line: 2,
                    column: 0,
                },
                end_mark: Mark {
                    index: 10,
                    line: 2,
                    column: 0,
                },
            },
        ];
        assert_eq!(events, expected);
    }
}
