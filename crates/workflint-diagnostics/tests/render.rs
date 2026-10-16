//! Tests for the default and one-line renderers.
//!
//! The files in `actionlint/` come from the reference: `one_error.yaml` from `testdata/err/`,
//! and `escape.txt` and `no_escape.txt` from `scripts/generate-actionlint-matcher/test/`, which
//! hold its output for that file with and without color.

#![allow(clippy::unwrap_used)]

use workflint_diagnostics::{
    Category, Color, Diagnostic, FileId, RuleMeta, Severity, SourceFile, Span, render_default,
    render_oneline,
};

const fn rule(id: &'static str) -> RuleMeta {
    RuleMeta {
        id,
        kind: "expression",
        category: Category::Correctness,
        default_severity: Severity::Error,
        docs_url: "https://example.com",
    }
}

static EXPRESSION: RuleMeta = rule("expression");
static TEST_RULE: RuleMeta = rule("test-rule");

fn file(text: &str) -> SourceFile {
    SourceFile::new(FileId::new(0), "test.yaml", text).unwrap()
}

/// A diagnostic whose span covers the first occurrence of `token` in `file`.
fn at_token(file: &SourceFile, token: &str, message: &str) -> Diagnostic {
    let start = file.text().find(token).unwrap();
    span(start, start + token.len(), message)
}

fn span(start: usize, end: usize, message: &str) -> Diagnostic {
    let start = u32::try_from(start).unwrap();
    let end = u32::try_from(end).unwrap();
    Diagnostic::new(&TEST_RULE, Span::new(FileId::new(0), start, end), message)
}

fn default(file: &SourceFile, diagnostics: &[Diagnostic], color: Color) -> Vec<u8> {
    let mut out = Vec::new();
    render_default(&mut out, file, diagnostics, color).unwrap();
    out
}

fn default_plain(file: &SourceFile, diagnostics: &[Diagnostic]) -> String {
    String::from_utf8(default(file, diagnostics, Color::Off)).unwrap()
}

fn oneline(file: &SourceFile, diagnostics: &[Diagnostic], color: Color) -> String {
    let mut out = Vec::new();
    render_oneline(&mut out, file, diagnostics, color).unwrap();
    String::from_utf8(out).unwrap()
}

mod matcher_fixtures {
    use super::*;

    const MESSAGE: &str = "\"github.event.head_commit.message\" is potentially untrusted. avoid using it directly in inline scripts. instead, pass it through an environment variable. see https://docs.github.com/en/actions/reference/security/secure-use#good-practices-for-mitigating-script-injection-attacks for more details";

    /// The reference reports this error at line 6, column 41.
    fn one_error() -> (SourceFile, Diagnostic) {
        let file = SourceFile::new(
            FileId::new(0),
            "./testdata/err/one_error.yaml",
            include_str!("actionlint/one_error.yaml"),
        )
        .unwrap();
        let token = "github.event.head_commit.message";
        let start = file.text().find(token).unwrap();
        let start_u32 = u32::try_from(start).unwrap();
        let end = u32::try_from(start + token.len()).unwrap();
        let pos = file.line_col(start_u32);
        assert_eq!((pos.line, pos.column), (6, 41));
        let diagnostic =
            Diagnostic::new(&EXPRESSION, Span::new(file.id(), start_u32, end), MESSAGE);
        (file, diagnostic)
    }

    #[test]
    fn color_output_matches_escape_txt_byte_for_byte() {
        let (file, diagnostic) = one_error();
        let out = default(&file, &[diagnostic], Color::On);
        assert_eq!(
            String::from_utf8_lossy(&out),
            String::from_utf8_lossy(include_bytes!("actionlint/escape.txt"))
        );
    }

    #[test]
    fn plain_output_matches_no_escape_txt_byte_for_byte() {
        let (file, diagnostic) = one_error();
        let out = default(&file, &[diagnostic], Color::Off);
        assert_eq!(
            String::from_utf8_lossy(&out),
            String::from_utf8_lossy(include_bytes!("actionlint/no_escape.txt"))
        );
    }
}

