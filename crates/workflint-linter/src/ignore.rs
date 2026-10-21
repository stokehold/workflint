//! Ignore patterns: regular expressions that drop the diagnostics whose message they match.
//!
//! The reference compiles them with Go's `regexp` package, so a pattern is read with Go's
//! syntax and matches as it does in Go. [`IgnorePattern::new`] translates the pattern to the
//! syntax of the `regex` crate, whose engine has the same semantics as Go's for the translated
//! pattern, and reports a pattern that Go rejects with Go's error message.

use regex::Regex;
use regex_syntax::{ast, hir};

/// A regular expression in Go's syntax, matched unanchored against the message of a
/// diagnostic. `-ignore` gives them, and so does `paths.<glob>.ignore` in the config from M5.2.
#[derive(Debug, Clone)]
pub struct IgnorePattern {
    source: String,
    regex: Regex,
}

impl IgnorePattern {
    /// Compiles `pattern`, written in Go's `regexp` syntax.
    ///
    /// # Errors
    ///
    /// Fails if Go's `regexp.Compile` rejects the pattern. The error prints as Go's does, such
    /// as ``error parsing regexp: missing closing ): `(` ``.
    pub fn new(pattern: &str) -> Result<Self, PatternError> {
        let translated = translate(pattern)?;
        let error = |error: regex_syntax::Error| translated.error(pattern, &error);
        let ast = ast::parse::Parser::new()
            .parse(&translated.text)
            .map_err(|err| error(err.into()))?;
        hir::translate::Translator::new()
            .translate(&translated.text, &ast)
            .map_err(|err| error(err.into()))?;
        if let Some(span) = too_many_copies(&ast) {
            let start = translated.origin(span.start.offset, pattern);
            let end = translated.origin(span.end.offset, pattern);
            return Err(PatternError::new(
                INVALID_REPEAT_SIZE,
                pattern.get(start..end).unwrap_or(pattern),
            ));
        }
        let regex = Regex::new(&translated.text).map_err(|error| match error {
            regex::Error::CompiledTooBig(_) => PatternError::new("expression too large", pattern),
            _ => PatternError::new(error.to_string(), pattern),
        })?;
        Ok(Self {
            source: pattern.to_owned(),
            regex,
        })
    }

    /// Whether the pattern matches somewhere in `message`.
    #[must_use]
    pub fn is_match(&self, message: &str) -> bool {
        self.regex.is_match(message)
    }

    /// The pattern as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.source
    }
}

/// A pattern that Go's `regexp` package rejects. It prints as Go's error does:
/// ``error parsing regexp: <code>: `<expr>` ``, where `expr` is the part of the pattern
/// that is wrong.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("error parsing regexp: {code}: `{expr}`")]
pub struct PatternError {
    code: String,
    expr: String,
}

impl PatternError {
    fn new(code: impl Into<String>, expr: &str) -> Self {
        Self {
            code: code.into(),
            expr: expr.to_owned(),
        }
    }
}

// The error codes of Go's `regexp/syntax`.
const MISSING_PAREN: &str = "missing closing )";
const UNEXPECTED_PAREN: &str = "unexpected )";
const MISSING_BRACKET: &str = "missing closing ]";
const INVALID_CHAR_RANGE: &str = "invalid character class range";
const INVALID_ESCAPE: &str = "invalid escape sequence";
const INVALID_NAMED_CAPTURE: &str = "invalid named capture";
const INVALID_PERL_OP: &str = "invalid or unsupported Perl syntax";
const INVALID_REPEAT_OP: &str = "invalid nested repetition operator";
const INVALID_REPEAT_SIZE: &str = "invalid repeat count";
const MISSING_REPEAT_ARGUMENT: &str = "missing argument to repetition operator";
const TRAILING_BACKSLASH: &str = "trailing backslash at end of expression";
const NESTING_DEPTH: &str = "expression nests too deeply";

/// The largest count of a repetition that Go accepts.
const MAX_REPEAT: u32 = 1000;

/// A pattern in the syntax of the `regex` crate, with the byte offset in the Go pattern that
/// each of its bytes comes from.
struct Translated {
    text: String,
    origin: Vec<usize>,
}

impl Translated {
    fn push(&mut self, text: &str, at: usize) {
        self.text.push_str(text);
        self.origin.resize(self.text.len(), at);
    }

