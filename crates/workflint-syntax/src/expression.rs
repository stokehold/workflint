//! The `${{ }}` placeholders in a string: the reference's tests for them,
//! quirks included, and the scan that parses each one.

use std::ops::Range;

use workflint_expr::{ExprError, Lexer, TokenKind, parse_expression};

/// One `${{ }}` in a string, with its parsed expression or the error that
/// stopped the parse.
///
/// Offsets are bytes in the string's decoded value. The ranges in
/// [`expr`](Self::expr) and in its error are relative to
/// [`offset`](Self::offset), the start of the text the expression parser
/// reads, so [`to_value`](Self::to_value) turns them into ranges in the
/// value.
#[derive(Debug, Clone, PartialEq)]
pub struct Placeholder {
    /// Where the placeholder starts: at its `${{`, or at 0 for an `if`
    /// condition written without `${{ }}`.
    pub start: u32,
    /// Where the placeholder ends: just past its `}}`. After a parse error
    /// it is where the scan for the next `${{` goes on, which is past the
    /// `}}` if the lexer finds one, and otherwise past the character that
    /// the lexer rejects.
    pub end: u32,
    /// Where the expression's text starts: just past `${{`, or at 0 for an
    /// `if` condition written without `${{ }}`.
    pub offset: u32,
    /// The parsed expression, or the parser's first error.
    pub expr: Result<workflint_expr::Expr, ExprError>,
}

impl Placeholder {
    /// Returns the byte range of the placeholder in the value.
    #[must_use]
    pub fn range(&self) -> Range<usize> {
        to_usize(self.start)..to_usize(self.end)
    }

    /// Returns whether the placeholder is an `if` condition written without
    /// `${{ }}`, which GitHub evaluates as an expression all the same.
    #[must_use]
    pub fn is_implicit(&self) -> bool {
        self.start == self.offset
    }

    /// Turns a byte range in the expression's text, such as
    /// [`ExprError::range`] or [`Expr::range`](workflint_expr::Expr::range),
    /// into a byte range in the value.
    ///
    /// The `}}` that the parser reads after an implicit `if` condition is
    /// not in the value, so offsets in it become the end of the placeholder.
    #[must_use]
    pub fn to_value(&self, range: Range<usize>) -> Range<usize> {
        let offset = to_usize(self.offset);
        let end = to_usize(self.end);
        (offset + range.start).min(end)..(offset + range.end).min(end)
    }
}

/// Finds and parses every `${{ }}` in `value`, as the reference's
/// `checkExprsIn` does, except that a parse error does not stop the scan.
///
/// The scan looks for `${{` without regard to quotes, and lets the lexer
/// decide where the expression ends, so a `}}` inside a string literal does
/// not end it. After an expression, the scan goes on at its
/// [`end`](Placeholder::end).
pub(crate) fn scan(value: &str) -> Vec<Placeholder> {
    let mut placeholders = Vec::new();
    let mut pos = 0;
    while let Some(found) = value.get(pos..).and_then(|rest| rest.find("${{")) {
        let start = pos + found;
        let offset = start + "${{".len();
        let text = value.get(offset..).unwrap_or_default();
        let (expr, len) = match parse_expression(text) {
            Ok((expr, len)) => (Ok(expr), to_usize(len)),
            Err(err) => (Err(err), resume_after_error(text)),
        };
        pos = (offset + len).min(value.len());
        placeholders.push(Placeholder {
            start: to_u32(start),
            end: to_u32(pos),
            offset: to_u32(offset),
            expr,
        });
    }
    placeholders
}

/// Parses the `if` condition `value`, which has no `${{ }}`, as the
/// reference's `checkIfCondition` does: it appends the `}}` that ends an
/// expression and parses the result.
pub(crate) fn implicit(value: &str) -> Placeholder {
    let text = format!("{value}}}}}");
    Placeholder {
        start: 0,
        end: to_u32(value.len()),
        offset: 0,
        expr: parse_expression(&text).map(|(expr, _)| expr),
    }
}

/// Returns where the scan for the next `${{` goes on after a parse error in
/// `text`: past the `}}` that ends the expression if the lexer reaches it,
/// and otherwise past the character that the lexer rejects. A parse error
/// does not tell where the expression ends, so this lexes it again.
fn resume_after_error(text: &str) -> usize {
    let mut lexer = Lexer::new(text);
    while lexer.next_token().kind != TokenKind::End {}
    match lexer.error() {
        None => to_usize(lexer.offset()),
        Some(lex_err) => lex_err.range().end,
    }
}

