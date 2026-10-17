use crate::{LineCol, RuleMeta, Severity, SourceFile, Span};

/// One finding: a rule, a severity, a message, and where in the source it applies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// The rule that produced the diagnostic.
    pub rule: &'static RuleMeta,
    /// How serious the diagnostic is.
    pub severity: Severity,
    /// The message, byte-identical to actionlint's wherever actionlint is correct.
    pub message: String,
    /// The primary location. It covers the whole token the message is about.
    pub span: Span,
    /// Secondary locations, each with a short note.
    pub labels: Vec<Label>,
    /// Edits that fix the problem, if the rule knows them.
    pub fix: Option<Fix>,
}

impl Diagnostic {
    /// Creates a diagnostic with the rule's default severity, no labels, and no fix.
    #[must_use]
    pub fn new(rule: &'static RuleMeta, span: Span, message: impl Into<String>) -> Self {
        Self {
            rule,
            severity: rule.default_severity,
            message: message.into(),
            span,
            labels: Vec::new(),
            fix: None,
        }
    }
}

/// A secondary location of a diagnostic, such as "anchor defined here".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    /// The location.
    pub span: Span,
    /// A short note about the location.
    pub note: String,
}

/// A set of edits that fixes the problem a diagnostic reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    /// The edits, which must not overlap.
    pub edits: Vec<TextEdit>,
    /// Whether the edits are safe to apply without review.
    pub applicability: Applicability,
}

/// Replaces a byte range of the diagnostic's file with new text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    /// The offset of the first byte to replace.
    pub start: u32,
    /// The offset one past the last byte to replace.
    pub end: u32,
    /// The text to insert in place of the range.
    pub replacement: String,
}

/// Whether a [`Fix`] keeps the meaning of the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Applicability {
    /// The fix keeps the meaning of the file and can be applied without review.
    Safe,
    /// The fix can change behaviour, so a person must review it.
    Unsafe,
}