    /// The offset in the Go pattern of the byte at `offset` in the translation.
    fn origin(&self, offset: usize, pattern: &str) -> usize {
        self.origin.get(offset).copied().unwrap_or(pattern.len())
    }

    /// Go's error for the `regex_syntax` error of the translation of `pattern`.
    fn error(&self, pattern: &str, error: &regex_syntax::Error) -> PatternError {
        let (code, span) = match error {
            regex_syntax::Error::Parse(error) => (ast_code(error.kind()), error.span()),
            regex_syntax::Error::Translate(error) => (hir_code(error.kind()), error.span()),
            _ => (None, &ast::Span::splat(ast::Position::new(0, 1, 1))),
        };
        let start = self.origin(span.start.offset, pattern);
        let end = self.origin(span.end.offset, pattern).max(start);
        let text = |start: usize, end: usize| pattern.get(start..end).unwrap_or(pattern);
        match code {
            Some(code @ (MISSING_PAREN | UNEXPECTED_PAREN | NESTING_DEPTH)) => {
                PatternError::new(code, pattern)
            }
            Some(MISSING_BRACKET) => PatternError::new(MISSING_BRACKET, text(start, pattern.len())),
            Some(code @ INVALID_PERL_OP) => {
                let open = pattern.get(..start).and_then(|before| before.rfind('('));
                PatternError::new(code, text(open.unwrap_or(start), end.max(start + 1)))
            }
            Some(code) => PatternError::new(code, text(start, end)),
            None => PatternError::new(error.to_string(), pattern),
        }
    }
}

/// Go's error code for a `regex_syntax` parse error.
fn ast_code(kind: &ast::ErrorKind) -> Option<&'static str> {
    use ast::ErrorKind as K;
    Some(match kind {
        K::GroupUnclosed => MISSING_PAREN,
        K::GroupUnopened => UNEXPECTED_PAREN,
        K::ClassUnclosed => MISSING_BRACKET,
        K::ClassRangeInvalid | K::ClassRangeLiteral | K::UnicodeClassInvalid => INVALID_CHAR_RANGE,
        K::ClassEscapeInvalid
        | K::EscapeHexEmpty
        | K::EscapeHexInvalid
        | K::EscapeHexInvalidDigit
        | K::EscapeUnrecognized
        | K::UnsupportedBackreference => INVALID_ESCAPE,
        K::EscapeUnexpectedEof => TRAILING_BACKSLASH,
        K::RepetitionMissing => MISSING_REPEAT_ARGUMENT,
        K::RepetitionCountInvalid => INVALID_REPEAT_SIZE,
        K::FlagDanglingNegation
        | K::FlagDuplicate { .. }
        | K::FlagRepeatedNegation { .. }
        | K::FlagUnexpectedEof
        | K::FlagUnrecognized
        | K::UnsupportedLookAround
        | K::SpecialWordBoundaryUnclosed
        | K::SpecialWordBoundaryUnrecognized
        | K::SpecialWordOrRepetitionUnexpectedEof => INVALID_PERL_OP,
        K::NestLimitExceeded(_) => NESTING_DEPTH,
        _ => return None,
    })
}

/// Go's error code for a `regex_syntax` translation error.
fn hir_code(kind: &hir::ErrorKind) -> Option<&'static str> {
    use hir::ErrorKind as K;
    match kind {
        K::UnicodePropertyNotFound | K::UnicodePropertyValueNotFound => Some(INVALID_CHAR_RANGE),
        _ => None,
    }
}

