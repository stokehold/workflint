//! The `${{ }}` expression language of workflint.
//!
//! This is an internal crate with no stability promise. It has the lexer, the parser, the
//! type system, and the type checker.
//!
//! [`Lexer`] turns the text after `${{` into [`Token`]s up to the `}}` that ends the
//! expression, and [`lex_expression`] collects them. Tokens are kinds and byte ranges, so
//! lexing allocates nothing per token. [`parse_expression`] builds the syntax tree, an
//! [`Expr`], with GitHub's operator precedence. Errors are [`ExprError`]s with actionlint's
//! messages and byte ranges. They are diagnostics of [`rules::INVALID_EXPRESSION_SYNTAX`], or
//! of [`rules::INVALID_NUMBER_LITERAL`] for a number beyond the range of a double.
//!
//! [`Ty`] is the type of an expression value, with actionlint's assignability, merge, and
//! printed form. Types are immutable and can cross threads. `Ty::from` converts a type from
//! `workflint-data`.
//!
//! [`TypeEnv`] holds the types of the contexts at one place in a workflow, and
//! [`TypeEnv::check`] type-checks an [`Expr`] against them. It returns the expression's type
//! and actionlint's semantic errors, such as an undefined variable or property or a call of a
//! built-in function with the wrong arguments, each with its rule from [`rules`].
//! [`TypeEnv::set_workflow_key`] limits the contexts and special functions to those that a
//! workflow key allows.
//! [`Expr::is_constant`] tells whether an expression has the same value in every run.
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

mod ast;
mod check;
mod env;
mod error;
mod format;
mod functions;
mod lexer;
mod parser;
pub mod rules;
mod token;
mod types;

pub use ast::{CompareOp, Expr, ExprKind, LogicalOp, Name, StringLit, Visit};
pub use env::TypeEnv;
pub use error::ExprError;
pub use lexer::{Lexer, lex_expression};
pub use parser::parse_expression;
pub use token::{Token, TokenKind};
pub use types::{ArrayTy, ObjectTy, Rest, Ty};
