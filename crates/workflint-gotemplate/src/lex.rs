//! The lexer, a port of Go's `text/template/parse/lex.go`.
//!
//! The parser pulls one item at a time with [`Lexer::next_item`]. An error is an item of kind
//! [`ItemKind::Error`] whose value is the message, after which the lexer returns only
//! [`ItemKind::Eof`].

use crate::fmt::{is_digit, is_letter, quote};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ItemKind {
    Error,
    Bool,
    Char,
    CharConstant,
    Complex,
    Assign,
    Declare,
    Eof,
    Field,
    Identifier,
    LeftDelim,
    LeftParen,
    Number,
    Pipe,
    RawString,
    RightDelim,
    RightParen,
    Space,
    String,
    Text,
    Variable,
    // Keywords.
    Block,
    Break,
    Continue,
    Dot,
    Define,
    Else,
    End,
    If,
    Nil,
    Range,
    Template,
    With,
}

impl ItemKind {
    fn is_keyword(self) -> bool {
        matches!(
            self,
            ItemKind::Block
                | ItemKind::Break
                | ItemKind::Continue
                | ItemKind::Dot
                | ItemKind::Define
                | ItemKind::Else
                | ItemKind::End
                | ItemKind::If
                | ItemKind::Nil
                | ItemKind::Range
                | ItemKind::Template
                | ItemKind::With
        )
    }
}

fn keyword(word: &str) -> Option<ItemKind> {
    Some(match word {
        "." => ItemKind::Dot,
        "block" => ItemKind::Block,
        "break" => ItemKind::Break,
        "continue" => ItemKind::Continue,
        "define" => ItemKind::Define,
        "else" => ItemKind::Else,
        "end" => ItemKind::End,
        "if" => ItemKind::If,
        "range" => ItemKind::Range,
        "nil" => ItemKind::Nil,
        "template" => ItemKind::Template,
        "with" => ItemKind::With,
        _ => return None,
    })
}

/// A token: its kind, its byte offset, its text, and the line it starts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item {
    pub(crate) kind: ItemKind,
    pub(crate) pos: usize,
    pub(crate) val: String,
    pub(crate) line: usize,
}

impl std::fmt::Display for Item {
    /// Go's `item.String`, which parse errors quote.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            ItemKind::Eof => f.write_str("EOF"),
            ItemKind::Error => f.write_str(&self.val),
            kind if kind.is_keyword() => write!(f, "<{}>", self.val),
            _ if self.val.len() > 10 => {
                // Go's `%.10q` quotes the first ten characters.
                let head: String = self.val.chars().take(10).collect();
                write!(f, "{}...", quote(&head))
            }
            _ => f.write_str(&quote(&self.val)),
        }
    }
}

const LEFT_DELIM: &str = "{{";
const RIGHT_DELIM: &str = "}}";
const LEFT_COMMENT: &str = "/*";
const RIGHT_COMMENT: &str = "*/";
/// A trim marker and the space next to it.
const TRIM_MARKER_LEN: usize = 2;

#[derive(Clone, Copy)]
enum State {
    Text,
    LeftDelim,
    Comment,
    RightDelim,
    InsideAction,
    Space,
    Identifier,
    Field,
    Variable,
    Char,
    Number,
    Quote,
    RawQuote,
}

#[allow(clippy::struct_excessive_bools)]
pub(crate) struct Lexer<'a> {
    input: &'a str,
    pos: usize,
    start: usize,
    at_eof: bool,
    paren_depth: usize,
    line: usize,
    start_line: usize,
    inside_action: bool,
    break_ok: bool,
    continue_ok: bool,
    /// Set once an error was returned; the lexer then only returns EOF.
    failed: bool,
}

