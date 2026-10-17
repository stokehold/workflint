use workflint_diagnostics::RuleMeta;
use workflint_diagnostics::quote::{quote, quotes};

use crate::ast::{CompareOp, Expr, ExprKind, LogicalOp, Name, StringLit};
use crate::error::ExprError;
use crate::lexer::Lexer;
use crate::rules::{INVALID_EXPRESSION_SYNTAX, INVALID_NUMBER_LITERAL};
use crate::token::{Token, TokenKind};

/// The deepest nesting the parser accepts. Each parenthesis, `!`, function call, property
/// access, object filter, index access, and binary operator adds a level, so that neither the
/// parser nor a recursive walk of the tree can overflow the stack.
const MAX_DEPTH: u32 = 128;

const EXPECTED_PRIMARY: &str = "variable access, function call, null, bool, int, float or string";

/// The precedence level of `&&`. `&&` and `||` group to the right, and the operators above
/// them group to the left.
const AND_LEVEL: u8 = 2;

/// Parses the expression at the start of `src` up to and including the `}}` that ends it.
///
/// Returns the tree and the byte offset just past `}}`. The precedence of the operators, from
/// the tightest, is: property access, object filters, and index access; `!`; `<`, `<=`, `>`,
/// and `>=`; `==` and `!=`; `&&`; `||`. Comparisons group to the left, as in GitHub's parser,
/// and `&&` and `||` group to the right, as in actionlint's.
///
/// # Errors
///
/// Returns the first error. A lexer error wins over the parser error it causes, as in
/// actionlint. Number literals beyond the range of a double are errors of
/// [`INVALID_NUMBER_LITERAL`], and every other error belongs to
/// [`INVALID_EXPRESSION_SYNTAX`].
///
/// ```
/// use workflint_expr::{ExprKind, parse_expression};
///
/// let src = "github.ref == 'main' }} rest";
/// let (expr, end) = parse_expression(src).unwrap();
/// assert!(matches!(expr.kind, ExprKind::Compare { .. }));
/// assert_eq!(&src[..end as usize], "github.ref == 'main' }}");
///
/// let err = parse_expression("github. }}").unwrap_err();
/// assert_eq!(err.start, 8);
/// assert!(err.message.starts_with("unexpected end of input while parsing object property"));
/// ```
pub fn parse_expression(src: &str) -> Result<(Expr, u32), ExprError> {
    Parser::new(src).parse()
}

/// A recursive-descent parser with precedence climbing for binary operators, ported from
/// actionlint's `ExprParser`. Like actionlint's, it pulls one token at a time from the lexer
/// and stops at the first error.
struct Parser<'src> {
    src: &'src str,
    lexer: Lexer<'src>,
    /// The next token, not yet consumed.
    cur: Token,
    /// The byte offset one past the last consumed token.
    prev_end: u32,
    /// The current nesting depth, at most [`MAX_DEPTH`].
    depth: u32,
    error: Option<ExprError>,
}

impl<'src> Parser<'src> {
    fn new(src: &'src str) -> Self {
        let mut lexer = Lexer::new(src);
        let cur = lexer.next_token();
        Self {
            src,
            lexer,
            cur,
            prev_end: 0,
            depth: 0,
            error: None,
        }
    }

    fn parse(mut self) -> Result<(Expr, u32), ExprError> {
        let root = self.binary(1);
        if let Some(err) = self.lexer.error() {
            return Err(err.clone());
        }
        let Some(root) = root else {
            return Err(self.into_error());
        };
        if self.cur.kind != TokenKind::End {
            // The reference lists the remaining tokens up to the end marker or the first lexer
            // error, and reports this error rather than the lexer error.
            let mut kinds = vec![self.cur.kind.as_str()];
            loop {
                let token = self.lexer.next_token();
                if token.kind == TokenKind::End {
                    break;
                }
                kinds.push(token.kind.as_str());
            }
            self.error(&INVALID_EXPRESSION_SYNTAX, || {
                format!(
                    "parser did not reach end of input after parsing the expression. {} \
                     remaining token(s) in the input: {}",
                    kinds.len(),
                    quotes(&kinds)
                )
            });
            return Err(self.into_error());
        }
        Ok((root, self.lexer.offset()))
    }

    /// Returns the recorded error. Every parse function records an error before it returns
    /// `None`, so the fallback is never used.
    fn into_error(self) -> ExprError {
        self.error.unwrap_or_else(|| ExprError {
            message: "parser stopped without an error".to_owned(),
            start: self.cur.start,
            end: self.cur.end,
            rule: &INVALID_EXPRESSION_SYNTAX,
        })
    }

