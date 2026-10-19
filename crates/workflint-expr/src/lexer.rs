use workflint_diagnostics::quote::quote_rune;

use crate::error::ExprError;
use crate::rules::INVALID_EXPRESSION_SYNTAX;
use crate::token::{Token, TokenKind};

const BOM: char = '\u{feff}';

const EXPECTED_DIGITS: &str = "'0'..'9'";
const EXPECTED_HEX_DIGITS: &str = "'0'..'9', 'a'..'f', 'A'..'F'";
const EXPECTED_PUNCT: &str =
    "''', '}', '(', ')', '[', ']', '.', '!', '<', '>', '=', '&', '|', '*', ',', ' '";
const EXPECTED_ALL: &str = "'a'..'z', 'A'..'Z', '_', '0'..'9', ''', '}', '(', ')', '[', ']', \
                            '.', '!', '<', '>', '=', '&', '|', '*', ',', ' '";

/// A lexer for one expression, from just after `${{` to the `}}` that ends it.
///
/// [`Lexer::next_token`] returns one token per call. The `}}` that ends the expression is a
/// [`TokenKind::End`] token, and [`Lexer::offset`] then returns the byte offset just past it, so
/// the caller knows where the expression ends in its string. A `}}` inside a string literal does
/// not end the expression. Reaching the end of the input before `}}` is an error.
///
/// The lexer keeps only the first error, available from [`Lexer::error`]. After an error
/// other than a NUL character, every call returns an empty [`TokenKind::End`] token.
///
/// The lexer works like actionlint's, which is built on Go's `text/scanner`:
///
/// - A byte order mark at the start of the input is skipped but stays in the first token.
/// - A NUL character is an error, reported when the lexer reaches it. The lexer goes on: the
///   token being lexed completes, and the next calls return the tokens after it. A NUL
///   outside a string literal is then an unexpected character.
/// - Unlike actionlint's, the exponent of a number accepts `+`, as in JSON.
///
/// The lexer reads at most `u32::MAX` bytes of its input, because tokens store `u32` offsets.
#[derive(Debug, Clone)]
pub struct Lexer<'src> {
    src: &'src str,
    /// The offset of the next character to read.
    pos: usize,
    /// The offset where the current token starts.
    start: usize,
    error: Option<ExprError>,
    /// Whether an error other than a NUL character stopped the lexer.
    stopped: bool,
}

impl<'src> Lexer<'src> {
    /// Creates a lexer for the expression that starts at the beginning of `src`.
    #[must_use]
    pub fn new(src: &'src str) -> Self {
        let max = usize::try_from(u32::MAX).unwrap_or(usize::MAX);
        let src = src.get(..src.floor_char_boundary(max)).unwrap_or(src);
        let mut lexer = Self {
            src,
            pos: 0,
            start: 0,
            error: None,
            stopped: false,
        };
        if src.starts_with(BOM) {
            lexer.pos = BOM.len_utf8();
        }
        lexer.check_nul();
        lexer
    }

    /// Lexes and returns the next token.
    pub fn next_token(&mut self) -> Token {
        if self.stopped {
            return self.eof();
        }
        self.skip_white();
        let Some(c) = self.peek() else {
            return self.unexpected_eof();
        };
        match c {
            'a'..='z' | 'A'..='Z' | '_' => self.lex_ident(),
            '0'..='9' | '-' => self.lex_num(),
            '\'' => self.lex_string(),
            '}' => self.lex_end(),
            '!' => self.lex_maybe_eq(TokenKind::Not, TokenKind::NotEq),
            '<' => self.lex_maybe_eq(TokenKind::Less, TokenKind::LessEq),
            '>' => self.lex_maybe_eq(TokenKind::Greater, TokenKind::GreaterEq),
            '=' => self.lex_double('=', TokenKind::Eq, "== operator", "'='"),
            '&' => self.lex_double('&', TokenKind::And, "&& operator", "'&'"),
            '|' => self.lex_double('|', TokenKind::Or, "|| operator", "'|'"),
            '(' => self.lex_char(TokenKind::LeftParen),
            ')' => self.lex_char(TokenKind::RightParen),
            '[' => self.lex_char(TokenKind::LeftBracket),
            ']' => self.lex_char(TokenKind::RightBracket),
            '.' => self.lex_char(TokenKind::Dot),
            '*' => self.lex_char(TokenKind::Star),
            ',' => self.lex_char(TokenKind::Comma),
            _ => self.unexpected(Some(c), "expression", EXPECTED_ALL),
        }
    }

