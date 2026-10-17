//! Compares the two dumps of each file and reports the result.

use std::fmt::Write as _;

use similar::TextDiff;

/// How the two dumps of one file compare.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// The dumps are the same.
    Identical,
    /// Both parsers reject the file with the same message, or with messages
    /// that [`allowed_messages`] allows. Their error positions differ by
    /// design (`docs/differences.md#yaml-error-positions` and
    /// `#unknown-anchor-position`), so only the messages are compared.
    BothFailed,
    /// The dumps differ, and an entry of `docs/differences.md` that the
    /// caller names for the file explains it.
    Allowed,
    /// Anything else.
    Differing,
}

/// Compares the go-yaml dump of a file with the `workflint-yaml` dump.
pub(crate) fn compare(go_yaml: &str, workflint: &str) -> Verdict {
    if go_yaml == workflint {
        return Verdict::Identical;
    }
    match (error_message(go_yaml), error_message(workflint)) {
        (Some(go_yaml), Some(workflint))
            if go_yaml == workflint || allowed_messages(go_yaml, workflint) =>
        {
            Verdict::BothFailed
        }
        _ => Verdict::Differing,
    }
}

/// The message of a dump that is an error, without its position and the
/// prefixes that differ between the two dumps: go-yaml's `yaml:` and
/// `line N:`, and workflint's `could not parse as YAML:`.
fn error_message(dump: &str) -> Option<&str> {
    let line = dump.strip_prefix("error: ")?.lines().next()?;
    let line = strip_position(line).unwrap_or(line);
    let line = line.strip_prefix("yaml: ").unwrap_or(line);
    let line = strip_line_number(line).unwrap_or(line);
    Some(
        line.strip_prefix("could not parse as YAML: ")
            .unwrap_or(line),
    )
}

/// Strips a position such as `12:3: ` from the start of `text`.
fn strip_position(text: &str) -> Option<&str> {
    let rest = strip_digits(text)?.strip_prefix(':')?;
    strip_digits(rest)?.strip_prefix(": ")
}

/// Strips a line number such as `line 12: ` from the start of `text`.
fn strip_line_number(text: &str) -> Option<&str> {
    strip_digits(text.strip_prefix("line ")?)?.strip_prefix(": ")
}

/// Strips one or more ASCII digits from the start of `text`.
fn strip_digits(text: &str) -> Option<&str> {
    let rest = text.trim_start_matches(|c: char| c.is_ascii_digit());
    (rest.len() < text.len()).then_some(rest)
}

/// The messages of the encoding errors that `workflint_yaml::decode`
/// reports.
const DECODE_PROBLEMS: [&str; 9] = [
    "invalid leading UTF-8 octet",
    "incomplete UTF-8 octet sequence",
    "invalid trailing UTF-8 octet",
    "invalid length of a UTF-8 sequence",
    "invalid Unicode character",
    "incomplete UTF-16 character",
    "unexpected low surrogate area",
    "incomplete UTF-16 surrogate pair",
    "expected low surrogate area",
];

/// Whether a documented difference explains why go-yaml failed with the
/// message `go_yaml` and workflint with the message `workflint`.
///
/// workflint decodes the whole file before it parses, so it reports an
/// encoding error where go-yaml reports a syntax error that comes before it
/// (`docs/differences.md#yaml-error-positions`).
fn allowed_messages(go_yaml: &str, workflint: &str) -> bool {
    DECODE_PROBLEMS.contains(&workflint) && !DECODE_PROBLEMS.contains(&go_yaml)
}

/// The results of a run.
#[derive(Debug, Default)]
pub(crate) struct Report {
    out: String,
    compared: usize,
    identical: usize,
    both_failed: usize,
    allowed: usize,
    differing: usize,
}

