//! The placeholders in the format string of `format()`.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `parseFormatFuncSpecifiers` in
//! `expr_sema.go`, lines 31-96.

use std::collections::BTreeSet;

/// The largest placeholder index. Go's `strconv.Atoi` returns the largest `int` of a 64-bit
/// platform for a larger number.
const MAX_INDEX: u64 = 9_223_372_036_854_775_807;

/// Returns the indices of the placeholders in `format`, such as `0` and `1` for
/// `'{0} and {1}'`.
///
/// `{{` and `}}` are escaped braces. A run of `}` after a placeholder closes it only when the
/// run is odd: `{0}}}` has a placeholder, and `{0}}` and `{0}}}}` do not. Malformed braces such
/// as `{}`, `{x}`, and a lone `{` are text.
pub(crate) fn placeholders(format: &str) -> BTreeSet<u64> {
    // `start` is the offset after the `{` that opens a placeholder, and `end` the offset of the
    // `}` that closes its digits.
    let mut found = BTreeSet::new();
    let mut start: Option<usize> = None;
    let mut end: Option<usize> = None;

    for (i, c) in format.char_indices() {
        let Some(open) = start else {
            if c == '{' {
                start = Some(i + 1);
            }
            continue;
        };

        let Some(close) = end else {
            if c == '{' && i == open {
                // `{{`, an escaped `{`.
                start = None;
            } else if c == '}' {
                if i == open {
                    // `{}`, which is no placeholder.
                    start = None;
                } else {
                    end = Some(i);
                }
            } else if !c.is_ascii_digit() {
                start = (c == '{').then_some(i + 1);
            }
            continue;
        };

        // After the digits, wait for the run of `}` to end.
        if c == '}' {
            continue;
        }
        if (i - close) % 2 == 1 {
            found.insert(index(&format[open..close]));
        }
        start = (c == '{').then_some(i + 1);
        end = None;
    }

    // The input ends in the run of `}`.
    if let (Some(open), Some(close)) = (start, end)
        && (format.len() - close) % 2 == 1
    {
        found.insert(index(&format[open..close]));
    }
    found
}

/// Returns the value of `digits`, ASCII digits, as Go's `strconv.Atoi` does.
fn index(digits: &str) -> u64 {
    digits.bytes().fold(0, |value: u64, digit| {
        value
            .checked_mul(10)
            .and_then(|value| value.checked_add(u64::from(digit - b'0')))
            .map_or(MAX_INDEX, |value| value.min(MAX_INDEX))
    })
}

/// `TestParseFormatSpecifiers`, one test per case.
#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! cases {
        ($($name:ident: $input:literal => [$($index:literal),*],)*) => {
            $(
                #[test]
                fn $name() {
                    let expected: BTreeSet<u64> = [$($index),*].into_iter().collect();
                    assert_eq!(placeholders($input), expected, "{}", $input);
                }
            )*
        };
    }

    cases! {
        empty_input: "" => [],
        no_specifier: "hello, world!" => [],
        single_specifier: "Hello{0}specifier" => [0],
        multiple_specifiers: "{0} {1}{2}x{3}_{4}!" => [0, 1, 2, 3, 4],
        many_specifiers: "{0}{1}{2}{3}{4}{5}{6}{7}{8}{9}{10}!" => [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
        unordered: "{2}foo{4} {0}{3}" => [2, 4, 0, 3],
        uncontiguous: "{0} {2}foo{42} {1}" => [0, 2, 42, 1],
        unclosed: "{12foo" => [],
        missing_closing_brace: "{0" => [],
        not_digit: "{hello}" => [],
        space_in_digits: "{1 2}" => [],
        empty: "{}" => [],
        opening_brace_only: "{" => [],
        specifier_inside_specifier: "{1{0}2}" => [0],
        escaped_single: "{{0}}" => [],
        complicated_escaped_format: "{{hello{{0}{{{{1}world}}" => [],
        placeholders_inside_escape: "{{{0} {1} {2}}}" => [0, 1, 2],
        no_extra_brace: "{0}" => [0],
        closing_brace_1: "{0}}" => [],
        closing_brace_2: "{0}}}" => [0],
        closing_brace_3: "{0}}}}" => [],
        closing_brace_4: "{0}}}}}" => [0],
        closing_brace_1_with_trailing_character: "{0}}x" => [],
        closing_brace_2_with_trailing_character: "{0}}}x" => [0],
        closing_brace_3_with_trailing_character: "{0}}}}x" => [],
        closing_brace_4_with_trailing_character: "{0}}}}}x" => [0],
        opening_brace_1: "{{0}" => [],
        opening_brace_2: "{{{0}" => [0],
        opening_brace_3: "{{{{0}" => [],
        opening_brace_4: "{{{{{0}" => [0],
        opening_brace_after_escaped_closing_brace: "{1}}{2}" => [2],
        escaped_opening_brace_after_escaped_closing_brace: "{1}}{{2}" => [],
        escaped_opening_brace_after_closing_brace: "{1}{{2}" => [1],
        kuma: "{・{ᴥ}・}" => [],
        multi_byte: "あ{0}い{1}う{2}え{3}お" => [0, 1, 2, 3],
        emoji: "🐶{0}👨‍👩‍👦‍👦{1}🇺🇸{2}🐦‍🔥" => [0, 1, 2],
    }

    #[test]
    fn leading_zeros_are_part_of_the_number() {
        assert_eq!(placeholders("{007}"), BTreeSet::from([7]));
    }

    #[test]
    fn index_beyond_the_largest_integer_is_the_largest_integer() {
        assert_eq!(
            placeholders("{99999999999999999999}"),
            BTreeSet::from([MAX_INDEX])
        );
        assert_eq!(
            placeholders("{9223372036854775808}"),
            BTreeSet::from([MAX_INDEX])
        );
    }
}