/// Translates `pattern` from Go's syntax to the `regex` crate's.
///
/// The two syntaxes mean the same for most patterns. The translation changes the rest:
///
/// - `\d`, `\s`, `\w`, and `\b` match ASCII only in Go, and Unicode in the `regex` crate.
/// - `\Q...\E` quotes literal text in Go only.
/// - A `{` that does not start a repetition such as `{2}` or `{2,}` is a literal in Go and an
///   error in the `regex` crate.
/// - Inside brackets, `[` that does not start a class such as `[:alpha:]`, `&&`, and `~~`
///   are literals in Go and class syntax in the `regex` crate.
/// - Go allows two groups with the same name. The names do not change what matches, so the
///   translation drops them.
///
/// It also rejects what Go rejects and the `regex` crate allows: a repetition of a
/// repetition, such as `a**`, and a count over 1000.
fn translate(pattern: &str) -> Result<Translated, PatternError> {
    let mut out = Translated {
        text: String::with_capacity(pattern.len()),
        origin: Vec::with_capacity(pattern.len()),
    };
    let bytes = pattern.as_bytes();
    let mut at = 0;
    // The start of the last repetition operator, if the last thing translated was one.
    let mut last_repeat: Option<usize> = None;
    // Whether a repetition operator here has something to repeat.
    let mut operand = false;
    while let Some(c) = pattern.get(at..).and_then(|rest| rest.chars().next()) {
        let start = at;
        let mut repeat = false;
        let mut opens = false;
        match c {
            '\\' => at = escape(pattern, at, false, &mut out)?,
            '[' => at = class(pattern, at, &mut out)?,
            '(' if bytes.get(at + 1) == Some(&b'?') => {
                let (end, group_opens) = group(pattern, at, &mut out)?;
                at = end;
                if !group_opens {
                    // A group of flags such as `(?i)` leaves the operand as it was.
                    continue;
                }
                opens = true;
            }
            '(' | '|' => {
                opens = true;
                at += 1;
                out.push(&pattern[start..at], start);
            }
            '*' | '+' | '?' => {
                repeat = true;
                at += 1;
                out.push(&pattern[start..at], start);
            }
            '{' => {
                if let Some((end, min, max)) = repetition(pattern, at) {
                    if min > MAX_REPEAT || max.is_some_and(|max| max > MAX_REPEAT || min > max) {
                        return Err(PatternError::new(INVALID_REPEAT_SIZE, &pattern[at..end]));
                    }
                    repeat = true;
                    at = end;
                    out.push(&pattern[start..at], start);
                } else {
                    at += 1;
                    out.push("\\{", start);
                }
            }
            '}' => {
                at += 1;
                out.push("\\}", start);
            }
            _ => {
                at += c.len_utf8();
                out.push(&pattern[start..at], start);
            }
        }
        if repeat {
            // A `?` after a repetition makes it lazy, and is part of the operator.
            if bytes.get(at) == Some(&b'?') {
                out.push("?", at);
                at += 1;
            }
            if let Some(last) = last_repeat {
                return Err(PatternError::new(INVALID_REPEAT_OP, &pattern[last..at]));
            }
            if !operand {
                return Err(PatternError::new(
                    MISSING_REPEAT_ARGUMENT,
                    &pattern[start..at],
                ));
            }
            last_repeat = Some(start);
        } else {
            last_repeat = None;
            operand = !opens;
        }
    }
    Ok(out)
}

/// The operator of the first counted repetition, such as `{3}` in `(a{500}){3}`, whose count
/// times the counts of the repetitions inside it is over 1000, which Go rejects.
fn too_many_copies(ast: &ast::Ast) -> Option<ast::Span> {
    match ast {
        ast::Ast::Repetition(repetition) => too_many_copies(&repetition.ast).or_else(|| {
            let ast::RepetitionKind::Range(range) = &repetition.op.kind else {
                return None;
            };
            let (min, max) = match *range {
                ast::RepetitionRange::Exactly(count) => (count, Some(count)),
                ast::RepetitionRange::AtLeast(min) => (min, None),
                ast::RepetitionRange::Bounded(min, max) => (min, Some(max)),
            };
            let counted = min >= 2 || max.is_some_and(|max| max >= 2);
            (counted && !copies_fit(ast, MAX_REPEAT)).then_some(repetition.op.span)
        }),
        ast::Ast::Group(group) => too_many_copies(&group.ast),
        ast::Ast::Alternation(alternation) => alternation.asts.iter().find_map(too_many_copies),
        ast::Ast::Concat(concat) => concat.asts.iter().find_map(too_many_copies),
        _ => None,
    }
}

/// Whether `ast` makes at most `limit` copies of what its innermost repetition repeats, as Go's
/// `repeatIsValid` computes it.
fn copies_fit(ast: &ast::Ast, mut limit: u32) -> bool {
    match ast {
        ast::Ast::Repetition(repetition) => {
            if let ast::RepetitionKind::Range(range) = &repetition.op.kind {
                let copies = match *range {
                    ast::RepetitionRange::Exactly(count)
                    | ast::RepetitionRange::AtLeast(count)
                    | ast::RepetitionRange::Bounded(_, count) => count,
                };
                // Nothing inside `{0}` is copied.
                if copies == 0 {
                    return true;
                }
                if copies > limit {
                    return false;
                }
                limit /= copies;
            }
            copies_fit(&repetition.ast, limit)
        }
        ast::Ast::Group(group) => copies_fit(&group.ast, limit),
        ast::Ast::Alternation(alternation) => {
            alternation.asts.iter().all(|ast| copies_fit(ast, limit))
        }
        ast::Ast::Concat(concat) => concat.asts.iter().all(|ast| copies_fit(ast, limit)),
        _ => true,
    }
}

