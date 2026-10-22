//! The output of `-format summary`: a Markdown report for the job summary of a GitHub Actions
//! run (ADR 0015).
//!
//! [`Rows::of`] builds the table rows of one file, and [`write()`] prints a heading, the line of
//! `-summary`, and one table over every file in the order of the output. GitHub rejects a step
//! summary over 1 MiB, so rows that would pass [`SIZE_LIMIT`] are left out and a line says how
//! many. `docs/github-output.md` describes the output.

use std::fmt::Write as _;
use std::io::{self, Write};

use workflint_diagnostics::{Diagnostic, RuleMeta, SourceFile};

/// The largest summary [`write()`] prints, in bytes: GitHub's limit of 1 MiB for one step,
/// less room for the line about the rows left out.
pub(crate) const SIZE_LIMIT: usize = 1024 * 1024 - 1024;

/// The table rows of one file.
#[derive(Debug, Clone)]
pub(crate) struct Rows(Vec<Row>);

/// One table row, without the link to its rule.
#[derive(Debug, Clone)]
struct Row {
    rule: &'static RuleMeta,
    /// The severity, the location, and the message, escaped.
    cells: [String; 3],
}

impl Rows {
    /// The rows of `diagnostics`, which are in `file`, in the order of `diagnostics`.
    #[must_use]
    pub(crate) fn of(file: &SourceFile, diagnostics: &[Diagnostic]) -> Self {
        let rows = diagnostics
            .iter()
            .map(|diagnostic| {
                let pos = file.line_col(diagnostic.span.start);
                let location = format!("{}:{}:{}", file.path(), pos.line, pos.column);
                let mut message = escape(&diagnostic.message);
                if let Some(help) = &diagnostic.help {
                    message.push_str("<br>help: ");
                    message.push_str(&escape(help));
                }
                Row {
                    rule: diagnostic.rule,
                    cells: [
                        diagnostic.severity.as_str().to_owned(),
                        escape(&location),
                        message,
                    ],
                }
            })
            .collect();
        Self(rows)
    }
}

/// Writes the summary of `files`: a heading, `counts`, which is the line of `-summary`, and a table of
/// the diagnostics if there are any. The rows that would make the summary longer than `limit`
/// bytes are left out, and a line says how many.
///
/// # Errors
///
/// Returns the error of writing to `out`.
pub(crate) fn write<W: Write + ?Sized>(
    out: &mut W,
    counts: &str,
    files: &[Rows],
    limit: usize,
) -> io::Result<()> {
    let mut text = format!("## workflint\n\n{counts}\n");
    let rows: Vec<&Row> = files.iter().flat_map(|file| &file.0).collect();
    if !rows.is_empty() {
        text.push_str("\n| Severity | Location | Rule | Message |\n| --- | --- | --- | --- |\n");
    }
    let mut left_out = 0;
    for row in &rows {
        let [severity, location, message] = &row.cells;
        let line = format!(
            "| {severity} | {location} | [{}]({}) | {message} |\n",
            row.rule.id, row.rule.docs_url
        );
        if left_out > 0 || text.len() + line.len() > limit {
            left_out += 1;
        } else {
            text.push_str(&line);
        }
    }
    if left_out > 0 {
        let (problems, are) = if left_out == 1 {
            ("problem", "is")
        } else {
            ("problems", "are")
        };
        // Writing to a `String` cannot fail.
        let _ = write!(
            text,
            "\n{left_out} more {problems} {are} not listed, because GitHub limits the summary of \
             a step to 1 MiB.\n"
        );
    }
    out.write_all(text.as_bytes())
}

/// Escapes `text` for a table cell: a backslash before each ASCII punctuation character, so that
/// no character starts Markdown or HTML, and `<br>` for each line break.
fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                escaped.push_str("<br>");
            }
            '\n' => escaped.push_str("<br>"),
            c if c.is_ascii_punctuation() => {
                escaped.push('\\');
                escaped.push(c);
            }
            c => escaped.push(c),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use workflint_diagnostics::{Category, FileId, Severity, Span};

    use super::*;

    const fn rule(id: &'static str, severity: Severity) -> RuleMeta {
        RuleMeta {
            id,
            description: "A test rule reports something",
            kind: "syntax-check",
            category: Category::Correctness,
            default_severity: severity,
            docs_url: "https://example.com/rules/test.md",
        }
    }

    static ERROR_RULE: RuleMeta = rule("error-rule", Severity::Error);
    static INFO_RULE: RuleMeta = rule("info-rule", Severity::Info);

    fn file(path: &str, text: &str) -> SourceFile {
        SourceFile::new(FileId::new(0), path, text).unwrap()
    }

    fn span(file: &SourceFile, token: &str) -> Span {
        let start = u32::try_from(file.text().find(token).unwrap()).unwrap();
        let end = start + u32::try_from(token.len()).unwrap();
        Span::new(file.id(), start, end)
    }

    fn summary_text(files: &[Rows], limit: usize) -> String {
        let mut out = Vec::new();
        write(
            &mut out,
            "2 problems in 2 files: 1 error, 0 warnings, 1 info",
            files,
            limit,
        )
        .unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn markdown_and_html_in_a_message_are_escaped() {
        let a = file("a_b.yaml", "x: tok\n");
        let b = file("dir/b.yaml", "y: tok\n");
        let files = [
            Rows::of(
                &a,
                &[Diagnostic::new(
                    &ERROR_RULE,
                    span(&a, "tok"),
                    "`${{ x | y }}` is <b>*bad*</b> & [wrong]\nsecond line",
                )
                .with_help(Some("use \"z\"".to_owned()))],
            ),
            Rows::of(&b, &[Diagnostic::new(&INFO_RULE, span(&b, "tok"), "plain")]),
        ];
        insta::assert_snapshot!(summary_text(&files, SIZE_LIMIT));
    }

    #[test]
    fn zero_diagnostics_print_no_table() {
        let f = file("a.yaml", "");
        let text = summary_text(&[Rows::of(&f, &[])], SIZE_LIMIT);
        assert_eq!(
            text,
            "## workflint\n\n2 problems in 2 files: 1 error, 0 warnings, 1 info\n"
        );
    }

    #[test]
    fn rows_past_the_size_limit_are_counted_instead_of_listed() {
        let f = file("a.yaml", "t1 t2 t3\n");
        let diagnostics: Vec<Diagnostic> = ["t1", "t2", "t3"]
            .iter()
            .map(|token| Diagnostic::new(&ERROR_RULE, span(&f, token), *token))
            .collect();
        let files = [Rows::of(&f, &diagnostics)];
        let full = summary_text(&files, SIZE_LIMIT);
        let one_row_less = full.len() - 1;
        let text = summary_text(&files, one_row_less);
        assert!(text.len() <= one_row_less + 1024, "{text}");
        assert!(text.contains("| t2 |"), "{text}");
        assert!(!text.contains("| t3 |"), "{text}");
        assert!(
            text.ends_with(
                "\n1 more problem is not listed, because GitHub limits the summary of a step to \
                 1 MiB.\n"
            ),
            "{text}"
        );
    }
}