// Ported from `TestErrorPrettyPrint` in the reference's `error_test.go`. The reference's errors
// have a column but no span, so each case uses an empty span, which falls back to the reference's
// underline rule. Three cases are not ported: a column past the end of its line, column 0, and
// line 0. A span always points into its file, so it cannot produce any of them.
#[test]
fn reference_pretty_print_cases_render_the_same() {
    let cases: [(&str, usize, &str); 11] = [
        ("this is source", 0, "1 | this is source\n  | ^~~~"),
        ("this is source", 5, "1 | this is source\n  |      ^~"),
        (
            "this is source",
            14,
            "1 | this is source\n  |               ^",
        ),
        ("foo . bar", 4, "1 | foo . bar\n  |     ^"),
        ("foo bar", 3, "1 | foo bar\n  |    ^"),
        ("this\nis\nsource", 10, "3 | source\n  |   ^~~~"),
        ("this\nis\nsource", 14, "3 | source\n  |       ^"),
        ("this\nis\nsource", 7, "2 | is\n  |   ^"),
        ("this\n\nsource", 5, "2 | \n  | ^"),
        ("\n\n\n\n\n\n\n\n\n\nfooo", 11, "11 | fooo\n   |  ^~~"),
        ("", 0, ""),
    ];
    for (source, offset, want) in cases {
        let file = SourceFile::new(FileId::new(0), "filename.txt", source).unwrap();
        let mut diagnostic = span(offset, offset, "msg");
        diagnostic.rule = &KIND;
        let pos = file.line_col(u32::try_from(offset).unwrap());
        let header = format!("filename.txt:{}:{}: msg [kind]\n", pos.line, pos.column);
        let want = if want.is_empty() {
            header
        } else {
            let gutter = " ".repeat(want.find(" | ").unwrap());
            format!("{header}{gutter} |\n{want}\n")
        };
        assert_eq!(
            default_plain(&file, &[diagnostic]),
            want,
            "{source:?} at {offset}"
        );
    }
}

static KIND: RuleMeta = rule("kind");

mod snapshots {
    use insta::assert_snapshot;

    use super::*;

    #[test]
    fn ascii_token_is_underlined_from_start_to_end() {
        let file = file("on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n");
        let d = at_token(&file, "ubuntu-latest", "label \"ubuntu-latest\" is unknown");
        assert_snapshot!(default_plain(&file, &[d]));
    }

    #[test]
    fn cjk_characters_before_the_column_take_two_cells_each() {
        let file = file("- run: echo \"日本語\" ${{ matrix.foo }}\n");
        let d = at_token(&file, "matrix.foo", "property \"foo\" is not defined");
        assert_snapshot!(default_plain(&file, &[d]));
    }

    #[test]
    fn an_emoji_in_the_span_is_underlined_with_two_cells() {
        let file = file("name: 🚀 deploy\n");
        let d = at_token(&file, "🚀 deploy", "bad name");
        assert_snapshot!(default_plain(&file, &[d]));
    }

    #[test]
    fn tabs_before_the_column_are_copied_into_the_indicator_line() {
        let file = file("\tkey:\tvalue\n");
        let d = at_token(&file, "value", "bad value");
        let out = default_plain(&file, &[d]);
        assert!(out.ends_with("  | \t    \t^~~~~\n"), "{out:?}");
        assert_snapshot!(out);
    }

    #[test]
    fn the_carriage_return_of_a_crlf_line_is_not_printed() {
        let file = file("on: push\r\njobs:\r\n  test: {}\r\n");
        let d = at_token(&file, "test", "job has no steps");
        let out = default_plain(&file, &[d]);
        assert!(!out.contains('\r'), "{out:?}");
        assert_snapshot!(out);
    }

    #[test]
    fn line_nine_has_a_one_digit_gutter() {
        let file = file(&format!("{}token\n", "\n".repeat(8)));
        let d = at_token(&file, "token", "msg");
        assert_snapshot!(default_plain(&file, &[d]));
    }