/// Reads the repetition `{min}`, `{min,}`, or `{min,max}` at `at`, as Go's `parseRepeat`
/// does, and returns its end, minimum, and maximum. Returns `None` if the `{` at `at` does not
/// start one, so that it is a literal.
fn repetition(pattern: &str, at: usize) -> Option<(usize, u32, Option<u32>)> {
    let rest = pattern.get(at + 1..)?;
    let (min, rest) = decimal(rest)?;
    let (max, rest) = match rest.strip_prefix(',') {
        Some(rest) if rest.starts_with('}') => (None, rest),
        Some(rest) => {
            let (max, rest) = decimal(rest)?;
            (Some(max), rest)
        }
        None => (Some(min), rest),
    };
    let rest = rest.strip_prefix('}')?;
    Some((pattern.len() - rest.len(), min, max))
}

/// Reads a decimal number without leading zeros at the start of `text`, as Go's `parseInt`
/// does. A number too big for the repeat count limit reads as `u32::MAX`.
fn decimal(text: &str) -> Option<(u32, &str)> {
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    let (number, rest) = text.split_at(digits);
    if number.is_empty() || (number.len() > 1 && number.starts_with('0')) {
        return None;
    }
    Some((number.parse().unwrap_or(u32::MAX), rest))
}

/// The ASCII class of the Perl class `\<letter>` in Go, or `None` if `letter` names none. The
/// first element goes inside brackets, and the second stands alone.
fn perl_class(letter: char) -> Option<(&'static str, &'static str)> {
    Some(match letter {
        'd' => ("0-9", "[0-9]"),
        'D' => ("[^0-9]", "[^0-9]"),
        's' => ("\\t\\n\\x0C\\r ", "[\\t\\n\\x0C\\r ]"),
        'S' => ("[^\\t\\n\\x0C\\r ]", "[^\\t\\n\\x0C\\r ]"),
        'w' => ("0-9A-Za-z_", "[0-9A-Za-z_]"),
        'W' => ("[^0-9A-Za-z_]", "[^0-9A-Za-z_]"),
        _ => return None,
    })
}

/// Translates the escape at `at`, inside brackets if `in_class`, and returns where it ends.
/// Fails as Go's `parseEscape` does on an escape that Go rejects.
fn escape(
    pattern: &str,
    at: usize,
    in_class: bool,
    out: &mut Translated,
) -> Result<usize, PatternError> {
    let Some(letter) = pattern.get(at + 1..).and_then(|rest| rest.chars().next()) else {
        return Err(PatternError::new(TRAILING_BACKSLASH, ""));
    };
    let after = at + 1 + letter.len_utf8();
    let invalid = |end: usize| PatternError::new(INVALID_ESCAPE, &pattern[at..end]);
    if let Some((inside, alone)) = perl_class(letter) {
        out.push(if in_class { inside } else { alone }, at);
        return Ok(after);
    }
    match letter {
        'b' | 'B' | 'A' | 'z' if !in_class => {
            out.push(
                match letter {
                    'b' => "(?-u:\\b)",
                    'B' => "(?-u:\\B)",
                    _ => &pattern[at..after],
                },
                at,
            );
            Ok(after)
        }
        'Q' if !in_class => {
            let rest = &pattern[after..];
            let (literal, end) = match rest.find("\\E") {
                Some(index) => (&rest[..index], after + index + 2),
                None => (rest, pattern.len()),
            };
            for (offset, c) in literal.char_indices() {
                out.push(
                    &regex_syntax::escape(c.encode_utf8(&mut [0; 4])),
                    after + offset,
                );
            }
            Ok(end)
        }
        'a' | 'f' | 'n' | 'r' | 't' | 'v' => {
            out.push(&pattern[at..after], at);
            Ok(after)
        }
        '0'..='7' => {
            // Up to three octal digits. A single digit other than 0 is a backreference, which
            // Go does not support.
            let digits = pattern[after..]
                .bytes()
                .take(2)
                .take_while(|byte| (b'0'..=b'7').contains(byte))
                .count();
            if letter != '0' && digits == 0 {
                return Err(invalid(after));
            }
            let end = after + digits;
            let value = u32::from_str_radix(&pattern[at + 1..end], 8).unwrap_or(0);
            out.push(&format!("\\x{{{value:X}}}"), at);
            Ok(end)
        }
        'x' => hex_escape(pattern, at, out),
        'p' | 'P' => {
            let end = match pattern[after..].chars().next() {
                None => {
                    return Err(PatternError::new(INVALID_CHAR_RANGE, &pattern[at..after]));
                }
                Some('{') => pattern[after..]
                    .find('}')
                    .map_or(pattern.len(), |index| after + index + 1),
                Some(name) => after + name.len_utf8(),
            };
            out.push(&pattern[at..end], at);
            Ok(end)
        }
        // Go reads any other ASCII punctuation as itself.
        _ if letter.is_ascii() && !letter.is_ascii_alphanumeric() => {
            out.push(&regex_syntax::escape(&pattern[at + 1..after]), at);
            Ok(after)
        }
        _ => Err(invalid(after)),
    }
}