impl Report {
    /// Adds the dumps of the file `path` and returns how they compare.
    ///
    /// `allowance` names the anchor of the entry in `docs/differences.md`
    /// that explains why the dumps of this file differ, if the file has one.
    /// Dumps that differ are then [`Verdict::Allowed`].
    pub(crate) fn add(
        &mut self,
        path: &str,
        go_yaml: &str,
        workflint: &str,
        allowance: Option<&str>,
    ) -> Verdict {
        self.compared += 1;
        let verdict = match (compare(go_yaml, workflint), allowance) {
            (Verdict::Differing, Some(_)) => Verdict::Allowed,
            (verdict, _) => verdict,
        };
        match verdict {
            Verdict::Identical => self.identical += 1,
            Verdict::BothFailed => {
                self.both_failed += 1;
                let _ = write!(
                    self.out,
                    "failed in both: {path}\n  go-yaml: {go_yaml}  workflint: {workflint}"
                );
            }
            Verdict::Allowed => {
                self.allowed += 1;
                let anchor = allowance.unwrap_or_default();
                let _ = write!(
                    self.out,
                    "differs as allowed by docs/differences.md#{anchor}: {path}\n{}",
                    diff(go_yaml, workflint)
                );
            }
            Verdict::Differing => {
                self.differing += 1;
                let _ = write!(self.out, "differs: {path}\n{}", diff(go_yaml, workflint));
            }
        }
        verdict
    }

    /// The number of files whose dumps differ.
    pub(crate) fn differing(&self) -> usize {
        self.differing
    }

    /// A line for each file that failed in both, a diff for each file that
    /// differs, and the summary last.
    pub(crate) fn render(&self) -> String {
        let files = if self.compared == 1 { "file" } else { "files" };
        format!(
            "{}yaml-parity: compared {} {files}: {} identical, {} failed in both, {} allowed, {} differing\n",
            self.out, self.compared, self.identical, self.both_failed, self.allowed, self.differing
        )
    }
}

