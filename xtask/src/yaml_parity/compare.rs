//! Compares the two dumps of each file and reports the result.

use std::fmt::Write as _;

use similar::TextDiff;

/// How the two dumps of one file compare.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// The dumps are the same.
    Identical,
    /// Both parsers reject the file. Their error positions differ by design
    /// (`docs/differences.md#yaml-error-positions` and
    /// `#unknown-anchor-position`), so only the failure is compared.
    BothFailed,
    /// Anything else.
    Differing,
}

/// Compares the go-yaml dump of a file with the `workflint-yaml` dump.
pub(crate) fn compare(go_yaml: &str, workflint: &str) -> Verdict {
    if go_yaml == workflint {
        Verdict::Identical
    } else if is_error(go_yaml) && is_error(workflint) {
        Verdict::BothFailed
    } else {
        Verdict::Differing
    }
}

fn is_error(dump: &str) -> bool {
    dump.starts_with("error: ")
}

/// The results of a run.
#[derive(Debug, Default)]
pub(crate) struct Report {
    out: String,
    compared: usize,
    identical: usize,
    both_failed: usize,
    differing: usize,
}

impl Report {
    /// Adds the dumps of the file `path` and returns how they compare.
    pub(crate) fn add(&mut self, path: &str, go_yaml: &str, workflint: &str) -> Verdict {
        self.compared += 1;
        let verdict = compare(go_yaml, workflint);
        match verdict {
            Verdict::Identical => self.identical += 1,
            Verdict::BothFailed => {
                self.both_failed += 1;
                let _ = write!(
                    self.out,
                    "failed in both: {path}\n  go-yaml: {go_yaml}  workflint: {workflint}"
                );
            }
            Verdict::Differing => {
                self.differing += 1;
                let diff = TextDiff::from_lines(go_yaml, workflint);
                let _ = write!(
                    self.out,
                    "differs: {path}\n{}",
                    diff.unified_diff()
                        .context_radius(3)
                        .missing_newline_hint(false)
                        .header("go-yaml", "workflint")
                );
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
            "{}yaml-parity: compared {} {files}: {} identical, {} failed in both, {} differing\n",
            self.out, self.compared, self.identical, self.both_failed, self.differing
        )
    }
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
    fn two_errors_at_different_positions_failed_in_both() {
        assert_eq!(
            compare(
                "error: 1:0: yaml: line 1: did not find expected node content\n",
                "error: 2:1: could not parse as YAML: did not find expected node content\n",
            ),
            Verdict::BothFailed,
        );
    }

    #[test]
    fn an_error_on_one_side_only_differs() {
        assert_eq!(compare("error: 0:0: yaml: x\n", TREE), Verdict::Differing);
        assert_eq!(compare(TREE, "error: 1:1: x\n"), Verdict::Differing);
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
        report.add("ok/a.yaml", TREE, TREE);
        report.add("ok/b.yaml", TREE, &TREE.replace("1:5", "1:6"));
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
        report.add("err/x.yaml", "error: 0:0: yaml: bad\n", "error: 1:1: bad\n");
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
        report.add("a", TREE, TREE);
        assert!(
            report.render().contains("compared 1 file: "),
            "{}",
            report.render()
        );
    }

    #[test]
    fn summary_counts_each_verdict() {
        let mut report = Report::default();
        report.add("a", TREE, TREE);
        report.add("b", TREE, TREE);
        report.add("c", "error: x\n", "error: y\n");
        report.add("d", TREE, "empty\n");
        assert!(
            report.render().ends_with(
                "yaml-parity: compared 4 files: 2 identical, 1 failed in both, 1 differing\n"
            ),
            "{}",
            report.render()
        );
    }
}