/// Translates the hexadecimal escape `\xHH` or `\x{H...}` at `at`, as Go's `parseEscape`
/// reads it, and returns where it ends.
fn hex_escape(pattern: &str, at: usize, out: &mut Translated) -> Result<usize, PatternError> {
    let mut chars = pattern[at + 2..].char_indices();
    let end_of = |index: usize, c: char| at + 2 + index + c.len_utf8();
    let invalid = |end: usize| PatternError::new(INVALID_ESCAPE, &pattern[at..end]);
    let value = match chars.next() {
        None => return Err(invalid(at + 2)),
        Some((_, '{')) => {
            let mut value: u32 = 0;
            let mut digits = 0;
            loop {
                let Some((index, c)) = chars.next() else {
                    return Err(invalid(pattern.len()));
                };
                if c == '}' {
                    if digits == 0 {
                        return Err(invalid(end_of(index, c)));
                    }
                    break;
                }
                let Some(digit) = c.to_digit(16) else {
                    return Err(invalid(end_of(index, c)));
                };
                value = value * 16 + digit;
                if value > u32::from(char::MAX) {
                    return Err(invalid(end_of(index, c)));
                }
                digits += 1;
            }
            value
        }
        Some((index, high)) => {
            let low = chars.next();
            let end = low.map_or(end_of(index, high), |(index, c)| end_of(index, c));
            match (high.to_digit(16), low.and_then(|(_, c)| c.to_digit(16))) {
                (Some(high), Some(low)) => high * 16 + low,
                _ => return Err(invalid(end)),
            }
        }
    };
    let end = chars
        .next()
        .map_or(pattern.len(), |(index, _)| at + 2 + index);
    out.push(&format!("\\x{{{value:X}}}"), at);
    Ok(end)
}

/// Translates the bracketed class that starts at `at` and returns where it ends.
fn class(pattern: &str, at: usize, out: &mut Translated) -> Result<usize, PatternError> {
    let mut next = at + 1;
    out.push("[", at);
    if pattern[next..].starts_with('^') {
        out.push("^", next);
        next += 1;
    }
    // A `]` first in the class is a literal.
    if pattern[next..].starts_with(']') {
        out.push("\\]", next);
        next += 1;
    }
    while let Some(c) = pattern.get(next..).and_then(|rest| rest.chars().next()) {
        let start = next;
        match c {
            ']' => {
                out.push("]", start);
                return Ok(next + 1);
            }
            '\\' => next = escape(pattern, next, true, out)?,
            '[' => {
                if let Some(len) = posix_class(&pattern[next..]).transpose()? {
                    next += len;
                    out.push(&pattern[start..next], start);
                } else {
                    next += 1;
                    out.push("\\[", start);
                }
            }
            '&' | '~' => {
                next += 1;
                out.push(if c == '&' { "\\&" } else { "\\~" }, start);
            }
            _ => {
                next += c.len_utf8();
                out.push(&pattern[start..next], start);
            }
        }
    }
    // The class is not closed. The `regex` crate reports it with its start.
    Ok(next)
}

/// The POSIX class names that Go knows.
const POSIX_CLASSES: [&str; 14] = [
    "alnum", "alpha", "ascii", "blank", "cntrl", "digit", "graph", "lower", "print", "punct",
    "space", "upper", "word", "xdigit",
];

