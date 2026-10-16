//! The `${{ }}` expression language of workflint.
//!
//! This is an internal crate with no stability promise. It has the lexer and the parser so
//! far. [`Lexer`] turns the text after `${{` into [`Token`]s up to the `}}` that ends the
//! expression, and [`lex_expression`] collects them. Tokens are kinds and byte ranges, so
//! lexing allocates nothing per token. [`parse_expression`] builds the syntax tree, an
//! [`Expr`], with GitHub's operator precedence. Errors are [`ExprError`]s with actionlint's
//! messages and byte ranges. They are diagnostics of [`rules::INVALID_EXPRESSION_SYNTAX`], or
//! of [`rules::INVALID_NUMBER_LITERAL`] for a number beyond the range of a double.
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

mod ast;
mod error;
mod lexer;
mod parser;
pub mod rules;
mod token;

pub use ast::{CompareOp, Expr, ExprKind, LogicalOp, Name, StringLit, Visit};
pub use error::ExprError;
pub use lexer::{Lexer, lex_expression};
pub use parser::parse_expression;
pub use token::{Token, TokenKind};
