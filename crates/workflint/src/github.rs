//! The output of `-format github`: one GitHub Actions workflow command per diagnostic, which the
//! runner turns into an annotation (ADR 0015).
//!
//! [`Annotations::of`] builds the commands of one file, and [`write()`] prints the commands of
//! every file, errors first, then warnings, then info. GitHub shows at most
//! [`LIMIT_PER_TYPE`] annotations of each type per step and drops the rest, so when a type has
//! more, a notice says how many problems have no annotation. The notice comes before the info
//! diagnostics, because it is a notice too and would otherwise be one that GitHub drops.
//! `docs/github-output.md` describes the output.

use std::cmp::Reverse;
use std::io::{self, Write};

use workflint_diagnostics::{Diagnostic, Severity, SourceFile};

/// The number of annotations of each type, error, warning, and notice, that GitHub shows for
/// one step.
pub(crate) const LIMIT_PER_TYPE: usize = 10;

/// The workflow commands of one file.
#[derive(Debug, Clone)]
pub(crate) struct Annotations(Vec<Annotation>);

/// One workflow command and the severity of its diagnostic.
#[derive(Debug, Clone)]
struct Annotation {
    severity: Severity,
    command: String,
}

impl Annotations {
    /// The commands of `diagnostics`, which are in `file`, in the order of `diagnostics`.
    #[must_use]
    pub(crate) fn of(file: &SourceFile, diagnostics: &[Diagnostic]) -> Self {
        let path = escape_property(file.path());
        let annotations = diagnostics
            .iter()
            .map(|diagnostic| {
                let start = file.line_col(diagnostic.span.start);
                let end = file.line_col(diagnostic.span.end);
                // GitHub takes columns only for a range on one line.
                let columns = if start.line == end.line {
                    format!(",col={},endColumn={}", start.column, end.column)
                } else {
                    String::new()
                };
                let message = match &diagnostic.help {
                    Some(help) => format!("{}\n\nhelp: {help}", diagnostic.message),
                    None => diagnostic.message.clone(),
                };
                Annotation {
                    severity: diagnostic.severity,
                    command: format!(
                        "::{} file={path},line={},endLine={}{columns},title={}::{}",
                        command_name(diagnostic.severity),
                        start.line,
                        end.line,
                        escape_property(diagnostic.rule.id),
                        escape_data(&message),
                    ),
                }
            })
            .collect();
        Self(annotations)
    }
}

/// Writes the commands of `files`, the most severe first and in the order of `files` within a
/// severity, one per line. When GitHub would drop some of them, a notice before the info
/// diagnostics says how many.
///
/// # Errors
///
/// Returns the error of writing to `out`.
pub(crate) fn write<W: Write + ?Sized>(out: &mut W, files: &[Annotations]) -> io::Result<()> {
    let mut annotations: Vec<&Annotation> = files.iter().flat_map(|file| &file.0).collect();
    annotations.sort_by_key(|annotation| Reverse(annotation.severity));
    let count = |severity: Severity| {
        annotations
            .iter()
            .filter(|annotation| annotation.severity == severity)
            .count()
    };
    let (errors, warnings, info) = (
        count(Severity::Error),
        count(Severity::Warning),
        count(Severity::Info),
    );
    let notice = (errors.max(warnings).max(info) > LIMIT_PER_TYPE).then(|| {
        // The notice takes the place of one info diagnostic.
        let hidden = errors.saturating_sub(LIMIT_PER_TYPE)
            + warnings.saturating_sub(LIMIT_PER_TYPE)
            + info.saturating_sub(LIMIT_PER_TYPE - 1);
        let total = annotations.len();
        format!(
            "::notice title=workflint::{hidden} of {total} problems have no annotation, because \
             GitHub shows at most {LIMIT_PER_TYPE} annotations of each type per step. The job \
             summary and the log of this step list every problem."
        )
    });
    let mut notice = notice.as_deref();
    for annotation in annotations {
        if annotation.severity == Severity::Info
            && let Some(line) = notice.take()
        {
            writeln!(out, "{line}")?;
        }
        writeln!(out, "{}", annotation.command)?;
    }
    if let Some(line) = notice {
        writeln!(out, "{line}")?;
    }
    Ok(())
}

/// The workflow command for a diagnostic of `severity`.
const fn command_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "notice",
    }
}

