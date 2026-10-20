//! Help text that names the likely cause of a cryptic YAML syntax error.
//!
//! libyaml's `did not find expected key` says only that the token at the
//! problem position cannot start a key of the enclosing block mapping. The
//! classifier reads the source around that position and, for the common
//! causes, says what is wrong and how to fix it. The message and position of
//! the diagnostic stay as they are, and the actionlint-compatible formats do
//! not print the help text (ADR 0032).

use workflint_diagnostics::quote::quote;

use crate::libyaml::{Error, ErrorKind, Mark};

/// The parser's problem when a block mapping meets a token that is not a key.
const EXPECTED_KEY: &str = "did not find expected key";

/// The scanner's problem when a plain scalar at the indentation of a key has
/// no `:` after it.
const EXPECTED_COLON: &str = "could not find expected ':'";

/// The context of [`EXPECTED_COLON`], whose mark is the start of the scalar.
const SIMPLE_KEY: &str = "while scanning a simple key";

/// The characters that cannot start a plain scalar used as a key.
const INDICATORS: &[char] = &[
    '-', '?', ':', ',', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '\'', '"', '%', '@', '`',
];

/// The help text for the syntax error `err` in `text`, or `None` when no
/// common cause matches.
pub(crate) fn syntax_error_help(text: &str, err: &Error) -> Option<String> {
    let problem = err.problem_mark()?;
    let context = err.context_mark()?;
    match (err.kind(), err.problem()) {
        (ErrorKind::Parser, EXPECTED_KEY) => expected_key(text, problem, context),
        (ErrorKind::Scanner, EXPECTED_COLON) if err.context() == Some(SIMPLE_KEY) => {
            let start = offset(text, context)?;
            missing_space(text.get(start..line_end(text, start))?)
        }
        _ => None,
    }
}

/// The help text for `did not find expected key` at `problem`, in the block
/// mapping that starts at `context`.
fn expected_key(text: &str, problem: Mark, context: Mark) -> Option<String> {
    let at = offset(text, problem)?;
    let before = text.get(line_start(text, at)..at)?;
    let rest = text.get(at..line_end(text, at))?;
    if let Some(last) = before.trim_end_matches([' ', '\t']).chars().next_back() {
        return text_after_value(last);
    }
    if rest == "-" || rest.starts_with("- ") || rest.starts_with("-\t") {
        return Some(list_item(text, problem, context));
    }
    if is_key(rest) {
        return (problem.column > context.column).then(|| {
            format!(
                "this key is indented {}, but the keys of the mapping that starts on line {} \
                 are indented {}, so align it with the keys of the mapping it belongs to",
                spaces(problem.column),
                context.line + 1,
                context.column,
            )
        });
    }
    missing_space(rest)
}

/// The help text for a token after the character `last` on the same line.
fn text_after_value(last: char) -> Option<String> {
    const AFTER: &str = "and YAML allows no more text after it on the line, so";
    let help = match last {
        ']' => format!(
            "a flow sequence ends at its \"]\" {AFTER} quote the whole value if it is a string"
        ),
        '}' => format!(
            "a flow mapping ends at its \"}}\" {AFTER} quote the whole value if it is a string"
        ),
        '"' | '\'' => format!(
            "a quoted string ends at its closing quote {AFTER} put the whole value inside the \
             quotes"
        ),
        _ => return None,
    };
    Some(help)
}

/// The help text for a `-` item at `problem`. The list it belongs to is the
/// least indented one between the start of the mapping and the item, which
/// skips nested lists and `-` lines inside block scalars.
fn list_item(text: &str, problem: Mark, context: Mark) -> String {
    let sibling = offset(text, context)
        .zip(offset(text, problem))
        .and_then(|(context_at, problem_at)| {
            text.get(line_start(text, context_at)..line_start(text, problem_at))
        })
        .and_then(|between| {
            (context.line..)
                .zip(lines(between))
                .filter_map(|(line, content)| {
                    let item = content.trim_start_matches(' ');
                    let is_item = item == "-" || item.starts_with("- ") || item.starts_with("-\t");
                    is_item.then(|| (content.len() - item.len(), line))
                })
                .min_by_key(|&(indent, line)| (indent, std::cmp::Reverse(line)))
        });
    match sibling {
        Some((indent, line)) if u64::try_from(indent).ok() != Some(problem.column) => format!(
            "this list item is indented {} and the item on line {} is indented {indent}, but the \
             items of one list need the same indentation",
            spaces(problem.column),
            line + 1,
        ),
        _ => format!(
            "the mapping that starts on line {} expects a key here, not a list item, so move the \
             item under the key whose value is the list",
            context.line + 1,
        ),
    }
}

/// The help text for a plain scalar at the start of `rest`, such as
/// `runs-on:ubuntu-latest`, that would be a key and its value with a space
/// after the `:`.
fn missing_space(rest: &str) -> Option<String> {
    if rest.starts_with(INDICATORS) || is_key(rest) {
        return None;
    }
    let token = rest.split([' ', '\t']).next()?;
    let (key, value) = token.split_once(':')?;
    if key.is_empty() || value.is_empty() || value.starts_with([':', '/']) {
        return None;
    }
    Some(format!(
        "YAML reads {} as one string, because a \":\" after a key needs a space after it; did \
         you mean {}?",
        quote(token),
        quote(&format!("{key}: {value}")),
    ))
}

/// Whether the rest of a line starts with a key: a `:` followed by a space,
/// a tab, or the end of the line comes before any comment.
fn is_key(rest: &str) -> bool {
    if rest.starts_with(['-', '?', ':', '#', '|', '>', '%', '@', '`']) {
        return false;
    }
    let content = rest.split(" #").next().unwrap_or(rest).trim_end();
    content.ends_with(':') || content.contains(": ") || content.contains(":\t")
}

/// `n` spaces, in words.
fn spaces(n: u64) -> String {
    if n == 1 {
        "1 space".to_owned()
    } else {
        format!("{n} spaces")
    }
}

/// The byte offset of `mark` in `text`, if it is on a character boundary.
fn offset(text: &str, mark: Mark) -> Option<usize> {
    usize::try_from(mark.index)
        .ok()
        .filter(|&at| text.is_char_boundary(at))
}

fn is_break(ch: char) -> bool {
    matches!(ch, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

/// The offset of the start of the line that contains `at`.
fn line_start(text: &str, at: usize) -> usize {
    text.get(..at)
        .and_then(|before| before.rfind(is_break))
        .and_then(|i| text.get(i..)?.chars().next().map(|ch| i + ch.len_utf8()))
        .unwrap_or(0)
}

/// The offset of the line break that ends the line that contains `at`, or of
/// the end of `text`.
fn line_end(text: &str, at: usize) -> usize {
    text.get(at..)
        .and_then(|rest| rest.find(is_break))
        .map_or(text.len(), |i| at + i)
}

/// The lines of `text`, with `\r\n` as one line break.
fn lines(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let end = rest.find(is_break).unwrap_or(rest.len());
        let (line, after) = rest.split_at_checked(end)?;
        let break_len = if after.starts_with("\r\n") {
            2
        } else {
            after.chars().next().map_or(0, char::len_utf8)
        };
        rest = after.get(break_len..).unwrap_or_default();
        Some(line)
    })
}
