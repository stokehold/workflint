//! Formatting helpers that reproduce the Go functions actionlint builds its messages with.
//!
//! Rust's `{:?}` escapes strings differently from Go's `%q`, so messages that must be
//! byte-identical to actionlint's use these helpers instead.

use std::fmt::Write as _;

/// Quotes `s` with double quotes, as Go's `strconv.Quote` and `%q` do.
#[must_use]
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    push_quoted(&mut out, s);
    out
}

/// Quotes `c` with single quotes, as Go's `strconv.QuoteRune` and `%q` on a rune do.
#[must_use]
pub fn quote_rune(c: char) -> String {
    let mut out = String::with_capacity(c.len_utf8() + 2);
    out.push('\'');
    push_escaped(&mut out, c, '\'');
    out.push('\'');
    out
}

/// Quotes each string with [`quote`] and joins them with `", "`, as actionlint's `quotes` does.
#[must_use]
pub fn quotes<I>(items: I) -> String
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    let mut out = String::new();
    for item in items {
        if !out.is_empty() {
            out.push_str(", ");
        }
        push_quoted(&mut out, item.as_ref());
    }
    out
}

/// Sorts the strings by their bytes, then formats them like [`quotes`], as actionlint's
/// `sortedQuotes` does.
#[must_use]
pub fn sorted_quotes<I>(items: I) -> String
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    let mut items: Vec<I::Item> = items.into_iter().collect();
    items.sort_by(|lhs, rhs| lhs.as_ref().cmp(rhs.as_ref()));
    quotes(items)
}

/// Formats the strings of all groups like [`quotes`], in order, as actionlint's `quotesAll` does.
#[must_use]
pub fn quotes_all<G>(groups: G) -> String
where
    G: IntoIterator,
    G::Item: IntoIterator,
    <G::Item as IntoIterator>::Item: AsRef<str>,
{
    quotes(groups.into_iter().flatten())
}

