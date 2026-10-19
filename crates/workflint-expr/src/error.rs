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
    /// The byte offset where the error starts. A lexer error starts at the unexpected
    /// character or at the end of the input, a parser error at the unexpected token, and an
    /// error of [`TypeEnv::check`](crate::TypeEnv::check) at the token that actionlint reports
    /// it at.
    pub start: u32,
    /// The byte offset one past the end of what the error is about: the character, the token,
    /// or the node. Equal to `start` for an error at the end of the input.
    pub end: u32,
    /// The rule the error belongs to. Syntax errors belong to [`INVALID_NUMBER_LITERAL`] or
    /// [`INVALID_EXPRESSION_SYNTAX`], and the errors of `TypeEnv::check` to the other rules in
    /// [`rules`](crate::rules).
    ///
    /// [`INVALID_NUMBER_LITERAL`]: crate::rules::INVALID_NUMBER_LITERAL
    /// [`INVALID_EXPRESSION_SYNTAX`]: crate::rules::INVALID_EXPRESSION_SYNTAX
    pub rule: &'static RuleMeta,
    /// A hint that the actionlint-compatible output formats do not print, such as the closest
    /// known name to an undefined one.
    pub help: Option<String>,
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