/// Sorts the diagnostics of one file and removes duplicates.
///
/// The order is line, column, rule ID, then message. actionlint sorts by message after the
/// column, so the order of diagnostics at the same position differs from actionlint
/// (`docs/differences.md#sort-order`). Diagnostics with the same line, column, and message
/// collapse into one even if their rule IDs differ, as in actionlint. The one kept has the
/// highest severity and, among those, the smallest rule ID. When those are equal too, the first
/// in `diagnostics` is kept.
///
/// Every diagnostic's span must be in `file`.
pub fn sort_and_dedup(file: &SourceFile, diagnostics: &mut Vec<Diagnostic>) {
    let mut keyed: Vec<(LineCol, Diagnostic)> = diagnostics
        .drain(..)
        .map(|d| (file.line_col(d.span.start), d))
        .collect();

    // Group the duplicates, with the one to keep first in each group. The sort is stable, so
    // the input order breaks the remaining ties.
    keyed.sort_by(|(lhs_pos, lhs), (rhs_pos, rhs)| {
        lhs_pos
            .cmp(rhs_pos)
            .then_with(|| lhs.message.cmp(&rhs.message))
            .then_with(|| rhs.severity.cmp(&lhs.severity))
            .then_with(|| lhs.rule.id.cmp(rhs.rule.id))
    });
    keyed.dedup_by(|(pos, d), (kept_pos, kept)| pos == kept_pos && d.message == kept.message);

    keyed.sort_by(|(lhs_pos, lhs), (rhs_pos, rhs)| {
        lhs_pos
            .cmp(rhs_pos)
            .then_with(|| lhs.rule.id.cmp(rhs.rule.id))
            .then_with(|| lhs.message.cmp(&rhs.message))
    });
    diagnostics.extend(keyed.into_iter().map(|(_, d)| d));
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::{Category, FileId};

    const WIDTH: usize = 40;

    static RULE_A: RuleMeta = rule("a-rule");
    static RULE_B: RuleMeta = rule("b-rule");
    static RULE_C: RuleMeta = rule("c-rule");

    const fn rule(id: &'static str) -> RuleMeta {
        RuleMeta {
            id,
            description: "",
            kind: "syntax-check",
            category: Category::Correctness,
            default_severity: Severity::Error,
            docs_url: "https://example.com",
        }
    }

    /// A file of 20 lines with `WIDTH` ASCII characters each.
    fn file() -> SourceFile {
        let line = format!("{}\n", "x".repeat(WIDTH));
        SourceFile::new(FileId::new(0), "test.yaml", line.repeat(20)).unwrap()
    }

    fn at(rule: &'static RuleMeta, line: usize, column: usize, message: &str) -> Diagnostic {
        let offset = u32::try_from((line - 1) * (WIDTH + 1) + (column - 1)).unwrap();
        Diagnostic::new(rule, Span::new(FileId::new(0), offset, offset + 1), message)
    }

    fn positions(file: &SourceFile, diagnostics: &[Diagnostic]) -> Vec<(usize, usize)> {
        diagnostics
            .iter()
            .map(|d| {
                let pos = file.line_col(d.span.start);
                (pos.line, pos.column)
            })
            .collect()
    }

    fn summary(diagnostics: &[Diagnostic]) -> Vec<(&'static str, &str)> {
        diagnostics
            .iter()
            .map(|d| (d.rule.id, d.message.as_str()))
            .collect()
    }

    #[test]
    fn new_diagnostic_takes_the_rule_default_severity() {
        static INFO_RULE: RuleMeta = RuleMeta {
            default_severity: Severity::Info,
            ..rule("info-rule")
        };
        let d = Diagnostic::new(&INFO_RULE, Span::new(FileId::new(0), 0, 1), "msg");
        assert_eq!(d.severity, Severity::Info);
        assert_eq!(d.labels, []);
        assert_eq!(d.fix, None);
    }

    // Ported from `TestErrorSortErrorsByPosition` in the reference's `error_test.go`. The
    // reference's messages are all empty, so its last case is also a deduplication case.
    #[test]
    fn sorts_by_line_then_column() {
        type Positions = &'static [(usize, usize)];
        let file = file();
        let cases: [(Positions, Positions); 4] = [
            (&[], &[]),
            (&[(1, 2)], &[(1, 2)]),
            (
                &[(1, 2), (4, 1), (3, 20), (1, 1)],
                &[(1, 1), (1, 2), (3, 20), (4, 1)],
            ),
            (&[(1, 1), (1, 1), (1, 1)], &[(1, 1)]),
        ];
        for (input, want) in cases {
            let mut diagnostics: Vec<_> = input
                .iter()
                .map(|&(line, column)| at(&RULE_A, line, column, ""))
                .collect();
            sort_and_dedup(&file, &mut diagnostics);
            assert_eq!(positions(&file, &diagnostics), want, "input {input:?}");
        }
    }

    // Ported from `TestErrorSortErrorsByMessage` in the reference's `error_test.go`.
    #[test]
    fn sorts_by_message_at_the_same_position_and_rule() {
        let file = file();
        let mut messages: Vec<String> = ('a'..='z')
            .rev()
            .chain(('A'..='Z').rev())
            .map(|c| format!("Message {c}"))
            .collect();
        let mut diagnostics: Vec<_> = messages.iter().map(|m| at(&RULE_A, 1, 1, m)).collect();

        sort_and_dedup(&file, &mut diagnostics);

        messages.sort();
        let have: Vec<_> = diagnostics.iter().map(|d| d.message.clone()).collect();
        assert_eq!(have, messages);
    }

    #[test]
    fn rule_id_orders_diagnostics_at_the_same_position_before_the_message() {
        let file = file();
        let mut diagnostics = vec![
            at(&RULE_B, 2, 3, "a message"),
            at(&RULE_A, 2, 3, "b message"),
            at(&RULE_A, 1, 9, "z message"),
        ];
        sort_and_dedup(&file, &mut diagnostics);
        assert_eq!(
            summary(&diagnostics),
            [
                ("a-rule", "z message"),
                ("a-rule", "b message"),
                ("b-rule", "a message"),
            ]
        );
    }

    #[test]
    fn same_position_and_message_with_different_rule_ids_collapse_into_one() {
        let file = file();
        let mut diagnostics = vec![
            at(&RULE_B, 5, 7, "duplicate"),
            at(&RULE_A, 5, 7, "duplicate"),
        ];
        sort_and_dedup(&file, &mut diagnostics);
        assert_eq!(summary(&diagnostics), [("a-rule", "duplicate")]);
    }

    #[test]
    fn duplicates_separated_by_another_rule_at_the_same_position_collapse() {
        let file = file();
        let mut diagnostics = vec![
            at(&RULE_C, 5, 7, "duplicate"),
            at(&RULE_B, 5, 7, "other"),
            at(&RULE_A, 5, 7, "duplicate"),
        ];
        sort_and_dedup(&file, &mut diagnostics);
        assert_eq!(
            summary(&diagnostics),
            [("a-rule", "duplicate"), ("b-rule", "other")]
        );
    }

    #[test]
    fn same_message_at_different_positions_is_kept() {
        let file = file();
        let mut diagnostics = vec![at(&RULE_A, 5, 7, "same"), at(&RULE_A, 5, 8, "same")];
        sort_and_dedup(&file, &mut diagnostics);
        assert_eq!(positions(&file, &diagnostics), [(5, 7), (5, 8)]);
    }

    #[test]
    fn collapsed_duplicates_keep_the_highest_severity_then_the_smallest_rule_id() {
        static WARNING_A: RuleMeta = RuleMeta {
            default_severity: Severity::Warning,
            ..rule("a-rule")
        };
        let file = file();
        let mut diagnostics = vec![
            at(&WARNING_A, 5, 7, "duplicate"),
            at(&RULE_C, 5, 7, "duplicate"),
            at(&RULE_B, 5, 7, "duplicate"),
        ];
        sort_and_dedup(&file, &mut diagnostics);
        assert_eq!(summary(&diagnostics), [("b-rule", "duplicate")]);
        assert_eq!(
            diagnostics.first().map(|d| d.severity),
            Some(Severity::Error)
        );
    }

    #[test]
    fn collapsing_compares_the_diagnostic_severity_not_the_rule_default() {
        let file = file();
        let mut info = at(&RULE_A, 5, 7, "duplicate");
        info.severity = Severity::Info;
        let mut warning = at(&RULE_B, 5, 7, "duplicate");
        warning.severity = Severity::Warning;
        let mut diagnostics = vec![info, warning];
        sort_and_dedup(&file, &mut diagnostics);
        assert_eq!(summary(&diagnostics), [("b-rule", "duplicate")]);
    }

    // The reference's `err/dedup_errors` fixture reports one error for a step that an alias
    // repeats eleven times.
    #[test]
    fn diagnostics_repeated_through_aliases_collapse_into_one() {
        let file = file();
        let mut diagnostics: Vec<_> = (0..11)
            .map(|_| at(&RULE_A, 9, 9, "unexpected key \"with\""))
            .collect();
        sort_and_dedup(&file, &mut diagnostics);
        assert_eq!(diagnostics.len(), 1);
    }
}