/// Returns the byte offset in `value` of each `}}` that has no opening
/// `${{`.
///
/// It finds where each `${{ }}` ends as GitHub's template reader does: at
/// the first `}}` after `${{` that is not inside a single-quoted string. A
/// `${{` with no `}}` after it ends the search, because GitHub rejects the
/// string.
///
/// GitHub keeps the text between the placeholders as it is, so a `}}`
/// there may be meant, as in `docker inspect --format '{{.Id}}'` or in
/// JSON. A `}}` is stray only if neither of its braces closes a `{` in the
/// text before it. The text of all the gaps between placeholders counts,
/// so `{"a": ${{ x }}}` has no stray.
pub(crate) fn stray_closings(value: &str) -> Vec<u32> {
    let bytes = value.as_bytes();
    let mut strays = Vec::new();
    let mut depth = 0_usize;
    let mut pos = 0;
    loop {
        let open = value.get(pos..).and_then(|rest| rest.find("${{"));
        let text_end = open.map_or(value.len(), |found| pos + found);
        let mut i = pos;
        while i < text_end {
            match bytes.get(i) {
                Some(b'{') => depth += 1,
                Some(b'}') if depth > 0 => depth -= 1,
                Some(b'}') if i + 1 < text_end && bytes.get(i + 1) == Some(&b'}') => {
                    strays.push(to_u32(i));
                    i += 1;
                }
                _ => {}
            }
            i += 1;
        }
        let Some(end) = closing_end(value, text_end + "${{".len()) else {
            return strays;
        };
        pos = end;
    }
}

/// Returns the offset just past the `}}` that ends the `${{ }}` whose
/// expression starts at `from`, as GitHub's template reader finds it.
///
/// Every `'` starts or ends a string, so the `''` escape needs no special
/// case.
fn closing_end(value: &str, from: usize) -> Option<usize> {
    let bytes = value.as_bytes();
    let mut in_string = false;
    for i in from..bytes.len() {
        match bytes.get(i) {
            Some(b'\'') => in_string = !in_string,
            Some(b'}') if !in_string && i > 0 && bytes.get(i - 1) == Some(&b'}') => {
                return Some(i + 1);
            }
            _ => {}
        }
    }
    None
}

/// Returns the byte offset of the `}}` that ends the `if` condition
/// `value`, which has no `${{ }}`, if one ends it before the end of
/// `value`. The rest of the condition is not part of the expression.
pub(crate) fn implicit_stray_closing(value: &str) -> Option<u32> {
    let text = format!("{value}}}}}");
    let mut lexer = Lexer::new(&text);
    loop {
        let token = lexer.next_token();
        if token.kind == TokenKind::End {
            let early = lexer.error().is_none() && to_usize(token.start) < value.len();
            return early.then_some(token.start);
        }
    }
}

fn to_usize(offset: u32) -> usize {
    usize::try_from(offset).unwrap_or(usize::MAX)
}

fn to_u32(offset: usize) -> u32 {
    u32::try_from(offset).unwrap_or(u32::MAX)
}

/// Returns whether `s` contains a `${{ }}` placeholder, as the reference's
/// `ContainsExpression` decides it: the first `${{` comes before the first
/// `}}`.
///
/// A `}}` before the first `${{` makes the result false, so
/// `}} ${{ github.ref }}` has no placeholder.
///
/// ```
/// use workflint_syntax::contains_expression;
///
/// assert!(contains_expression("foo-${{ matrix.os }}-bar"));
/// assert!(!contains_expression("}} ${{ github.ref }}"));
/// ```
#[must_use]
pub fn contains_expression(s: &str) -> bool {
    match (s.find("${{"), s.find("}}")) {
        (Some(open), Some(close)) => open < close,
        _ => false,
    }
}

