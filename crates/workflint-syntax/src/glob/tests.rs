//! The test vectors of the reference's `glob_test.go`, with the full
//! messages and columns that the reference returns, and a few more cases.

use super::*;

const ZERO_OR_ONE: &str = "invalid glob pattern. unexpected character '?' while checking special character ? (zero or one). the preceding character must not be special character";
const ONE_OR_MORE: &str = "invalid glob pattern. unexpected character '+' while checking special character + (one or more). the preceding character must not be special character";
const NEGATE_ALONE: &str = "invalid glob pattern. unexpected character '!' while checking ! at first character (negate pattern). at least one character must follow !";
const NEWLINE: &str =
    "invalid glob pattern. unexpected character '\\n'. newline cannot be contained";
const EMPTY_MATCH: &str = "invalid glob pattern. unexpected character ']' while checking content of character match []. character match must not be empty";
const MISSING_BRACKET: &str =
    "invalid glob pattern. unexpected EOF while checking end of character match []. missing ]";
const MISSING_RANGE_END: &str = "invalid glob pattern. unexpected character ']' while checking character range in []. end of range is missing";
const B_TO_A: &str = "invalid glob pattern. unexpected character 'a' while checking character range in []. start of range 'b' (98) is larger than end of range 'a' (97)";
const SINGLE_CHAR_MATCH: &str = "invalid glob pattern. unexpected character ']' while checking character match []. character match with single character is useless. simply use x instead of [x]";
const START_SLASH: &str = "character '/' is invalid for branch and tag names. ref name must not start with /. see `man git-check-ref-format` for more details. note that regular expression is unavailable";
const END_SLASH: &str = "character '/' is invalid for branch and tag names. ref name must not end with / and .. see `man git-check-ref-format` for more details. note that regular expression is unavailable";
const END_DOT: &str = "character '.' is invalid for branch and tag names. ref name must not end with / and .. see `man git-check-ref-format` for more details. note that regular expression is unavailable";
const BAD_ESCAPE: &str = "character '\\' is invalid for branch and tag names. only special characters [, ?, +, *, \\, ! can be escaped with \\. see `man git-check-ref-format` for more details. note that regular expression is unavailable";
const SPACE: &str = "character ' ' is invalid for branch and tag names. ref name cannot contain spaces, ~, ^, :, [, ?, *. see `man git-check-ref-format` for more details. note that regular expression is unavailable";
const PATH_SPACES: &str = "leading and trailing spaces are not allowed in glob path";
const PATH_DOT: &str = "'.' and '..' are not allowed in glob path";

/// Returns the message of a problem in a branch or tag name about the
/// character `c`, which a ref name cannot contain.
fn cannot_contain(c: &str) -> String {
    format!(
        "character {c} is invalid for branch and tag names. ref name cannot contain spaces, ~, ^, :, [, ?, *. see `man git-check-ref-format` for more details. note that regular expression is unavailable"
    )
}

/// Returns the column that the reference reports for `problem`: the 1-based
/// position in characters of the start of its range, or 0 if it has none.
///
/// `rule_glob.go` adds this column minus 1 to the position of the scalar, so
/// equal columns give equal positions.
fn reference_column(pattern: &str, problem: &GlobProblem) -> usize {
    problem
        .range
        .as_ref()
        .map_or(0, |range| pattern[..range.start].chars().count() + 1)
}

/// Returns the message and the reference column of each problem.
fn summary(pattern: &str, problems: &[GlobProblem]) -> Vec<(String, usize)> {
    problems
        .iter()
        .map(|problem| (problem.message.clone(), reference_column(pattern, problem)))
        .collect()
}

fn ref_problems(pattern: &str) -> Vec<(String, usize)> {
    summary(pattern, &validate_ref_glob(pattern))
}

fn path_problems(pattern: &str) -> Vec<(String, usize)> {
    summary(pattern, &validate_path_glob(pattern))
}

fn owned(expected: &[(&str, usize)]) -> Vec<(String, usize)> {
    expected
        .iter()
        .map(|&(message, column)| (message.to_owned(), column))
        .collect()
}

