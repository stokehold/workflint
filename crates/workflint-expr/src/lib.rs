//! The `${{ }}` expression language of workflint.
//!
//! This is an internal crate with no stability promise. It has the lexer so far: [`Lexer`]
//! turns the text after `${{` into [`Token`]s up to the `}}` that ends the expression, and
//! [`lex_expression`] collects them. Tokens are kinds and byte ranges, so lexing allocates
//! nothing per token. Errors are [`ExprError`]s with actionlint's messages and byte ranges.
//! Syntax errors are diagnostics of [`rules::INVALID_EXPRESSION_SYNTAX`].
//!
//! ```
//! use workflint_expr::{Lexer, TokenKind};
//!
//! let src = " matrix.os }} and more";
//! let mut lexer = Lexer::new(src);
//! let mut texts = Vec::new();
//! loop {
//!     let token = lexer.next_token();
//!     texts.push(&src[token.range()]);
//!     if token.kind == TokenKind::End {
//!         break;
//!     }
//! }
//! assert!(lexer.error().is_none());
//! assert_eq!(texts, ["matrix", ".", "os", "}}"]);
//! assert_eq!(lexer.offset(), 13);
//! ```

mod error;
mod lexer;
pub mod rules;
mod token;

pub use error::ExprError;
pub use lexer::{Lexer, lex_expression};
pub use token::{Token, TokenKind};
