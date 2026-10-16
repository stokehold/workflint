//! The reference's tests for `${{ }}` in a string, quirks included.

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
    use super::*;

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
}
