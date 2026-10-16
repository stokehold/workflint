use std::fmt;
use std::ops::Range;

use crate::token::to_usize;

/// A syntax error in an expression: the message and the byte range it is about.
///
/// The range is relative to the string given to the lexer. Callers add the offset of that string
/// in the file and turn the result into a line and a code-point column through the file's line
/// index, as actionlint's columns count code points.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExprError {
    /// The message, byte-identical to actionlint's.
    pub message: String,
    /// The byte offset where the error is: the unexpected character, or the end of the input.
    pub start: u32,
    /// The byte offset one past the unexpected character. Equal to `start` for an error at the
    /// end of the input.
    pub end: u32,
}

impl ExprError {
    /// Returns the error's byte range in the lexer's input.
    #[must_use]
    pub fn range(&self) -> Range<usize> {
        to_usize(self.start)..to_usize(self.end)
    }
}

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ExprError {}