/// What a state step does next.
enum Step {
    Next(State),
    Emit(Item),
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(input: &'a str, break_ok: bool, continue_ok: bool) -> Lexer<'a> {
        Lexer {
            input,
            pos: 0,
            start: 0,
            at_eof: false,
            paren_depth: 0,
            line: 1,
            start_line: 1,
            inside_action: false,
            break_ok,
            continue_ok,
            failed: false,
        }
    }

    /// Go's `nextItem`.
    pub(crate) fn next_item(&mut self) -> Item {
        if self.failed {
            return Item {
                kind: ItemKind::Eof,
                pos: 0,
                val: "EOF".to_owned(),
                line: self.start_line,
            };
        }
        let mut state = if self.inside_action {
            State::InsideAction
        } else {
            State::Text
        };
        loop {
            match self.step(state) {
                Step::Next(next) => state = next,
                Step::Emit(item) => {
                    if item.kind == ItemKind::Error {
                        self.failed = true;
                    }
                    return item;
                }
            }
        }
    }

    fn step(&mut self, state: State) -> Step {
        match state {
            State::Text => self.lex_text(),
            State::LeftDelim => self.lex_left_delim(),
            State::Comment => self.lex_comment(),
            State::RightDelim => self.lex_right_delim(),
            State::InsideAction => self.lex_inside_action(),
            State::Space => self.lex_space(),
            State::Identifier => self.lex_identifier(),
            State::Field => self.lex_field_or_variable(ItemKind::Field),
            State::Variable => {
                if self.at_terminator() {
                    self.emit(ItemKind::Variable)
                } else {
                    self.lex_field_or_variable(ItemKind::Variable)
                }
            }
            State::Char => self.lex_char(),
            State::Number => self.lex_number(),
            State::Quote => self.lex_quote(),
            State::RawQuote => self.lex_raw_quote(),
        }
    }

    fn rest(&self) -> &'a str {
        self.input.get(self.pos..).unwrap_or_default()
    }

    /// Go's `next`: the next character, or `None` at the end.
    fn next(&mut self) -> Option<char> {
        let Some(c) = self.rest().chars().next() else {
            self.at_eof = true;
            return None;
        };
        self.pos += c.len_utf8();
        if c == '\n' {
            self.line += 1;
        }
        Some(c)
    }

    fn peek(&mut self) -> Option<char> {
        let c = self.next();
        self.backup();
        c
    }

    fn backup(&mut self) {
        if !self.at_eof
            && self.pos > 0
            && let Some(c) = self
                .input
                .get(..self.pos)
                .and_then(|s| s.chars().next_back())
        {
            self.pos -= c.len_utf8();
            if c == '\n' {
                self.line -= 1;
            }
        }
    }

    fn this_item(&mut self, kind: ItemKind) -> Item {
        let item = Item {
            kind,
            pos: self.start,
            val: self
                .input
                .get(self.start..self.pos)
                .unwrap_or_default()
                .to_owned(),
            line: self.start_line,
        };
        self.start = self.pos;
        self.start_line = self.line;
        item
    }

    fn emit(&mut self, kind: ItemKind) -> Step {
        Step::Emit(self.this_item(kind))
    }

    fn ignore(&mut self) {
        self.line += count_newlines(self.input.get(self.start..self.pos).unwrap_or_default());
        self.start = self.pos;
        self.start_line = self.line;
    }

    fn accept(&mut self, valid: &str) -> bool {
        if self.next().is_some_and(|c| valid.contains(c)) {
            return true;
        }
        self.backup();
        false
    }

    fn accept_run(&mut self, valid: &str) {
        while self.next().is_some_and(|c| valid.contains(c)) {}
        self.backup();
    }

    fn error(&mut self, message: String) -> Step {
        let item = Item {
            kind: ItemKind::Error,
            pos: self.start,
            val: message,
            line: self.start_line,
        };
        self.start = 0;
        self.pos = 0;
        self.input = "";
        Step::Emit(item)
    }

    fn lex_text(&mut self) -> Step {
        if let Some(x) = self.rest().find(LEFT_DELIM) {
            if x > 0 {
                self.pos += x;
                let delim_end = self.pos + LEFT_DELIM.len();
                let mut trim_length = 0;
                if has_left_trim_marker(self.input.get(delim_end..).unwrap_or_default()) {
                    trim_length =
                        right_trim_length(self.input.get(self.start..self.pos).unwrap_or_default());
                }
                self.pos -= trim_length;
                self.line +=
                    count_newlines(self.input.get(self.start..self.pos).unwrap_or_default());
                let item = self.this_item(ItemKind::Text);
                self.pos += trim_length;
                self.ignore();
                if !item.val.is_empty() {
                    return Step::Emit(item);
                }
            }
            return Step::Next(State::LeftDelim);
        }
        self.pos = self.input.len();
        if self.pos > self.start {
            self.line += count_newlines(self.input.get(self.start..self.pos).unwrap_or_default());
            return self.emit(ItemKind::Text);
        }
        self.emit(ItemKind::Eof)
    }