/// Escapes the message of a command as GitHub's toolkit does: `%`, CR, and LF.
fn escape_data(text: &str) -> String {
    text.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

/// Escapes the value of a command property as GitHub's toolkit does: as a message, and `:` and
/// `,` too.
fn escape_property(text: &str) -> String {
    escape_data(text).replace(':', "%3A").replace(',', "%2C")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::fmt::Write as _;

    use workflint_diagnostics::{Category, FileId, RuleMeta, Span};

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
    static WARNING_RULE: RuleMeta = rule("warning-rule", Severity::Warning);
    static INFO_RULE: RuleMeta = rule("info-rule", Severity::Info);

    fn file(path: &str, text: &str) -> SourceFile {
        SourceFile::new(FileId::new(0), path, text).unwrap()
    }

    fn span(file: &SourceFile, token: &str) -> Span {
        let start = u32::try_from(file.text().find(token).unwrap()).unwrap();
        let end = start + u32::try_from(token.len()).unwrap();
        Span::new(file.id(), start, end)
    }

    fn github_text(files: &[Annotations]) -> String {
        let mut out = Vec::new();
        write(&mut out, files).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn percent_cr_lf_colon_and_comma_are_escaped() {
        let f = file("dir/a:b,c%.yaml", "x: tok\n");
        let d = Diagnostic::new(
            &ERROR_RULE,
            span(&f, "tok"),
            "100% wrong: a, b\r\nnext line",
        )
        .with_help(Some("did you mean \"x:y\"?".to_owned()));
        insta::assert_snapshot!(github_text(&[Annotations::of(&f, &[d])]));
    }

    #[test]
    fn a_span_over_several_lines_has_no_columns() {
        let f = file("a.yaml", "key:\n  value\n");
        let d = Diagnostic::new(&WARNING_RULE, Span::new(f.id(), 0, 12), "spans lines");
        assert_eq!(
            github_text(&[Annotations::of(&f, &[d])]),
            "::warning file=a.yaml,line=1,endLine=2,title=warning-rule::spans lines\n"
        );
    }

    #[test]
    fn columns_count_code_points_as_the_default_format_does() {
        let f = file("a.yaml", "x: 日本 tok\n");
        let d = Diagnostic::new(&INFO_RULE, span(&f, "tok"), "m");
        assert_eq!(
            github_text(&[Annotations::of(&f, &[d])]),
            "::notice file=a.yaml,line=1,endLine=1,col=7,endColumn=10,title=info-rule::m\n"
        );
    }

    #[test]
    fn the_most_severe_come_first_in_the_order_of_the_files() {
        let a = file("a.yaml", "i1 w1 e1\n");
        let b = file("b.yaml", "e2 i2 w2\n");
        let of = |f: &SourceFile, tokens: [(&'static RuleMeta, &str); 3]| {
            let diagnostics: Vec<Diagnostic> = tokens
                .iter()
                .map(|(rule, token)| Diagnostic::new(rule, span(f, token), *token))
                .collect();
            Annotations::of(f, &diagnostics)
        };
        let files = [
            of(
                &a,
                [
                    (&INFO_RULE, "i1"),
                    (&WARNING_RULE, "w1"),
                    (&ERROR_RULE, "e1"),
                ],
            ),
            of(
                &b,
                [
                    (&ERROR_RULE, "e2"),
                    (&INFO_RULE, "i2"),
                    (&WARNING_RULE, "w2"),
                ],
            ),
        ];
        insta::assert_snapshot!(github_text(&files));
    }

    /// A file with `count` diagnostics of `rule`, each on its own line.
    fn many(rule: &'static RuleMeta, count: usize) -> Annotations {
        let text = (0..count).fold(String::new(), |mut text, n| {
            let _ = writeln!(text, "t{n:02}");
            text
        });
        let f = file("a.yaml", &text);
        let diagnostics: Vec<Diagnostic> = (0..count)
            .map(|n| {
                let token = format!("t{n:02}");
                Diagnostic::new(rule, span(&f, &token), token)
            })
            .collect();
        Annotations::of(&f, &diagnostics)
    }

    #[test]
    fn more_than_ten_of_a_type_end_the_errors_and_warnings_with_a_notice() {
        insta::assert_snapshot!(github_text(&[
            many(&ERROR_RULE, 12),
            many(&WARNING_RULE, 3),
            many(&INFO_RULE, 2),
        ]));
    }

    #[test]
    fn the_notice_is_among_the_ten_notices_github_shows() {
        let text = github_text(&[many(&WARNING_RULE, 1), many(&INFO_RULE, 11)]);
        let notices: Vec<&str> = text
            .lines()
            .filter(|line| line.starts_with("::notice "))
            .collect();
        assert_eq!(notices.len(), 12);
        assert!(notices[0].starts_with("::notice title=workflint::2 of 12 problems "));
        assert!(text.starts_with("::warning "));
    }

    #[test]
    fn ten_of_each_type_need_no_notice() {
        let text = github_text(&[
            many(&ERROR_RULE, LIMIT_PER_TYPE),
            many(&WARNING_RULE, LIMIT_PER_TYPE),
            many(&INFO_RULE, LIMIT_PER_TYPE),
        ]);
        assert_eq!(text.lines().count(), 3 * LIMIT_PER_TYPE);
        assert!(!text.contains("title=workflint"));
    }

    #[test]
    fn only_errors_over_the_limit_put_the_notice_last() {
        let text = github_text(&[many(&ERROR_RULE, 11)]);
        let last = text.lines().last().unwrap();
        assert!(
            last.starts_with("::notice title=workflint::1 of 11 problems "),
            "{last}"
        );
    }

    #[test]
    fn zero_diagnostics_print_nothing() {
        assert_eq!(
            github_text(&[Annotations::of(&file("a.yaml", ""), &[])]),
            ""
        );
    }
}