    /// Consumes the next token and returns it.
    fn next(&mut self) -> Token {
        let token = self.cur;
        self.prev_end = token.end;
        self.cur = self.lexer.next_token();
        token
    }

    fn text(&self, token: Token) -> &'src str {
        self.src.get(token.range()).unwrap_or_default()
    }

    /// Records an error at the next token, unless an earlier error was recorded.
    fn error(&mut self, rule: &'static RuleMeta, message: impl FnOnce() -> String) {
        if self.error.is_none() {
            self.error = Some(ExprError {
                message: message(),
                start: self.cur.start,
                end: self.cur.end,
                rule,
            });
        }
    }

    /// Records that the next token is not one of `expected` and returns `None`.
    fn unexpected<T>(&mut self, where_: &str, expected: &[TokenKind]) -> Option<T> {
        let what = if self.cur.kind == TokenKind::End {
            "end of input".to_owned()
        } else {
            format!("token {}", quote(self.cur.kind.as_str()))
        };
        let expected = quotes(expected.iter().map(|kind| kind.as_str()));
        self.error(&INVALID_EXPRESSION_SYNTAX, || {
            format!("unexpected {what} while parsing {where_}. expecting {expected}")
        });
        None
    }

    /// Adds a nesting level, or records an error if that exceeds [`MAX_DEPTH`]. The caller
    /// removes the level on success. After an error the parse stops, so the depth no longer
    /// matters.
    fn enter(&mut self) -> Option<()> {
        if self.depth >= MAX_DEPTH {
            self.error(&INVALID_EXPRESSION_SYNTAX, || {
                format!("expression is nested more than {MAX_DEPTH} levels deep")
            });
            return None;
        }
        self.depth += 1;
        Some(())
    }

    /// Parses binary operators of level `min` and above, and their operands.
    fn binary(&mut self, min: u8) -> Option<Expr> {
        let start = self.cur.start;
        let mut left = self.unary()?;
        let mut levels = 0;
        while let Some((op, level)) = binary_op(self.cur.kind) {
            if level < min {
                break;
            }
            self.enter()?;
            levels += 1;
            self.next();
            let right_min = if level <= AND_LEVEL { level } else { level + 1 };
            let right = Box::new(self.binary(right_min)?);
            let token = left.token;
            let left_box = Box::new(left);
            let kind = match op {
                BinaryOp::Compare(op) => ExprKind::Compare {
                    op,
                    left: left_box,
                    right,
                },
                BinaryOp::Logical(op) => ExprKind::Logical {
                    op,
                    left: left_box,
                    right,
                },
            };
            left = self.node(kind, start, token);
        }
        self.depth -= levels;
        Some(left)
    }

    fn unary(&mut self) -> Option<Expr> {
        if self.cur.kind != TokenKind::Not {
            return self.postfix();
        }
        self.enter()?;
        let token = self.next();
        let operand = Box::new(self.unary()?);
        self.depth -= 1;
        Some(self.node(ExprKind::Not { operand }, token.start, token))
    }

    fn postfix(&mut self) -> Option<Expr> {
        let start = self.cur.start;
        let mut expr = self.primary()?;
        let mut levels = 0;
        loop {
            // The reference reports errors about the whole chain at its first token.
            let token = expr.token;
            let kind = match self.cur.kind {
                TokenKind::Dot => {
                    self.enter()?;
                    levels += 1;
                    self.next();
                    let receiver = Box::new(expr);
                    match self.cur.kind {
                        TokenKind::Star => {
                            self.next();
                            ExprKind::ObjectFilter { receiver }
                        }
                        TokenKind::Ident => {
                            let token = self.next();
                            let name = Name::new(self.text(token), token.start, token.end);
                            ExprKind::Property { receiver, name }
                        }
                        _ => {
                            return self.unexpected(
                                "object property dereference like 'a.b' or array element \
                                 dereference like 'a.*'",
                                &[TokenKind::Ident, TokenKind::Star],
                            );
                        }
                    }
                }
                TokenKind::LeftBracket => {
                    self.enter()?;
                    levels += 1;
                    self.next();
                    let index = Box::new(self.binary(1)?);
                    if self.cur.kind != TokenKind::RightBracket {
                        return self.unexpected(
                            "closing bracket ']' for index access",
                            &[TokenKind::RightBracket],
                        );
                    }
                    self.next();
                    ExprKind::Index {
                        operand: Box::new(expr),
                        index,
                    }
                }
                _ => break,
            };
            expr = self.node(kind, start, token);
        }
        self.depth -= levels;
        Some(expr)
    }

    fn primary(&mut self) -> Option<Expr> {
        match self.cur.kind {
            TokenKind::Ident => self.ident(),
            TokenKind::LeftParen => {
                self.enter()?;
                self.next();
                let nested = self.binary(1)?;
                if self.cur.kind != TokenKind::RightParen {
                    return self.unexpected(
                        "closing ')' of nested expression (...)",
                        &[TokenKind::RightParen],
                    );
                }
                self.next();
                self.depth -= 1;
                Some(nested)
            }
            TokenKind::Int => self.number(TokenKind::Int),
            TokenKind::Float => self.number(TokenKind::Float),
            TokenKind::String => {
                let token = self.next();
                let text = self.text(token);
                // The lexer keeps a leading byte order mark in the first token.
                let text = text.strip_prefix('\u{feff}').unwrap_or(text);
                let text = text.strip_prefix('\'').unwrap_or(text);
                let text = text.strip_suffix('\'').unwrap_or(text);
                let lit = StringLit::new(text.replace("''", "'"));
                Some(self.node(ExprKind::String(lit), token.start, token))
            }
            _ => self.unexpected(
                EXPECTED_PRIMARY,
                &[
                    TokenKind::Ident,
                    TokenKind::LeftParen,
                    TokenKind::Int,
                    TokenKind::Float,
                    TokenKind::String,
                ],
            ),
        }
    }

    fn ident(&mut self) -> Option<Expr> {
        let token = self.next();
        let text = self.text(token);
        if self.cur.kind != TokenKind::LeftParen {
            // Keywords are case-sensitive: `TRUE` is a variable.
            let kind = match text {
                "null" => ExprKind::Null,
                "true" => ExprKind::Bool(true),
                "false" => ExprKind::Bool(false),
                _ => ExprKind::Variable(Name::new(text, token.start, token.end)),
            };
            return Some(self.node(kind, token.start, token));
        }

        // Only a bare name can be called, so a call is a primary expression.
        self.enter()?;
        self.next();
        let mut args = Vec::new();
        if self.cur.kind == TokenKind::RightParen {
            self.next();
        } else {
            loop {
                args.push(self.binary(1)?);
                match self.cur.kind {
                    TokenKind::Comma => {
                        self.next();
                    }
                    TokenKind::RightParen => {
                        self.next();
                        break;
                    }
                    _ => {
                        return self.unexpected(
                            "arguments of function call",
                            &[TokenKind::Comma, TokenKind::RightParen],
                        );
                    }
                }
            }
        }
        self.depth -= 1;
        let callee = Name::new(text, token.start, token.end);
        Some(self.node(ExprKind::Call { callee, args }, token.start, token))
    }

    fn number(&mut self, kind: TokenKind) -> Option<Expr> {
        let token = self.cur;
        let text = self.text(token);
        let (value, what, func) = if kind == TokenKind::Int {
            (int_value(text), "integer", "ParseInt")
        } else {
            (float_value(text), "float", "ParseFloat")
        };
        match value {
            Ok(value) => {
                self.next();
                let kind = if kind == TokenKind::Int {
                    ExprKind::Int(value)
                } else {
                    ExprKind::Float(value)
                };
                Some(self.node(kind, token.start, token))
            }
            Err(reason) => {
                self.error(&INVALID_NUMBER_LITERAL, || {
                    let text = quote(text);
                    format!(
                        "parsing invalid {what} literal {text}: strconv.{func}: parsing {text}: \
                         {reason}"
                    )
                });
                None
            }
        }
    }

    /// Returns a node from `start` to the end of the last consumed token.
    fn node(&self, kind: ExprKind, start: u32, token: Token) -> Expr {
        Expr {
            kind,
            start,
            end: self.prev_end,
            token,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum BinaryOp {
    Compare(CompareOp),
    Logical(LogicalOp),
}

/// Returns the binary operator of a token kind and its precedence level. A higher level binds
/// tighter.
fn binary_op(kind: TokenKind) -> Option<(BinaryOp, u8)> {
    let op = match kind {
        TokenKind::Or => (BinaryOp::Logical(LogicalOp::Or), 1),
        TokenKind::And => (BinaryOp::Logical(LogicalOp::And), AND_LEVEL),
        TokenKind::Eq => (BinaryOp::Compare(CompareOp::Eq), 3),
        TokenKind::NotEq => (BinaryOp::Compare(CompareOp::NotEq), 3),
        TokenKind::Less => (BinaryOp::Compare(CompareOp::Less), 4),
        TokenKind::LessEq => (BinaryOp::Compare(CompareOp::LessEq), 4),
        TokenKind::Greater => (BinaryOp::Compare(CompareOp::Greater), 4),
        TokenKind::GreaterEq => (BinaryOp::Compare(CompareOp::GreaterEq), 4),
        _ => return None,
    };
    Some(op)
}

const INVALID_SYNTAX: &str = "invalid syntax";
const OUT_OF_RANGE: &str = "value out of range";

/// Returns the value of an integer literal as the nearest double, or the reason, in Go's
/// `strconv` words, why it has none.
fn int_value(text: &str) -> Result<f64, &'static str> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(digits) => (true, digits),
        None => (false, text),
    };
    let value = match digits.strip_prefix("0x") {
        Some(hex) => hex_value(hex)?,
        None if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) => {
            digits.parse::<f64>().map_err(|_| INVALID_SYNTAX)?
        }
        None => return Err(INVALID_SYNTAX),
    };
    if value.is_infinite() {
        return Err(OUT_OF_RANGE);
    }
    Ok(if negative { -value } else { value })
}

