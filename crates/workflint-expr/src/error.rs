use std::fmt;
use std::ops::Range;

use workflint_diagnostics::RuleMeta;

use crate::token::to_usize;

/// An error in an expression: the message, the byte range it is about, and its rule.
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
    /// The rule the error belongs to: [`INVALID_NUMBER_LITERAL`] for a number literal beyond
    /// the range of a double, and [`INVALID_EXPRESSION_SYNTAX`] for every other error.
    ///
    /// [`INVALID_NUMBER_LITERAL`]: crate::rules::INVALID_NUMBER_LITERAL
    /// [`INVALID_EXPRESSION_SYNTAX`]: crate::rules::INVALID_EXPRESSION_SYNTAX
    pub rule: &'static RuleMeta,
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
