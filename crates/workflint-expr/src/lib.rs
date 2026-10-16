//! The `${{ }}` expression language of workflint.
//!
//! This is an internal crate with no stability promise. It has the lexer and the type system
//! so far.
//!
//! [`Lexer`] turns the text after `${{` into [`Token`]s up to the `}}` that ends the
//! expression, and [`lex_expression`] collects them. Tokens are kinds and byte ranges, so
//! lexing allocates nothing per token. Errors are [`ExprError`]s with actionlint's messages and
//! byte ranges. Syntax errors are diagnostics of [`rules::INVALID_EXPRESSION_SYNTAX`].
//!
//! [`Ty`] is the type of an expression value, with actionlint's assignability, merge, and
//! printed form. Types are immutable and can cross threads. `Ty::from` converts a type from
//! `workflint-data`.
//!
//! ```
//! use workflint_expr::Ty;
//!
//! let ty = Ty::strict([("os", Ty::String), ("count", Ty::Number)]);
//! assert_eq!(ty.to_string(), "{count: number; os: string}");
//! assert!(Ty::String.accepts(&Ty::Number));
//! assert!(!Ty::Bool.accepts(&Ty::String));
//! assert_eq!(Ty::Number.merge(&Ty::String), Ty::String);
//! ```
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
mod types;

pub use error::ExprError;
pub use lexer::{Lexer, lex_expression};
pub use token::{Token, TokenKind};
pub use types::{ArrayTy, ObjectTy, Rest, Ty};