/// Returns the value of hex digits as the nearest double, which is infinite beyond the range
/// of a double.
fn hex_value(hex: &str) -> Result<f64, &'static str> {
    if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(INVALID_SYNTAX);
    }
    let hex = hex.trim_start_matches('0');
    // The first 32 digits fit a `u128`. A conversion from `u128` rounds to the nearest double,
    // so set its lowest bit if any later digit is not zero, to round up a value that is just
    // above a halfway point. That bit is far below the 53 bits a double keeps.
    let (top, rest) = hex.split_at(hex.len().min(32));
    let mut mantissa = u128::from_str_radix(top, 16).unwrap_or(0);
    if rest.bytes().any(|b| b != b'0') {
        mantissa |= 1;
    }
    #[allow(clippy::cast_precision_loss)]
    let mantissa = mantissa as f64;
    let shift = i32::try_from(rest.len().saturating_mul(4)).unwrap_or(i32::MAX);
    Ok(mantissa * 2f64.powi(shift))
}

/// Returns the value of a float literal, or the reason, in Go's `strconv` words, why it has
/// none.
fn float_value(text: &str) -> Result<f64, &'static str> {
    let value = text.parse::<f64>().map_err(|_| INVALID_SYNTAX)?;
    if value.is_infinite() {
        return Err(OUT_OF_RANGE);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `TestParseReturnFirstErrorOnMultipleErrors`: a later error does not replace the first.
    #[test]
    fn later_error_keeps_the_first_error() {
        let mut parser = Parser::new(".}}");
        assert!(parser.binary(1).is_none());
        let first = parser.error.clone();
        assert!(first.is_some());
        let _: Option<()> = parser.unexpected("foo", &[]);
        assert_eq!(parser.error, first);
    }

    /// `TestParseExpressionNumberLiteralsError`: number tokens with the text `abc` are invalid
    /// literals. The lexer never returns such a token, so the test changes the kind of an
    /// identifier.
    #[test]
    fn number_token_that_is_not_a_number_is_an_invalid_literal() {
        for (kind, message) in [
            (
                TokenKind::Int,
                "parsing invalid integer literal \"abc\": strconv.ParseInt: parsing \"abc\": \
                 invalid syntax",
            ),
            (
                TokenKind::Float,
                "parsing invalid float literal \"abc\": strconv.ParseFloat: parsing \"abc\": \
                 invalid syntax",
            ),
        ] {
            let mut parser = Parser::new("abc}}");
            parser.cur.kind = kind;
            assert!(parser.number(kind).is_none());
            let err = parser.into_error();
            assert_eq!(err.message, message);
            assert_eq!((err.start, err.end), (0, 3));
            assert_eq!(err.rule, &INVALID_NUMBER_LITERAL);
        }
    }

    #[test]
    fn number_text_that_is_not_a_number_is_invalid_syntax() {
        assert_eq!(int_value("abc"), Err(INVALID_SYNTAX));
        assert_eq!(float_value("abc"), Err(INVALID_SYNTAX));
        assert_eq!(int_value("-"), Err(INVALID_SYNTAX));
        assert_eq!(int_value("0x"), Err(INVALID_SYNTAX));
    }

    #[test]
    fn hex_value_ignores_leading_zeros() {
        assert_eq!(hex_value("000ff"), Ok(255.0));
        assert_eq!(hex_value("0"), Ok(0.0));
    }
}