    /// Go's `atRightDelim`: whether a right delimiter follows, and whether it has a trim marker.
    fn at_right_delim(&self) -> (bool, bool) {
        let rest = self.rest();
        if has_right_trim_marker(rest)
            && rest
                .get(TRIM_MARKER_LEN..)
                .is_some_and(|s| s.starts_with(RIGHT_DELIM))
        {
            return (true, true);
        }
        if rest.starts_with(RIGHT_DELIM) {
            return (true, false);
        }
        (false, false)
    }

    fn lex_left_delim(&mut self) -> Step {
        self.pos += LEFT_DELIM.len();
        let trim_space = has_left_trim_marker(self.rest());
        let after_marker = if trim_space { TRIM_MARKER_LEN } else { 0 };
        if self
            .rest()
            .get(after_marker..)
            .is_some_and(|s| s.starts_with(LEFT_COMMENT))
        {
            self.pos += after_marker;
            self.ignore();
            return Step::Next(State::Comment);
        }
        let item = self.this_item(ItemKind::LeftDelim);
        self.inside_action = true;
        self.pos += after_marker;
        self.ignore();
        self.paren_depth = 0;
        Step::Emit(item)
    }

    fn lex_comment(&mut self) -> Step {
        self.pos += LEFT_COMMENT.len();
        let Some(x) = self.rest().find(RIGHT_COMMENT) else {
            return self.error("unclosed comment".to_owned());
        };
        self.pos += x + RIGHT_COMMENT.len();
        let (delim, trim_space) = self.at_right_delim();
        if !delim {
            return self.error("comment ends before closing delimiter".to_owned());
        }
        self.line += count_newlines(self.input.get(self.start..self.pos).unwrap_or_default());
        self.this_item(ItemKind::Text);
        if trim_space {
            self.pos += TRIM_MARKER_LEN;
        }
        self.pos += RIGHT_DELIM.len();
        if trim_space {
            self.pos += left_trim_length(self.rest());
        }
        self.ignore();
        Step::Next(State::Text)
    }

    fn lex_right_delim(&mut self) -> Step {
        let (_, trim_space) = self.at_right_delim();
        if trim_space {
            self.pos += TRIM_MARKER_LEN;
            self.ignore();
        }
        self.pos += RIGHT_DELIM.len();
        let item = self.this_item(ItemKind::RightDelim);
        if trim_space {
            self.pos += left_trim_length(self.rest());
            self.ignore();
        }
        self.inside_action = false;
        Step::Emit(item)
    }