    /// Returns the byte offset of the next character to read. After a [`TokenKind::End`] token
    /// that is not an error, it is the offset just past `}}`.
    #[must_use]
    pub fn offset(&self) -> u32 {
        to_u32(self.pos)
    }

    /// Returns the first error, if there was one.
    #[must_use]
    pub fn error(&self) -> Option<&ExprError> {
        self.error.as_ref()
    }

    /// Consumes the lexer and returns the first error, if there was one.
    #[must_use]
    pub fn into_error(self) -> Option<ExprError> {
        self.error
    }

    /// Returns the next character without reading it.
    fn peek(&self) -> Option<char> {
        let rest = self.src.get(self.pos..)?;
        match rest.as_bytes().first() {
            Some(&b) if b.is_ascii() => Some(char::from(b)),
            _ => rest.chars().next(),
        }
    }

    /// Reads the next character and returns the one after it without reading it.
    fn eat(&mut self) -> Option<char> {
        if let Some(c) = self.peek() {
            self.pos += c.len_utf8();
            self.check_nul();
        }
        self.peek()
    }

    /// Reads digits while the next character is one, and returns the first other character.
    fn eat_digits(&mut self) -> Option<char> {
        loop {
            let c = self.eat();
            if !is_digit(c) {
                return c;
            }
        }
    }

    /// Reports an error if the next character is NUL, as `text/scanner` does when it reads one.
    fn check_nul(&mut self) {
        if self.src.as_bytes().get(self.pos) == Some(&0) {
            self.report(|| "scan error while lexing expression: invalid character NUL".to_owned());
        }
    }

    /// Records an error at the next character, unless an earlier error was recorded.
    fn report(&mut self, message: impl FnOnce() -> String) {
        if self.error.is_none() {
            let len = self.peek().map_or(0, char::len_utf8);
            self.error = Some(ExprError {
                message: message(),
                start: to_u32(self.pos),
                end: to_u32(self.pos + len),
                rule: &INVALID_EXPRESSION_SYNTAX,
                help: None,
            });
        }
    }

    /// Records that the input ends before `}}`, and returns an end token.
    fn unexpected_eof(&mut self) -> Token {
        self.report(|| "unexpected EOF while lexing expression".to_owned());
        self.stopped = true;
        self.eof()
    }

    /// Records the unexpected character `c`, or the end of the input if it is `None`, and
    /// returns an end token.
    fn unexpected(&mut self, c: Option<char>, where_: &str, expected: &str) -> Token {
        self.stopped = true;
        self.report(|| {
            let what = c.map_or_else(
                || "EOF".to_owned(),
                |c| format!("character {}", quote_rune(c)),
            );
            let note = if c == Some('"') {
                ". do you mean string literals? only single quotes are available for string \
                 delimiter"
            } else {
                ""
            };
            format!("got unexpected {what} while lexing {where_}, expecting {expected}{note}")
        });
        self.eof()
    }

    /// Returns a token of `kind` from the token start to the next character, and starts the
    /// next token there.
    fn token(&mut self, kind: TokenKind) -> Token {
        let token = Token {
            kind,
            start: to_u32(self.start),
            end: to_u32(self.pos),
        };
        self.start = self.pos;
        token
    }

    /// Returns an empty end token at the token start.
    fn eof(&self) -> Token {
        let start = to_u32(self.start);
        Token {
            kind: TokenKind::End,
            start,
            end: start,
        }
    }