#[test]
fn valid_patterns_have_no_problems_in_either_mode() {
    let patterns = [
        "a",
        "abc/def",
        "[ab]",
        "[a-z]",
        "[a-zA-Z_]",
        "[xA-Zy0-9z]",
        "[xy][A-Z][yz][0-9][zx]",
        "xy[A-Z]yz[0-9]zx",
        "*",
        "**",
        "*/*",
        "*/**",
        "a?",
        "a+",
        r"\+",
        r"\\",
        r"\!",
        r"foo\!bar",
        r"\++\\?",
        "[a-z]+",
        "[a-z]?",
        "*.*.*-**",
        "!a",
        "a!",
        "a!+",
        "a!?",
        "!*",
        "feature/*",
        "feature/**",
        "main",
        "releases/mona-the-octcat",
        "*feature",
        "v2*",
        "v[12].[0-9]+.[0-9]+",
        ".foo.txt",
        "..foo.txt",
    ];

    for pattern in patterns {
        assert_eq!(validate_ref_glob(pattern), [], "ref glob {pattern:?}");
        assert_eq!(validate_path_glob(pattern), [], "path glob {pattern:?}");
    }
}

#[test]
fn ref_name_characters_are_valid_in_path_globs() {
    let patterns = [
        "/foo",
        "/foo/",
        "/foo/bar",
        r"\[\?\*\+\\",
        "foo bar",
        "~/foo",
        "foo:bar",
        "foo^bar",
        "*.jsx?",
        "*.js",
        "**.js",
        "docs/*",
        "docs/**",
        "docs/**/*.md",
        "**/docs/**",
        "**/README.md",
        "**/*src/**",
        "*/*-post.md",
        "**/migrate-*.sql",
        "!README.md",
    ];

    for pattern in patterns {
        assert_eq!(validate_path_glob(pattern), [], "path glob {pattern:?}");
    }
}

#[test]
fn syntax_errors_are_reported_in_both_modes() {
    let cases: &[(&str, &[(&str, usize)])] = &[
        ("", &[("glob pattern cannot be empty", 0)]),
        ("!", &[(NEGATE_ALONE, 1)]),
        ("?", &[(ZERO_OR_ONE, 1)]),
        ("*?", &[(ZERO_OR_ONE, 2)]),
        ("a??", &[(ZERO_OR_ONE, 3)]),
        ("+", &[(ONE_OR_MORE, 1)]),
        ("*+", &[(ONE_OR_MORE, 2)]),
        ("a++", &[(ONE_OR_MORE, 3)]),
        ("foo\nbar", &[(NEWLINE, 0)]),
        (
            "foo\rbar",
            &[(
                "invalid glob pattern. unexpected character '\\r'. newline cannot be contained",
                4,
            )],
        ),
        ("foo\r\nbar", &[(NEWLINE, 0)]),
        ("[]", &[(EMPTY_MATCH, 2)]),
        ("[", &[(MISSING_BRACKET, 1)]),
        ("[a", &[(MISSING_BRACKET, 2)]),
        ("[a-c", &[(MISSING_BRACKET, 4)]),
        ("[a-]", &[(MISSING_RANGE_END, 4)]),
        ("[a-", &[(MISSING_BRACKET, 3)]),
        ("[b-a]", &[(B_TO_A, 4)]),
        ("[x]", &[(SINGLE_CHAR_MATCH, 3)]),
        (
            "+?[][a-]*+\n[b",
            &[
                (ONE_OR_MORE, 1),
                (ZERO_OR_ONE, 2),
                (EMPTY_MATCH, 4),
                (MISSING_RANGE_END, 8),
                (ONE_OR_MORE, 10),
                (NEWLINE, 0),
                (MISSING_BRACKET, 0),
            ],
        ),
        ("!?", &[(ZERO_OR_ONE, 2)]),
        ("!+", &[(ONE_OR_MORE, 2)]),
    ];

    for &(pattern, expected) in cases {
        assert_eq!(
            ref_problems(pattern),
            owned(expected),
            "ref glob {pattern:?}"
        );
        assert_eq!(
            path_problems(pattern),
            owned(expected),
            "path glob {pattern:?}"
        );
    }
}