    fn lex_inside_action(&mut self) -> Step {
        let (delim, _) = self.at_right_delim();
        if delim {
            if self.paren_depth == 0 {
                return Step::Next(State::RightDelim);
            }
            return self.error("unclosed left paren".to_owned());
        }
        let Some(r) = self.next() else {
            return self.error("unclosed action".to_owned());
        };
        match r {
            _ if is_space(r) => {
                self.backup();
                Step::Next(State::Space)
            }
            '=' => self.emit(ItemKind::Assign),
            ':' => {
                if self.next() != Some('=') {
                    return self.error("expected :=".to_owned());
                }
                self.emit(ItemKind::Declare)
            }
            '|' => self.emit(ItemKind::Pipe),
            '"' => Step::Next(State::Quote),
            '`' => Step::Next(State::RawQuote),
            '$' => Step::Next(State::Variable),
            '\'' => Step::Next(State::Char),
            '.' if !self
                .rest()
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_digit) =>
            {
                if self.rest().is_empty() {
                    // Go falls through to the number scanner here.
                    self.backup();
                    return Step::Next(State::Number);
                }
                Step::Next(State::Field)
            }
            '.' | '+' | '-' | '0'..='9' => {
                self.backup();
                Step::Next(State::Number)
            }
            _ if is_alphanumeric(r) => {
                self.backup();
                Step::Next(State::Identifier)
            }
            '(' => {
                self.paren_depth += 1;
                self.emit(ItemKind::LeftParen)
            }
            ')' => {
                if self.paren_depth == 0 {
                    return self.error("unexpected right paren".to_owned());
                }
                self.paren_depth -= 1;
                self.emit(ItemKind::RightParen)
            }
            _ if r.is_ascii() && !r.is_ascii_control() => self.emit(ItemKind::Char),
            _ => self.error(format!(
                "unrecognized character in action: {}",
                unicode_format(r)
            )),
        }
    }

    fn lex_space(&mut self) -> Step {
        let mut num_spaces = 0;
        while self.peek().is_some_and(is_space) {
            self.next();
            num_spaces += 1;
        }
        let before = self.pos.saturating_sub(1);
        let tail = self.input.get(before..).unwrap_or_default();
        if has_right_trim_marker(tail)
            && tail
                .get(TRIM_MARKER_LEN..)
                .is_some_and(|s| s.starts_with(RIGHT_DELIM))
        {
            self.backup();
            if num_spaces == 1 {
                return Step::Next(State::RightDelim);
            }
        }
        self.emit(ItemKind::Space)
    }

    fn lex_identifier(&mut self) -> Step {
        loop {
            let r = self.next();
            if r.is_some_and(is_alphanumeric) {
                continue;
            }
            self.backup();
            let word = self
                .input
                .get(self.start..self.pos)
                .unwrap_or_default()
                .to_owned();
            if !self.at_terminator() {
                return self.bad_character(r);
            }
            if let Some(kind) = keyword(&word).filter(|kind| kind.is_keyword()) {
                if kind == ItemKind::Break && !self.break_ok
                    || kind == ItemKind::Continue && !self.continue_ok
                {
                    return self.emit(ItemKind::Identifier);
                }
                return self.emit(kind);
            }
            if word.starts_with('.') {
                return self.emit(ItemKind::Field);
            }
            if word == "true" || word == "false" {
                return self.emit(ItemKind::Bool);
            }
            return self.emit(ItemKind::Identifier);
        }
    }

    fn bad_character(&mut self, r: Option<char>) -> Step {
        // Go formats the end of input, -1, as U+FFFFFFFFFFFFFFFF.
        let shown = r.map_or_else(|| "U+FFFFFFFFFFFFFFFF".to_owned(), unicode_format);
        self.error(format!("bad character {shown}"))
    }

    fn lex_field_or_variable(&mut self, kind: ItemKind) -> Step {
        if self.at_terminator() {
            if kind == ItemKind::Variable {
                return self.emit(ItemKind::Variable);
            }
            return self.emit(ItemKind::Dot);
        }
        let mut r;
        loop {
            r = self.next();
            if !r.is_some_and(is_alphanumeric) {
                self.backup();
                break;
            }
        }
        if !self.at_terminator() {
            return self.bad_character(r);
        }
        self.emit(kind)
    }

    /// Go's `atTerminator`: whether the character after an identifier may end it.
    fn at_terminator(&mut self) -> bool {
        match self.peek() {
            None | Some('.' | ',' | '|' | ':' | ')' | '(') => true,
            Some(r) if is_space(r) => true,
            Some(_) => self.rest().starts_with(RIGHT_DELIM),
        }
    }

    fn lex_char(&mut self) -> Step {
        loop {
            match self.next() {
                Some('\\') => {
                    if matches!(self.next(), Some(r) if r != '\n') {
                        continue;
                    }
                    return self.error("unterminated character constant".to_owned());
                }
                None | Some('\n') => {
                    return self.error("unterminated character constant".to_owned());
                }
                Some('\'') => break,
                Some(_) => {}
            }
        }
        self.emit(ItemKind::CharConstant)
    }

    fn lex_number(&mut self) -> Step {
        if !self.scan_number() {
            return self.bad_number();
        }
        if let Some('+' | '-') = self.peek() {
            if !self.scan_number() || !self.input[..self.pos].ends_with('i') {
                return self.bad_number();
            }
            return self.emit(ItemKind::Complex);
        }
        self.emit(ItemKind::Number)
    }

    fn bad_number(&mut self) -> Step {
        let text = quote(self.input.get(self.start..self.pos).unwrap_or_default());
        self.error(format!("bad number syntax: {text}"))
    }

    fn scan_number(&mut self) -> bool {
        self.accept("+-");
        let mut digits = "0123456789_";
        if self.accept("0") {
            if self.accept("xX") {
                digits = "0123456789abcdefABCDEF_";
            } else if self.accept("oO") {
                digits = "01234567_";
            } else if self.accept("bB") {
                digits = "01_";
            }
        }
        self.accept_run(digits);
        if self.accept(".") {
            self.accept_run(digits);
        }
        if digits.len() == 10 + 1 && self.accept("eE") {
            self.accept("+-");
            self.accept_run("0123456789_");
        }
        if digits.len() == 16 + 6 + 1 && self.accept("pP") {
            self.accept("+-");
            self.accept_run("0123456789_");
        }
        self.accept("i");
        if self.peek().is_some_and(is_alphanumeric) {
            self.next();
            return false;
        }
        true
    }

    fn lex_quote(&mut self) -> Step {
        loop {
            match self.next() {
                Some('\\') => {
                    if matches!(self.next(), Some(r) if r != '\n') {
                        continue;
                    }
                    return self.error("unterminated quoted string".to_owned());
                }
                None | Some('\n') => {
                    return self.error("unterminated quoted string".to_owned());
                }
                Some('"') => break,
                Some(_) => {}
            }
        }
        self.emit(ItemKind::String)
    }

    fn lex_raw_quote(&mut self) -> Step {
        loop {
            match self.next() {
                None => return self.error("unterminated raw quoted string".to_owned()),
                Some('`') => break,
                Some(_) => {}
            }
        }
        self.emit(ItemKind::RawString)
    }
}