/// The length of the POSIX class, such as `[:alpha:]` or `[:^digit:]`, at the start of
/// `text`, if there is one. As in Go, any `[:` with a later `:]` is one, and fails if it has
/// no known name.
fn posix_class(text: &str) -> Option<Result<usize, PatternError>> {
    let end = text.strip_prefix("[:")?.find(":]")? + 4;
    let class = &text[..end];
    let name = &class[2..end - 2];
    let name = name.strip_prefix('^').unwrap_or(name);
    Some(if POSIX_CLASSES.contains(&name) {
        Ok(end)
    } else {
        Err(PatternError::new(INVALID_CHAR_RANGE, class))
    })
}

/// Translates the group that starts with `(?` at `at` as Go's `parsePerlFlags` reads it, and
/// returns where its opening ends and whether it opens a group, rather than only setting flags. A named group `(?P<name>` or `(?<name>` becomes `(`. A group
/// of flags such as `(?i-s:` gets the flags it sets, each once.
fn group(pattern: &str, at: usize, out: &mut Translated) -> Result<(usize, bool), PatternError> {
    let rest = &pattern[at + 2..];
    if let Some(name_at) = ["P<", "<"]
        .into_iter()
        .find(|open| rest.starts_with(open))
        .map(|open| at + 2 + open.len())
    {
        let Some(end) = pattern[name_at..].find('>') else {
            return Err(PatternError::new(INVALID_NAMED_CAPTURE, &pattern[at..]));
        };
        let name = &pattern[name_at..name_at + end];
        let after = name_at + end + 1;
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(PatternError::new(
                INVALID_NAMED_CAPTURE,
                &pattern[at..after],
            ));
        }
        out.push("(", at);
        return Ok((after, true));
    }
    // Each flag of `imsU` is on, off, or left as it is.
    let mut flags: [Option<bool>; 4] = [None; 4];
    let mut negated = false;
    let mut saw_flag = false;
    for (index, c) in rest.char_indices() {
        let end = at + 2 + index + c.len_utf8();
        let invalid = || PatternError::new(INVALID_PERL_OP, &pattern[at..end]);
        match c {
            'i' | 'm' | 's' | 'U' => {
                let slot = "imsU".find(c).and_then(|slot| flags.get_mut(slot));
                if let Some(slot) = slot {
                    *slot = Some(!negated);
                }
                saw_flag = true;
            }
            '-' if !negated => {
                negated = true;
                saw_flag = false;
            }
            ':' | ')' if !negated || saw_flag => {
                let mut text = String::from("(?");
                let (on, off): (Vec<_>, Vec<_>) = "imsU"
                    .chars()
                    .zip(flags)
                    .filter_map(|(flag, state)| state.map(|on| (flag, on)))
                    .partition(|&(_, on)| on);
                text.extend(on.iter().map(|&(flag, _)| flag));
                if !off.is_empty() {
                    text.push('-');
                    text.extend(off.iter().map(|&(flag, _)| flag));
                }
                if c == ':' {
                    text.push(':');
                } else if text.len() == 2 {
                    // `(?)` sets no flag.
                    return Ok((end, false));
                } else {
                    text.push(')');
                }
                out.push(&text, at);
                return Ok((end, c == ':'));
            }
            _ => return Err(invalid()),
        }
    }
    Err(PatternError::new(INVALID_PERL_OP, &pattern[at..]))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn error(pattern: &str) -> String {
        match IgnorePattern::new(pattern) {
            Ok(_) => format!("{pattern:?} compiled"),
            Err(error) => error.to_string(),
        }
    }

    /// The errors that Go's `regexp.Compile` returns for these patterns, from Go 1.26.
    #[test]
    fn a_pattern_that_go_rejects_fails_with_gos_message() {
        let cases = [
            ("(", "missing closing ): `(`"),
            ("((((", "missing closing ): `((((`"),
            ("[a", "missing closing ]: `[a`"),
            ("[]", "missing closing ]: `[]`"),
            ("[^]", "missing closing ]: `[^]`"),
            ("a**", "invalid nested repetition operator: `**`"),
            ("a*?*", "invalid nested repetition operator: `*?*`"),
            ("a{2}{3}", "invalid nested repetition operator: `{2}{3}`"),
            ("\\q", "invalid escape sequence: `\\q`"),
            ("[\\q]", "invalid escape sequence: `\\q`"),
            ("\\C", "invalid escape sequence: `\\C`"),
            ("\\xZZ", "invalid escape sequence: `\\xZZ`"),
            ("x{2,1}", "invalid repeat count: `{2,1}`"),
            ("a{1001}", "invalid repeat count: `{1001}`"),
            ("*", "missing argument to repetition operator: `*`"),
            ("a|*", "missing argument to repetition operator: `*`"),
            ("{2}", "missing argument to repetition operator: `{2}`"),
            (")", "unexpected ): `)`"),
            ("\\", "trailing backslash at end of expression: ``"),
            ("(?P<>a)", "invalid named capture: `(?P<>`"),
            ("(?<>a)", "invalid named capture: `(?<>`"),
            ("[z-a]", "invalid character class range: `z-a`"),
            ("\\p{Foo}", "invalid character class range: `\\p{Foo}`"),
            ("(?z)", "invalid or unsupported Perl syntax: `(?z`"),
            ("(?=a)", "invalid or unsupported Perl syntax: `(?=`"),
            ("(?i", "invalid or unsupported Perl syntax: `(?i`"),
            ("(?-)", "invalid or unsupported Perl syntax: `(?-)`"),
            ("(?--i)", "invalid or unsupported Perl syntax: `(?--`"),
            ("\\1", "invalid escape sequence: `\\1`"),
            ("\\8", "invalid escape sequence: `\\8`"),
            ("\\x{110000}", "invalid escape sequence: `\\x{110000`"),
            ("\\p", "invalid character class range: `\\p`"),
            ("[[:foo:]]", "invalid character class range: `[:foo:]`"),
            ("(a{500}){3}", "invalid repeat count: `{3}`"),
            ("(?:a{10}b{2,}){101}", "invalid repeat count: `{101}`"),
        ];
        for (pattern, message) in cases {
            assert_eq!(
                error(pattern),
                format!("error parsing regexp: {message}"),
                "{pattern}"
            );
        }
    }

    #[test]
    fn a_pattern_that_go_accepts_compiles() {
        for pattern in [
            "a{,2}",
            "a{01}",
            "a{2,3",
            "a{1000}",
            "(a{10}){100}",
            "(a*){1000}",
            "(a{1000}){0}",
            "a{2}?",
            "(?P<a>x)(?P<a>y)",
            "(?<n>a)",
            "\\Q.*",
            "(?i-i)",
            "\\b\\pZ",
            "\\pZ{",
            "\\z",
            "[]a]",
            "[^]a]",
            "",
        ] {
            assert!(IgnorePattern::new(pattern).is_ok(), "{pattern}");
        }
    }

    /// What Go's `regexp.MatchString` returns for these patterns and texts, from Go 1.26.
    #[test]
    fn a_pattern_matches_where_go_matches() {
        let cases = [
            ("\\d", "٣", false),
            ("\\w", "é", false),
            ("\\s", "\u{a0}", false),
            ("\\s", "\u{b}", false),
            ("\\bx", "éx", true),
            ("\\B", "é", true),
            ("[\\d-z]", "-", true),
            ("[\\D]", "5", false),
            ("object type {}", "object type {}", true),
            ("x{,2}", "x{,2}", true),
            ("a{01}", "a{01}", true),
            ("}", "}", true),
            ("\\Q.*\\E", "a.*b", true),
            ("\\Q.*\\E", "ab", false),
            ("[[a]", "[", true),
            ("[a&&b]", "&", true),
            ("[a~~b]", "~", true),
            ("[[:alpha:]]", "é", false),
            ("\\pL", "é", true),
            ("(?i)Straße", "STRASSE", false),
            ("(?i)k", "\u{212a}", true),
            ("a.c", "a\nc", false),
            ("^b", "a\nb", false),
            ("a$", "a\n", false),
            ("\\101", "A", true),
            ("\\0", "\0", true),
            ("a{2}?", "aa", true),
            ("(?P<a>x)(?P<a>y)", "xy", true),
        ];
        for (pattern, text, matches) in cases {
            let compiled = IgnorePattern::new(pattern).unwrap();
            assert_eq!(compiled.is_match(text), matches, "{pattern} on {text:?}");
        }
    }

    #[test]
    fn as_str_is_the_pattern_as_written() {
        assert_eq!(IgnorePattern::new("\\d+").unwrap().as_str(), "\\d+");
    }
}