/// Formats `n` as an English ordinal such as `1st`, `12th`, or `23rd`, as actionlint's `ordinal`
/// does.
#[must_use]
pub fn ordinal(n: usize) -> String {
    let suffix = match (n % 10, n % 100) {
        (1, 11) | (2, 12) | (3, 13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

fn push_quoted(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        push_escaped(out, c, '"');
    }
    out.push('"');
}

/// Appends `c` with Go's escaping rules for a literal delimited by `delimiter`.
fn push_escaped(out: &mut String, c: char, delimiter: char) {
    if c == delimiter || c == '\\' {
        out.push('\\');
        out.push(c);
        return;
    }
    if is_print(c) {
        out.push(c);
        return;
    }
    let escape = match c {
        '\u{7}' => "\\a",
        '\u{8}' => "\\b",
        '\u{c}' => "\\f",
        '\n' => "\\n",
        '\r' => "\\r",
        '\t' => "\\t",
        '\u{b}' => "\\v",
        _ => "",
    };
    if !escape.is_empty() {
        out.push_str(escape);
        return;
    }
    // Writing to a `String` cannot fail.
    let code = u32::from(c);
    let _ = if c < ' ' || c == '\u{7f}' {
        write!(out, "\\x{code:02x}")
    } else if code < 0x1_0000 {
        write!(out, "\\u{code:04x}")
    } else {
        write!(out, "\\U{code:08x}")
    };
}

/// Reports whether Go's `strconv.IsPrint` accepts `c`: letters, marks, numbers, punctuation,
/// symbols, and the ASCII space.
///
/// The answer depends on the Unicode version. Go 1.21 to 1.26 use Unicode 15.0.0, and so does
/// `unicode-general-category` 0.6.0, which is why `Cargo.toml` pins that version. Bump the crate
/// only to the release whose Unicode version matches the Go release that builds the reference.
fn is_print(c: char) -> bool {
    use unicode_general_category::GeneralCategory as Gc;

    if c == ' ' {
        return true;
    }
    match unicode_general_category::get_general_category(c) {
        Gc::UppercaseLetter
        | Gc::LowercaseLetter
        | Gc::TitlecaseLetter
        | Gc::ModifierLetter
        | Gc::OtherLetter
        | Gc::NonspacingMark
        | Gc::SpacingMark
        | Gc::EnclosingMark
        | Gc::DecimalNumber
        | Gc::LetterNumber
        | Gc::OtherNumber
        | Gc::ConnectorPunctuation
        | Gc::DashPunctuation
        | Gc::OpenPunctuation
        | Gc::ClosePunctuation
        | Gc::InitialPunctuation
        | Gc::FinalPunctuation
        | Gc::OtherPunctuation
        | Gc::MathSymbol
        | Gc::CurrencySymbol
        | Gc::ModifierSymbol
        | Gc::OtherSymbol => true,
        Gc::SpaceSeparator
        | Gc::LineSeparator
        | Gc::ParagraphSeparator
        | Gc::Control
        | Gc::Format
        | Gc::Surrogate
        | Gc::PrivateUse
        | Gc::Unassigned => false,
    }
}

// The expected values come from Go 1.26.4's `strconv.Quote` and `strconv.QuoteRune`.
#[cfg(test)]
mod tests {
    use super::*;

    // Ported from `TestQuotesSortedQuotes` in the reference's `quotes_test.go`.
    #[test]
    fn sorted_quotes_sorts_then_quotes() {
        let empty: [&str; 0] = [];
        assert_eq!(sorted_quotes(empty), "");
        assert_eq!(sorted_quotes(["foo"]), r#""foo""#);
        assert_eq!(
            sorted_quotes(["foo", "bar", "piyo"]),
            r#""bar", "foo", "piyo""#
        );
        assert_eq!(sorted_quotes(["\n", "\t"]), r#""\t", "\n""#);
    }

    // Ported from `TestQuotesQuotesRunes` in the reference's `quotes_test.go`, which joins
    // `strconv.QuoteRune` results with `", "`.
    #[test]
    fn quote_rune_matches_go_for_the_reference_cases() {
        let join = |runes: &[char]| {
            runes
                .iter()
                .map(|&c| quote_rune(c))
                .collect::<Vec<_>>()
                .join(", ")
        };
        assert_eq!(join(&[]), "");
        assert_eq!(join(&['a']), "'a'");
        assert_eq!(join(&['a', 'b', 'c']), "'a', 'b', 'c'");
        assert_eq!(join(&['\n', '\t', '\\']), r"'\n', '\t', '\\'");
    }

    // Ported from `TestQuotesQuotesAll` in the reference's `quotes_test.go`.
    #[test]
    fn quotes_all_quotes_every_group_in_order() {
        let empty: &[&str] = &[];
        assert_eq!(quotes_all([empty]), "");
        assert_eq!(quotes_all([&["foo"][..]]), r#""foo""#);
        assert_eq!(quotes_all([&["foo", "bar"][..]]), r#""foo", "bar""#);
        assert_eq!(quotes_all([&["foo"][..], &["bar"]]), r#""foo", "bar""#);
        assert_eq!(
            quotes_all([empty, &["foo"], &["bar", "piyo"]]),
            r#""foo", "bar", "piyo""#
        );
        assert_eq!(
            quotes_all([&["\n", "\t"][..], &["\u{b}"], empty]),
            r#""\n", "\t", "\v""#
        );
    }

    #[test]
    fn quotes_keeps_the_given_order() {
        assert_eq!(quotes(["b", "a"]), r#""b", "a""#);
        assert_eq!(quotes(Vec::<String>::new()), "");
    }

    #[test]
    fn quote_escapes_the_delimiter_and_backslash() {
        assert_eq!(quote(""), r#""""#);
        assert_eq!(quote(r#"say "hi""#), r#""say \"hi\"""#);
        assert_eq!(quote(r"a\b"), r#""a\\b""#);
        assert_eq!(quote("it's"), r#""it's""#);
    }

    #[test]
    fn quote_escapes_control_characters_like_go() {
        assert_eq!(quote("\0"), r#""\x00""#);
        assert_eq!(quote("\u{7}\u{8}\u{c}\u{b}\r"), r#""\a\b\f\v\r""#);
        assert_eq!(quote("\u{1b}[0m"), r#""\x1b[0m""#);
        assert_eq!(quote("\u{7f}"), r#""\x7f""#);
        assert_eq!(quote("\u{85}"), r#""\u0085""#);
    }

    #[test]
    fn quote_keeps_non_ascii_letters() {
        assert_eq!(quote("日本語"), "\"日本語\"");
        assert_eq!(quote("café"), "\"café\"");
        assert_eq!(quote("Ωμέγα"), "\"Ωμέγα\"");
        assert_eq!(quote("\u{1f600}"), "\"\u{1f600}\"");
        assert_eq!(quote("\u{fffd}"), "\"\u{fffd}\"");
    }

    #[test]
    fn quote_escapes_non_printable_code_points_like_go() {
        let cases = [
            ("\u{ad}", r#""\u00ad""#),
            ("\u{a0}", r#""\u00a0""#),
            ("\u{200b}", r#""\u200b""#),
            ("\u{2028}", r#""\u2028""#),
            ("\u{feff}", r#""\ufeff""#),
            ("\u{e000}", r#""\ue000""#),
            ("\u{378}", r#""\u0378""#),
            ("\u{e0001}", r#""\U000e0001""#),
            ("\u{10ffff}", r#""\U0010ffff""#),
        ];
        for (input, want) in cases {
            assert_eq!(quote(input), want, "{input:?}");
        }
    }

    #[test]
    fn quote_rune_escapes_like_go() {
        let cases = [
            ('\'', r"'\''"),
            ('"', "'\"'"),
            ('\0', r"'\x00'"),
            ('\u{ad}', r"'\u00ad'"),
            ('日', "'日'"),
            ('\u{1f600}', "'\u{1f600}'"),
            ('\u{e0001}', r"'\U000e0001'"),
            ('\u{2028}', r"'\u2028'"),
        ];
        for (input, want) in cases {
            assert_eq!(quote_rune(input), want, "{input:?}");
        }
    }

    #[test]
    fn unicode_version_matches_the_one_go_uses() {
        assert_eq!(
            unicode_general_category::UNICODE_VERSION,
            (15, 0, 0),
            "Go 1.21 to 1.26 use Unicode 15.0.0. Another version changes which characters \
             `quote` escapes, so the messages stop matching actionlint's. See `is_print`."
        );
    }

    #[test]
    fn ordinal_matches_the_reference() {
        let cases = [
            (1, "1st"),
            (2, "2nd"),
            (3, "3rd"),
            (4, "4th"),
            (11, "11th"),
            (12, "12th"),
            (13, "13th"),
            (21, "21st"),
            (22, "22nd"),
            (23, "23rd"),
            (101, "101st"),
            (111, "111th"),
            (112, "112th"),
        ];
        for (n, want) in cases {
            assert_eq!(ordinal(n), want);
        }
    }
}