fn count_newlines(s: &str) -> usize {
    s.bytes().filter(|b| *b == b'\n').count()
}

fn is_space(r: char) -> bool {
    matches!(r, ' ' | '\t' | '\r' | '\n')
}

fn is_alphanumeric(r: char) -> bool {
    r == '_' || is_letter(r) || is_digit(r)
}

fn has_left_trim_marker(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 2 && b[0] == b'-' && is_space(char::from(b[1]))
}

fn has_right_trim_marker(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 2 && is_space(char::from(b[0])) && b[1] == b'-'
}

fn right_trim_length(s: &str) -> usize {
    s.len() - s.trim_end_matches(is_space).len()
}

fn left_trim_length(s: &str) -> usize {
    s.len() - s.trim_start_matches(is_space).len()
}

/// Go's `%#U`: `U+00A7 '§'`, without the character if it does not print.
fn unicode_format(r: char) -> String {
    let code = u32::from(r);
    if crate::fmt::is_print(r) {
        format!("U+{code:04X} '{r}'")
    } else {
        format!("U+{code:04X}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(input: &str) -> Vec<(ItemKind, String)> {
        let mut lexer = Lexer::new(input, true, true);
        let mut items = Vec::new();
        loop {
            let item = lexer.next_item();
            let kind = item.kind;
            items.push((kind, item.val));
            if matches!(kind, ItemKind::Eof | ItemKind::Error) {
                return items;
            }
        }
    }

    #[test]
    fn trim_markers_remove_the_space_around_an_action() {
        assert_eq!(
            kinds("a \n{{- 1 -}}\n b"),
            vec![
                (ItemKind::Text, "a".to_owned()),
                (ItemKind::LeftDelim, "{{".to_owned()),
                (ItemKind::Number, "1".to_owned()),
                (ItemKind::RightDelim, "}}".to_owned()),
                (ItemKind::Text, "b".to_owned()),
                (ItemKind::Eof, String::new()),
            ]
        );
    }

    #[test]
    fn a_minus_without_a_space_is_a_number() {
        assert_eq!(
            kinds("{{-1}}"),
            vec![
                (ItemKind::LeftDelim, "{{".to_owned()),
                (ItemKind::Number, "-1".to_owned()),
                (ItemKind::RightDelim, "}}".to_owned()),
                (ItemKind::Eof, String::new()),
            ]
        );
    }

    #[test]
    fn break_is_an_identifier_when_a_function_has_its_name() {
        let mut lexer = Lexer::new("{{break}}", false, true);
        lexer.next_item();
        assert_eq!(lexer.next_item().kind, ItemKind::Identifier);
    }

    #[test]
    fn an_error_ends_the_items() {
        let mut lexer = Lexer::new("{{\"abc", true, true);
        lexer.next_item();
        assert_eq!(lexer.next_item().kind, ItemKind::Error);
        assert_eq!(lexer.next_item().kind, ItemKind::Eof);
    }

    #[test]
    fn long_items_print_their_first_ten_characters() {
        let item = Item {
            kind: ItemKind::String,
            pos: 0,
            val: "\"a very long string\"".to_owned(),
            line: 1,
        };
        assert_eq!(item.to_string(), r#""\"a very lo"..."#);
    }
}