#[test]
fn characters_that_ref_names_cannot_contain_are_reported() {
    let open = cannot_contain("'['");
    let question = cannot_contain("'?'");
    let star = cannot_contain("'*'");
    let tab = cannot_contain(r"'\t'");
    let tilde = cannot_contain("'~'");
    let caret = cannot_contain("'^'");
    let colon = cannot_contain("':'");
    let cases: &[(&str, &[(&str, usize)])] = &[
        ("/foo", &[(START_SLASH, 1)]),
        ("foo/", &[(END_SLASH, 4)]),
        ("foo.", &[(END_DOT, 4)]),
        (r"\[\?\*", &[(&open, 2), (&question, 4), (&star, 6)]),
        (r"\d", &[(BAD_ESCAPE, 1)]),
        (
            " \t~^:",
            &[(SPACE, 1), (&tab, 2), (&tilde, 3), (&caret, 4), (&colon, 5)],
        ),
        (
            r"/^v\d+\.\d+\.(x|\d+)$/",
            &[
                (START_SLASH, 1),
                (&caret, 2),
                (BAD_ESCAPE, 4),
                (BAD_ESCAPE, 7),
                (BAD_ESCAPE, 9),
                (BAD_ESCAPE, 12),
                (BAD_ESCAPE, 17),
                (END_SLASH, 22),
            ],
        ),
    ];

    for &(pattern, expected) in cases {
        assert_eq!(
            ref_problems(pattern),
            owned(expected),
            "ref glob {pattern:?}"
        );
    }
}

#[test]
fn columns_match_the_reference() {
    let cases = [
        ("", 0),
        ("!", 1),
        ("?", 1),
        ("+", 1),
        ("*?", 2),
        ("a++", 3),
        ("a\n", 0),
        ("[]", 2),
        ("[0", 2),
        ("[0-]", 4),
        ("[0-", 3),
        ("[b-a]", 4),
        ("/foo", 1),
        ("foo/", 4),
        ("foo.", 4),
        (r"\[", 2),
        ("foo bar", 4),
    ];

    for (pattern, column) in cases {
        let problems = ref_problems(pattern);
        let columns: Vec<usize> = problems.iter().map(|&(_, column)| column).collect();
        assert_eq!(columns, [column], "ref glob {pattern:?}: {problems:?}");
    }
}

#[test]
fn messages_quote_characters_like_the_reference() {
    let cases = [("!", "'!'"), (r"\d", r"'\'"), ("\n", r"'\n'")];

    for (pattern, quoted) in cases {
        let problems = validate_ref_glob(pattern);
        let first = problems.first().map(|problem| problem.message.as_str());
        assert!(
            first.is_some_and(|message| message.contains(quoted)),
            "ref glob {pattern:?}: {first:?} does not contain {quoted}"
        );
    }
}

#[test]
fn path_globs_reject_surrounding_spaces_and_dot_directories() {
    let cases = [
        ("   foo.txt", PATH_SPACES),
        ("foo.txt\n", PATH_SPACES),
        (".", PATH_DOT),
        ("..", PATH_DOT),
        ("!.", PATH_DOT),
        ("./foo.txt", PATH_DOT),
        ("../foo.txt", PATH_DOT),
        ("!./foo.txt", PATH_DOT),
    ];

    for (pattern, message) in cases {
        assert_eq!(
            validate_path_glob(pattern),
            [GlobProblem {
                kind: if message == PATH_DOT {
                    GlobProblemKind::PathDot
                } else {
                    GlobProblemKind::PathWhitespace
                },
                message: message.to_owned(),
                range: None,
            }],
            "path glob {pattern:?}"
        );
    }
}

#[test]
fn unicode_spaces_around_a_path_glob_are_reported() {
    for pattern in ["\u{a0}foo", "foo\u{3000}", "\u{85}foo", "\t"] {
        assert_eq!(
            path_problems(pattern),
            owned(&[(PATH_SPACES, 0)]),
            "{pattern:?}"
        );
    }
}

#[test]
fn an_escaped_special_character_in_a_ref_glob_reports_that_character() {
    let open = cannot_contain("'['");
    let question = cannot_contain("'?'");
    let star = cannot_contain("'*'");

    assert_eq!(ref_problems(r"\["), owned(&[(&open, 2)]));
    assert_eq!(ref_problems(r"\[x"), owned(&[(&open, 2)]));
    assert_eq!(
        ref_problems(r"\[\?\*a"),
        owned(&[(&open, 2), (&question, 4), (&star, 6)])
    );
}

#[test]
fn a_backslash_before_another_character_in_a_ref_glob_is_reported() {
    assert_eq!(ref_problems("\\"), owned(&[(BAD_ESCAPE, 1)]));
    assert_eq!(ref_problems("a\\"), owned(&[(BAD_ESCAPE, 2)]));
    assert_eq!(ref_problems(r"\."), owned(&[(BAD_ESCAPE, 1), (END_DOT, 2)]));
    assert_eq!(path_problems(r"\."), []);
}