    /// The text from the token start to the next character.
    fn lexeme(&self) -> &'src str {
        self.src.get(self.start..self.pos).unwrap_or_default()
    }

    fn skip_white(&mut self) {
        while matches!(self.peek(), Some(' ' | '\n' | '\r' | '\t')) {
            self.eat();
            self.start = self.pos;
        }
    }

    fn lex_ident(&mut self) -> Token {
        while matches!(self.eat(), Some(c) if c.is_ascii_alphanumeric() || c == '_' || c == '-') {}
        self.token(TokenKind::Ident)
    }

    fn lex_num(&mut self) -> Token {
        let mut c = self.peek();
        if c == Some('-') {
            c = self.eat();
        }

        if c == Some('0') {
            c = self.eat();
            if c == Some('x') {
                self.eat();
                return self.lex_hex_int();
            }
        } else {
            if !is_digit(c) {
                return self.unexpected(c, "integer part of number", EXPECTED_DIGITS);
            }
            c = self.eat_digits();
        }

        let mut kind = TokenKind::Int;

        if c == Some('.') {
            c = self.eat();
            if !is_digit(c) {
                return self.unexpected(c, "fraction part of float number", EXPECTED_DIGITS);
            }
            c = self.eat_digits();
            kind = TokenKind::Float;
        }

        if matches!(c, Some('e' | 'E')) {
            c = self.eat();
            // actionlint accepts only `-` here (docs/differences.md#exponent-plus-sign).
            if matches!(c, Some('-' | '+')) {
                c = self.eat();
            }
            if c == Some('0') {
                c = self.eat();
            } else {
                if !is_digit(c) {
                    return self.unexpected(c, "exponent part of float number", EXPECTED_DIGITS);
                }
                c = self.eat_digits();
            }
            kind = TokenKind::Float;
        }

        if is_alphanumeric(c) {
            let where_ = format!("character following number {}", self.lexeme());
            return self.unexpected(c, &where_, EXPECTED_PUNCT);
        }

        self.token(kind)
    }

    fn lex_hex_int(&mut self) -> Token {
        let mut c = self.peek();
        if c == Some('0') {
            c = self.eat();
        } else {
            if !c.is_some_and(|c| c.is_ascii_hexdigit()) {
                return self.unexpected(c, "hex integer", EXPECTED_HEX_DIGITS);
            }
            while self.eat().is_some_and(|c| c.is_ascii_hexdigit()) {}
            c = self.peek();
        }

        if is_alphanumeric(c) {
            let where_ = format!("character following hex integer {}", self.lexeme());
            return self.unexpected(c, &where_, EXPECTED_PUNCT);
        }

        self.token(TokenKind::Int)
    }

    fn lex_string(&mut self) -> Token {
        loop {
            match self.eat() {
                // A quote that is not followed by another quote ends the string. `''` is an
                // escaped quote.
                Some('\'') => {
                    if self.eat() != Some('\'') {
                        return self.token(TokenKind::String);
                    }
                }
                Some(_) => {}
                None => return self.unexpected(None, "end of string literal", "'''"),
            }
        }
    }

    fn lex_end(&mut self) -> Token {
        let c = self.eat();
        if c != Some('}') {
            return self.unexpected(c, "end marker }}", "'}'");
        }
        self.eat();
        self.token(TokenKind::End)
    }

    /// Lexes `single`, or `double` if `=` follows.
    fn lex_maybe_eq(&mut self, single: TokenKind, double: TokenKind) -> Token {
        if self.eat() == Some('=') {
            self.eat();
            return self.token(double);
        }
        self.token(single)
    }

    /// Lexes an operator made of the next character twice, such as `&&`.
    fn lex_double(&mut self, second: char, kind: TokenKind, where_: &str, expected: &str) -> Token {
        let c = self.eat();
        if c != Some(second) {
            return self.unexpected(c, where_, expected);
        }
        self.eat();
        self.token(kind)
    }

    fn lex_char(&mut self, kind: TokenKind) -> Token {
        self.eat();
        self.token(kind)
    }
}

/// Lexes the expression at the start of `src` up to and including the `}}` that ends it.
///
/// Returns the tokens, the last of which is the [`TokenKind::End`] token of `}}`, and the byte
/// offset just past `}}`.
///
/// # Errors
///
/// Returns the first error, including reaching the end of `src` before `}}`.
///
/// ```
/// use workflint_expr::{TokenKind, lex_expression};
///
/// let src = "github.ref == 'main' }} rest";
/// let (tokens, end) = lex_expression(src).unwrap();
/// let kinds: Vec<_> = tokens.iter().map(|token| token.kind).collect();
/// assert_eq!(
///     kinds,
///     [TokenKind::Ident, TokenKind::Dot, TokenKind::Ident, TokenKind::Eq, TokenKind::String, TokenKind::End],
/// );
/// assert_eq!(&src[..end as usize], "github.ref == 'main' }}");
///
/// let err = lex_expression("x == \"main\" }}").unwrap_err();
/// assert_eq!((err.start, err.end), (5, 6));
/// assert!(err.message.starts_with("got unexpected character '\"' while lexing expression"));
/// ```
pub fn lex_expression(src: &str) -> Result<(Vec<Token>, u32), ExprError> {
    let mut lexer = Lexer::new(src);
    let mut tokens = Vec::new();
    loop {
        let token = lexer.next_token();
        if let Some(err) = lexer.error.take() {
            return Err(err);
        }
        tokens.push(token);
        if token.kind == TokenKind::End {
            return Ok((tokens, lexer.offset()));
        }
    }
}

fn is_digit(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_ascii_digit())
}

fn is_alphanumeric(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_ascii_alphanumeric())
}

/// Converts an offset into the input to `u32`. [`Lexer::new`] limits the input to `u32::MAX`
/// bytes, so the conversion cannot fail.
fn to_u32(offset: usize) -> u32 {
    u32::try_from(offset).unwrap_or(u32::MAX)
}