    #[test]
    fn line_ten_widens_the_gutter() {
        let file = file(&format!("{}token\n", "\n".repeat(9)));
        let d = at_token(&file, "token", "msg");
        assert_snapshot!(default_plain(&file, &[d]));
    }

    #[test]
    fn a_line_over_64_kib_still_gets_a_snippet() {
        const LEN: usize = 70_000;
        let file = file(&format!("{} token\n", "x".repeat(LEN)));
        let d = at_token(&file, "token", "msg");
        let out = default_plain(&file, &[d])
            .replace(&"x".repeat(LEN), "<70000 x>")
            .replace(&format!("| {}", " ".repeat(LEN + 1)), "| <70001 spaces>");
        assert_snapshot!(out);
    }

    #[test]
    fn a_span_past_the_end_of_its_line_is_underlined_to_the_end_of_the_line() {
        let file = file("run: |\n  echo one\n  echo two\n");
        let start = file.text().find("one").unwrap();
        let end = file.text().find("two").unwrap();
        let d = span(start, end, "msg");
        assert_snapshot!(default_plain(&file, &[d]));
    }

    #[test]
    fn a_span_at_the_end_of_a_line_gets_a_caret_after_the_last_character() {
        let file = file("key:\nnext: 1\n");
        let d = span(4, 4, "value is missing");
        assert_snapshot!(default_plain(&file, &[d]));
    }

    #[test]
    fn an_empty_span_is_underlined_to_the_end_of_the_non_blank_run() {
        let file = file("steps: run-this now\n");
        let start = file.text().find("run").unwrap();
        let d = span(start, start, "msg");
        assert_snapshot!(default_plain(&file, &[d]));
    }

    #[test]
    fn a_file_without_source_prints_only_the_header() {
        let file = file("");
        let d = span(0, 0, "workflow is empty");
        assert_snapshot!(default_plain(&file, &[d]));
    }

    #[test]
    fn several_diagnostics_print_one_block_each_in_the_given_order() {
        let file = file("on: push\njobs: {}\n");
        let first = at_token(&file, "push", "first");
        let second = at_token(&file, "{}", "second");
        assert_snapshot!(default_plain(&file, &[first, second]));
    }
}

#[test]
fn a_position_at_the_end_of_the_file_after_a_newline_prints_only_the_header() {
    // The reference's line reader finds no line 2 in "a\n".
    let file = file("a\n");
    let d = span(2, 2, "msg");
    assert_eq!(
        default_plain(&file, &[d]),
        "test.yaml:2:1: msg [test-rule]\n"
    );
}

#[test]
fn snippet_lines_end_at_every_yaml_line_break() {
    for brk in ["\r", "\u{85}", "\u{2028}", "\u{2029}"] {
        let file = file(&format!("a: 1{brk}b: x{brk}c: 3"));
        let d = at_token(&file, "x", "msg");
        assert_eq!(
            default_plain(&file, &[d]),
            "test.yaml:2:4: msg [test-rule]\n  |\n2 | b: x\n  |    ^\n",
            "{brk:?}"
        );
    }
}

#[test]
fn oneline_prints_only_the_header_without_color() {
    let file = file("on: push\njobs: {}\n");
    let d = at_token(&file, "{}", "\"jobs\" section is empty");
    assert_eq!(
        oneline(&file, &[d], Color::Off),
        "test.yaml:2:7: \"jobs\" section is empty [test-rule]\n"
    );
}

#[test]
fn oneline_prints_the_colored_header_bytes_with_color() {
    let file = file("on: push\njobs: {}\n");
    let d = at_token(&file, "{}", "msg");
    assert_eq!(
        oneline(&file, &[d], Color::On),
        "\x1b[33mtest.yaml\x1b[0m\x1b[90m:\x1b[0m2\x1b[90m:\x1b[0m7\x1b[90m: \x1b[0m\x1b[1mmsg\x1b[0m\x1b[90m [test-rule]\n\x1b[0m"
    );
}