#[test]
fn a_negated_ref_glob_checks_the_character_after_the_negation() {
    assert_eq!(ref_problems("!/"), owned(&[(END_SLASH, 2)]));
    assert_eq!(ref_problems("!."), owned(&[(END_DOT, 2)]));
    assert_eq!(ref_problems("/"), owned(&[(START_SLASH, 1)]));
}

#[test]
fn a_negation_inside_a_character_match_is_a_literal_character() {
    // The reference has no `[!...]` syntax, so `[!a]` matches `!` or `a`.
    for pattern in ["[!a]", "[!ab]"] {
        assert_eq!(validate_ref_glob(pattern), [], "{pattern:?}");
        assert_eq!(validate_path_glob(pattern), [], "{pattern:?}");
    }
}

#[test]
fn a_newline_inside_a_character_match_is_not_reported() {
    assert_eq!(validate_ref_glob("[a\nb]"), []);
}

#[test]
fn a_newline_ends_the_special_character_before_it() {
    assert_eq!(ref_problems("a\n?"), owned(&[(NEWLINE, 0)]));
}

#[test]
fn a_nul_character_reports_a_scanner_error_at_the_character_before_it() {
    let nul = |pattern: &str| {
        format!(
            "error while scanning glob pattern {}: invalid character NUL",
            workflint_diagnostics::quote::quote(pattern)
        )
    };

    assert_eq!(ref_problems("a\0b"), owned(&[(&nul("a\0b"), 1)]));
    assert_eq!(path_problems("a\0b"), owned(&[(&nul("a\0b"), 1)]));
    assert_eq!(ref_problems("\0"), owned(&[(&nul("\0"), 0)]));
    assert_eq!(
        ref_problems("\u{feff}a\nb\0"),
        owned(&[(NEWLINE, 0), (&nul("\u{feff}a\nb\0"), 0)])
    );
}

#[test]
fn a_leading_byte_order_mark_is_skipped_but_counted_in_columns() {
    let nul = format!(
        "error while scanning glob pattern {}: invalid character NUL",
        r#""\ufeff\x00""#
    );

    assert_eq!(ref_problems("\u{feff}?"), owned(&[(ZERO_OR_ONE, 2)]));
    assert_eq!(ref_problems("\u{feff}\0"), owned(&[(&nul, 1)]));
}

#[test]
fn ranges_cover_whole_characters() {
    let problems = validate_ref_glob("日本[b-a]");
    let ranges: Vec<_> = problems
        .iter()
        .map(|problem| problem.range.clone())
        .collect();
    assert_eq!(ranges, [Some(9..10)]);
    assert_eq!(ref_problems("日本[b-a]"), owned(&[(B_TO_A, 6)]));

    let problems = validate_ref_glob("é[b-é");
    let ranges: Vec<_> = problems
        .iter()
        .map(|problem| problem.range.clone())
        .collect();
    assert_eq!(ranges, [Some(5..7)]);

    let range_message = "invalid glob pattern. unexpected character 'a' while checking character range in []. start of range 'é' (233) is larger than end of range 'a' (97)";
    assert_eq!(ref_problems("[é-a]"), owned(&[(range_message, 4)]));
    assert_eq!(validate_ref_glob("é?"), []);
}

#[test]
fn each_problem_has_the_kind_of_its_message() {
    let kinds = |problems: Vec<GlobProblem>| -> Vec<GlobProblemKind> {
        problems.into_iter().map(|problem| problem.kind).collect()
    };
    assert_eq!(kinds(validate_ref_glob("")), [GlobProblemKind::Syntax]);
    assert_eq!(kinds(validate_ref_glob("v[0-")), [GlobProblemKind::Syntax]);
    assert_eq!(
        kinds(validate_ref_glob("a\\0b")),
        [GlobProblemKind::RefName]
    );
    assert_eq!(kinds(validate_ref_glob("a\nb")), [GlobProblemKind::Syntax]);
    assert_eq!(
        kinds(validate_ref_glob("^a b")),
        [GlobProblemKind::RefName, GlobProblemKind::RefName]
    );
    assert_eq!(kinds(validate_path_glob("a\0b")), [GlobProblemKind::Syntax]);
    assert_eq!(
        kinds(validate_path_glob(" ./a")),
        [GlobProblemKind::PathWhitespace, GlobProblemKind::PathDot]
    );
}
