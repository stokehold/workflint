use std::fmt;
use std::ops::Range;

/// The kind of a [`Token`].
///
/// [`TokenKind::as_str`] returns the names that actionlint's messages print, such as `IDENT` or
/// `<=`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    /// The `}}` that ends an expression, or the token the lexer returns after an error.
    End,
    /// An identifier: `[A-Za-z_][A-Za-z0-9_-]*`. `true`, `false`, and `null` are identifiers too.
    Ident,
    /// A string literal in single quotes, where `''` is an escaped quote.
    String,
    /// A decimal or `0x` hexadecimal integer, with an optional leading `-`.
    Int,
    /// A number with a fraction, an exponent, or both.
    Float,
    /// `(`
    LeftParen,
    /// `)`
    RightParen,
    /// `[`
    LeftBracket,
    /// `]`
    RightBracket,
    /// `.`
    Dot,
    /// `!`
    Not,
    /// `<`
    Less,
    /// `<=`
    LessEq,
    /// `>`
    Greater,
    /// `>=`
    GreaterEq,
    /// `==`
    Eq,
    /// `!=`
    NotEq,
    /// `&&`
    And,
    /// `||`
    Or,
    /// `*`
    Star,
    /// `,`
    Comma,
}

impl TokenKind {
    /// Returns the name that actionlint's messages print for this kind: `END`, `IDENT`,
    /// `STRING`, `INTEGER`, `FLOAT`, or the punctuation itself.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::End => "END",
            Self::Ident => "IDENT",
            Self::String => "STRING",
            Self::Int => "INTEGER",
            Self::Float => "FLOAT",
            Self::LeftParen => "(",
            Self::RightParen => ")",
            Self::LeftBracket => "[",
            Self::RightBracket => "]",
            Self::Dot => ".",
            Self::Not => "!",
            Self::Less => "<",
            Self::LessEq => "<=",
            Self::Greater => ">",
            Self::GreaterEq => ">=",
            Self::Eq => "==",
            Self::NotEq => "!=",
            Self::And => "&&",
            Self::Or => "||",
            Self::Star => "*",
            Self::Comma => ",",
        }
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One token: its kind and its byte range in the lexer's input.
///
/// The token's text is `&src[token.range()]`, where `src` is the string given to the lexer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Token {
    /// The kind of the token.
    pub kind: TokenKind,
    /// The byte offset of the token's first byte.
    pub start: u32,
    /// The byte offset one past the token's last byte.
    pub end: u32,
}

impl Token {
    /// Returns the token's byte range in the lexer's input.
    #[must_use]
    pub fn range(&self) -> Range<usize> {
        to_usize(self.start)..to_usize(self.end)
    }
}

/// Converts a `u32` offset to `usize`. Offsets past `usize::MAX` cannot occur, because they
/// point into a `str`.
pub(crate) fn to_usize(offset: u32) -> usize {
    usize::try_from(offset).unwrap_or(usize::MAX)
}