/// The unified diff from the go-yaml dump to the workflint dump.
fn diff(go_yaml: &str, workflint: &str) -> String {
    TextDiff::from_lines(go_yaml, workflint)
        .unified_diff()
        .context_radius(3)
        .missing_newline_hint(false)
        .header("go-yaml", "workflint")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TREE: &str = "mapping 1:1\n  scalar 1:1 \"on\"\n  scalar 1:5 \"push\"\n";

    #[test]
    fn equal_dumps_are_identical() {
        assert_eq!(compare(TREE, TREE), Verdict::Identical);
        assert_eq!(compare("empty\n", "empty\n"), Verdict::Identical);
    }

    #[test]
    fn two_errors_with_the_same_message_at_different_positions_failed_in_both() {
        assert_eq!(
            compare(
                "error: 1:0: yaml: line 1: did not find expected node content\n",
                "error: 2:1: could not parse as YAML: did not find expected node content\n",
            ),
            Verdict::BothFailed,
        );
        assert_eq!(
            compare(
                "error: 0:0: yaml: control characters are not allowed\n",
                "error: 1:4: could not parse as YAML: control characters are not allowed\n",
            ),
            Verdict::BothFailed,
        );
        assert_eq!(
            compare(
                "error: yaml: unknown anchor 'b' referenced\n",
                "error: 1:4: could not parse as YAML: unknown anchor 'b' referenced\n",
            ),
            Verdict::BothFailed,
        );
        assert_eq!(
            compare(
                "error: 0:0: yaml: invalid leading UTF-8 octet\n",
                "error: invalid leading UTF-8 octet\n",
            ),
            Verdict::BothFailed,
        );
    }

    #[test]
    fn two_errors_with_different_messages_differ() {
        assert_eq!(
            compare(
                "error: 0:5: yaml: did not find expected '-' indicator\n",
                "error: 1:7: could not parse as YAML: could not find expected ':'\n",
            ),
            Verdict::Differing,
        );
        assert_eq!(
            compare(
                "error: 0:0: yaml: invalid leading UTF-8 octet\n",
                "error: 1:1: could not parse as YAML: did not find expected node content\n",
            ),
            Verdict::Differing,
        );
    }

    // docs/differences.md#yaml-error-positions
    #[test]
    fn an_encoding_error_where_go_yaml_reports_a_syntax_error_failed_in_both() {
        assert_eq!(
            compare(
                "error: 2:0: yaml: line 2: could not find expected ':'\n",
                "error: invalid trailing UTF-8 octet\n",
            ),
            Verdict::BothFailed,
        );
    }

    #[test]
    fn a_message_that_only_looks_like_a_position_is_kept() {
        assert_eq!(error_message("error: 1:x: y\n"), Some("1:x: y"));
        assert_eq!(error_message("error: yaml: line x: y\n"), Some("line x: y"));
        assert_eq!(error_message("mapping 1:1\n"), None);
    }

    #[test]
    fn an_error_on_one_side_only_differs() {
        assert_eq!(compare("error: 0:0: yaml: x\n", TREE), Verdict::Differing);
        assert_eq!(compare(TREE, "error: 1:1: x\n"), Verdict::Differing);
    }

    #[test]
    fn a_different_style_or_tag_differs() {
        let quoted = "mapping 1:1\n  scalar 1:1 \"on\"\n  scalar 1:5 double \"push\"\n";
        assert_eq!(compare(TREE, quoted), Verdict::Differing);
        let tagged = "mapping 1:1\n  scalar 1:1 \"on\"\n  scalar 1:5 !!str \"push\"\n";
        assert_eq!(compare(TREE, tagged), Verdict::Differing);
    }

    #[test]
    fn a_moved_node_differs() {
        let moved = TREE.replace("1:5", "1:6");
        assert_eq!(compare(TREE, &moved), Verdict::Differing);
    }

    #[test]
    fn a_different_value_or_anchor_differs() {
        assert_eq!(
            compare(TREE, &TREE.replace("push", "pull")),
            Verdict::Differing
        );
        assert_eq!(
            compare(TREE, &TREE.replace("1:5", "1:5 &a")),
            Verdict::Differing
        );
    }

    #[test]
    fn report_shows_the_file_and_the_moved_node() {
        let mut report = Report::default();
        report.add("ok/a.yaml", TREE, TREE, None);
        report.add("ok/b.yaml", TREE, &TREE.replace("1:5", "1:6"), None);
        let out = report.render();
        assert_eq!(report.differing(), 1);
        assert!(out.contains("differs: ok/b.yaml\n"), "{out}");
        assert!(out.contains("--- go-yaml\n+++ workflint\n"), "{out}");
        assert!(
            out.contains("\n-  scalar 1:5 \"push\"\n+  scalar 1:6 \"push\"\n"),
            "{out}"
        );
        assert!(!out.contains("ok/a.yaml"), "{out}");
    }

    #[test]
    fn report_lists_files_that_failed_in_both_with_both_errors() {
        let mut report = Report::default();
        report.add(
            "err/x.yaml",
            "error: 0:0: yaml: bad\n",
            "error: 1:1: bad\n",
            None,
        );
        let out = report.render();
        assert_eq!(report.differing(), 0);
        assert!(
            out.contains(
                "failed in both: err/x.yaml\n  go-yaml: error: 0:0: yaml: bad\n  workflint: error: 1:1: bad\n"
            ),
            "{out}"
        );
    }

    #[test]
    fn summary_of_one_file_says_file() {
        let mut report = Report::default();
        report.add("a", TREE, TREE, None);
        assert!(
            report.render().contains("compared 1 file: "),
            "{}",
            report.render()
        );
    }

    #[test]
    fn summary_counts_each_verdict() {
        let mut report = Report::default();
        report.add("a", TREE, TREE, None);
        report.add("b", TREE, TREE, None);
        report.add("c", "error: x\n", "error: x\n", None);
        report.add("d", "error: 1:1: x\n", "error: 2:2: x\n", None);
        report.add("e", TREE, "empty\n", Some("anchor"));
        report.add("f", TREE, "empty\n", None);
        report.add("g", "error: x\n", "error: y\n", None);
        assert!(
            report.render().ends_with(
                "yaml-parity: compared 7 files: 3 identical, 1 failed in both, 1 allowed, 2 differing\n"
            ),
            "{}",
            report.render()
        );
    }

    #[test]
    fn report_names_the_entry_that_allows_a_difference() {
        let mut report = Report::default();
        let verdict = report.add("x.yaml", TREE, "empty\n", Some("some-anchor"));
        assert_eq!(verdict, Verdict::Allowed);
        assert_eq!(report.differing(), 0);
        let out = report.render();
        assert!(
            out.contains(
                "differs as allowed by docs/differences.md#some-anchor: x.yaml\n--- go-yaml\n"
            ),
            "{out}"
        );
    }

    #[test]
    fn an_allowance_does_not_change_identical_dumps() {
        let mut report = Report::default();
        assert_eq!(
            report.add("x.yaml", TREE, TREE, Some("some-anchor")),
            Verdict::Identical
        );
    }
}