/// Returns whether `s` is a single `${{ }}` expression, as the reference's
/// `isExprAssigned` decides it: after trimming white space, `s` starts with
/// `${{`, ends with `}}`, and has no other `${{`.
///
/// Only `${{` is counted, so `${{ a }} b }}` is one expression. That keeps a
/// `}}` inside a string literal, as in `${{ x == '{"a": {}}' }}`, from
/// splitting the expression.
///
/// ```
/// use workflint_syntax::is_expression_assigned;
///
/// assert!(is_expression_assigned(" ${{ inputs.debug }} "));
/// assert!(!is_expression_assigned("v${{ inputs.version }}"));
/// ```
#[must_use]
pub fn is_expression_assigned(s: &str) -> bool {
    // Go's `strings.TrimSpace` trims Unicode white space, as `str::trim`
    // does.
    let v = s.trim();
    v.starts_with("${{") && v.ends_with("}}") && v.matches("${{").count() == 1
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic)]

    use super::*;

    /// Describes `placeholder` of `value`: its text, then the text of its
    /// expression, or its error and the byte range the error is about.
    fn describe(value: &str, placeholder: &Placeholder) -> String {
        let text = &value[placeholder.range()];
        match &placeholder.expr {
            Ok(expr) => format!("{text:?}: {:?}", &value[placeholder.to_value(expr.range())]),
            Err(err) => format!(
                "{text:?}: {} at {:?}",
                err.message,
                placeholder.to_value(err.range())
            ),
        }
    }

    fn placeholders(value: &str) -> Vec<String> {
        scan(value).iter().map(|p| describe(value, p)).collect()
    }

    #[test]
    fn a_string_without_placeholders_has_none() {
        assert_eq!(placeholders("echo hello }} {{"), Vec::<String>::new());
        assert_eq!(placeholders(""), Vec::<String>::new());
    }

    #[test]
    fn one_placeholder_is_parsed_with_offsets_in_the_string() {
        let value = "v${{ inputs.version }}-linux";
        let [placeholder] = &scan(value)[..] else {
            panic!("not one placeholder");
        };
        assert_eq!(
            (placeholder.start, placeholder.end, placeholder.offset),
            (1, 22, 4)
        );
        assert!(!placeholder.is_implicit());
        assert_eq!(
            describe(value, placeholder),
            r#""${{ inputs.version }}": "inputs.version""#
        );
    }

    #[test]
    fn several_placeholders_are_parsed_in_source_order() {
        assert_eq!(
            placeholders("${{ a }}${{ b }} and ${{ c == 1 }}"),
            [
                r#""${{ a }}": "a""#,
                r#""${{ b }}": "b""#,
                r#""${{ c == 1 }}": "c == 1""#,
            ]
        );
    }

    #[test]
    fn a_closing_pair_in_a_string_literal_does_not_end_the_placeholder() {
        assert_eq!(
            placeholders("${{ x == '}}' }} ${{ '${{' }}"),
            [
                r#""${{ x == '}}' }}": "x == '}}'""#,
                r#""${{ '${{' }}": "'${{'""#,
            ]
        );
    }

    #[test]
    fn an_unterminated_placeholder_is_an_error_at_the_end_of_the_string() {
        assert_eq!(
            placeholders("echo ${{ github.ref"),
            [r#""${{ github.ref": unexpected EOF while lexing expression at 19..19"#]
        );
    }

    #[test]
    fn every_placeholder_is_parsed_after_a_lexer_error() {
        let found = placeholders(r#"${{ "a" }} ${{ b }}"#);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(
            found[0].starts_with(r#""${{ \"": got unexpected character '"'"#),
            "{found:?}"
        );
        assert!(found[0].ends_with(" at 4..5"), "{found:?}");
        assert_eq!(found[1], r#""${{ b }}": "b""#);
    }

    #[test]
    fn every_placeholder_is_parsed_after_a_parser_error() {
        assert_eq!(
            placeholders("${{ a b }} ${{ c. }} ${{ d }}"),
            [
                r#""${{ a b }}": parser did not reach end of input after parsing the expression. 1 remaining token(s) in the input: "IDENT" at 6..7"#,
                r#""${{ c. }}": unexpected end of input while parsing object property dereference like 'a.b' or array element dereference like 'a.*'. expecting "IDENT", "*" at 18..20"#,
                r#""${{ d }}": "d""#,
            ]
        );
    }

    #[test]
    fn an_implicit_condition_is_the_whole_string() {
        let value = "github.ref == 'main'";
        let placeholder = implicit(value);
        assert!(placeholder.is_implicit());
        assert_eq!(placeholder.range(), 0..value.len());
        assert_eq!(
            describe(value, &placeholder),
            format!("{value:?}: {value:?}")
        );
    }

    #[test]
    fn an_implicit_condition_ends_at_its_first_closing_pair() {
        assert_eq!(describe("a }} b", &implicit("a }} b")), r#""a }} b": "a""#);
    }

    #[test]
    fn an_error_past_an_implicit_condition_is_at_its_end() {
        assert_eq!(
            describe("github.", &implicit("github.")),
            r#""github.": unexpected end of input while parsing object property dereference like 'a.b' or array element dereference like 'a.*'. expecting "IDENT", "*" at 7..7"#
        );
    }

    /// The cases of `TestStringIsExpressionAssigned` in the reference's
    /// `ast_test.go`.
    #[test]
    fn is_expression_assigned_passes_the_reference_cases() {
        let cases = [
            ("${{...}}", true),
            (" ${{...}} ", true),
            (r#"${{ foo == '{"a": {"b": "c"}}' }}"#, true),
            ("", false),
            ("${}", false),
            ("{{}}", false),
            ("${{", false),
            ("}}", false),
            ("${{ ${{ }}", false),
            ("abc ${{...}}", false),
            ("${{...}} abc", false),
            ("${{...}}${{...}}", false),
        ];
        for (input, want) in cases {
            assert_eq!(is_expression_assigned(input), want, "{input:?}");
        }
    }

    /// The cases of `TestStringContainsExpression` in the reference's
    /// `ast_test.go`.
    #[test]
    fn contains_expression_passes_the_reference_cases() {
        let cases = [
            ("${{...}}", true),
            ("foo-${{...}}-bar", true),
            ("${{...}}-${{...}}", true),
            ("${{...", false),
            ("...}}", false),
            ("${{...} }", false),
        ];
        for (input, want) in cases {
            assert_eq!(contains_expression(input), want, "{input:?}");
        }
    }

    #[test]
    fn a_closing_pair_before_the_first_opening_hides_the_expression() {
        assert!(!contains_expression("}} ${{ github.ref }}"));
        assert!(contains_expression("${{ a }} }}"));
    }

    #[test]
    fn unicode_white_space_around_an_expression_is_trimmed() {
        // U+00A0 and U+3000 are white space for Go's `unicode.IsSpace`.
        assert!(is_expression_assigned("\u{a0}${{ x }}\u{3000}\n"));
        assert!(is_expression_assigned("${{ a }} b }}"));
    }

    fn strays(value: &str) -> Vec<&str> {
        let offsets = stray_closings(value);
        offsets
            .iter()
            .map(|&offset| &value[to_usize(offset)..])
            .collect()
    }

    #[test]
    fn a_closing_pair_with_no_opening_is_stray() {
        assert_eq!(strays("echo }} done"), ["}} done"]);
        assert_eq!(strays("}}"), ["}}"]);
        assert_eq!(strays("a }}}} b"), ["}}}} b", "}} b"]);
    }

    #[test]
    fn a_closing_pair_after_a_closed_placeholder_is_stray() {
        assert_eq!(strays("${{ a }} }}"), ["}}"]);
        assert_eq!(strays("${{ a }}}"), Vec::<&str>::new());
        assert_eq!(strays("${{ a }} b }} ${{ c }}"), ["}} ${{ c }}"]);
        assert_eq!(strays("}} ${{ a }}"), ["}} ${{ a }}"]);
    }

    #[test]
    fn a_closing_pair_in_a_string_literal_is_not_stray() {
        assert_eq!(strays("${{ x == '}}' }}"), Vec::<&str>::new());
        assert_eq!(strays("${{ x == 'it''s }}' }} }}"), ["}}"]);
        assert_eq!(strays("${{ '{\"a\": {}}' }}"), Vec::<&str>::new());
    }

    #[test]
    fn a_placeholder_with_an_error_ends_at_its_closing_pair() {
        // The lexer stops at `"`, but the placeholder still ends at `}}`.
        assert_eq!(strays(r#"${{ "a" }} b"#), Vec::<&str>::new());
        assert_eq!(strays("${{ a b }} }}"), ["}}"]);
    }

    #[test]
    fn an_unterminated_placeholder_ends_the_search() {
        assert_eq!(strays("}} ${{ a"), ["}} ${{ a"]);
        assert_eq!(strays("${{ 'a }} }}"), Vec::<&str>::new());
    }

    #[test]
    fn a_closing_pair_that_closes_braces_in_the_text_is_not_stray() {
        assert_eq!(
            strays("docker inspect --format '{{.State.Status}}' app"),
            Vec::<&str>::new()
        );
        assert_eq!(strays(r#"echo '{"a": {"b": 1}}'"#), Vec::<&str>::new());
        assert_eq!(strays(r#"echo '{"a": {"b": ${{ x }}}}' }}"#), ["}}"]);
        assert_eq!(strays("{ a }} b }}"), ["}}"]);
        assert_eq!(strays("${A}} }} {"), ["}} {"]);
    }

    #[test]
    fn a_string_without_closing_pairs_has_no_strays() {
        assert_eq!(strays(""), Vec::<&str>::new());
        assert_eq!(strays("echo { } {{ ${{ a }}"), Vec::<&str>::new());
    }

    #[test]
    fn an_implicit_condition_with_a_closing_pair_before_its_end_has_a_stray() {
        assert_eq!(implicit_stray_closing("a }} b"), Some(2));
        assert_eq!(implicit_stray_closing("a }}"), Some(2));
        assert_eq!(implicit_stray_closing("}} ${{ a"), Some(0));
    }

    #[test]
    fn an_implicit_condition_without_a_closing_pair_has_no_stray() {
        assert_eq!(implicit_stray_closing("github.ref == 'main'"), None);
        assert_eq!(implicit_stray_closing("x == '}}'"), None);
        assert_eq!(implicit_stray_closing("a } b"), None);
        assert_eq!(implicit_stray_closing(""), None);
    }
}
