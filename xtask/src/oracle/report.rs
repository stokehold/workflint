//! The oracle report: a summary, then a diff for each file that differs.

use std::fmt::Write as _;

use similar::{Algorithm, ChangeTag, DiffOp, capture_diff_slices, group_diff_ops};

use super::compare::{File, Run};

/// The runs of one oracle invocation.
#[derive(Debug, Default)]
pub(crate) struct Report {
    /// A sentence about what the runs compared, shown under the title.
    pub(crate) description: String,
    pub(crate) runs: Vec<Run>,
}

/// The counts in the summary.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Totals {
    pub(crate) runs: usize,
    pub(crate) files: usize,
    pub(crate) identical: usize,
    pub(crate) differing: usize,
    pub(crate) applied: usize,
    /// Lines that only one side has, over every differing file.
    pub(crate) unexplained: usize,
    pub(crate) exit_codes: usize,
    pub(crate) problems: usize,
}

impl Totals {
    /// Whether every difference is covered by the allow-list.
    pub(crate) fn clean(&self) -> bool {
        self.unexplained == 0 && self.exit_codes == 0 && self.problems == 0
    }
}

impl Report {
    pub(crate) fn totals(&self) -> Totals {
        let mut totals = Totals {
            runs: self.runs.len(),
            ..Totals::default()
        };
        for run in &self.runs {
            totals.applied += run.applied;
            totals.problems += run.problems.len();
            totals.exit_codes += usize::from(run.expected_code != run.actual_code);
            for file in &run.files {
                totals.files += 1;
                if file.identical {
                    totals.identical += 1;
                } else {
                    totals.differing += 1;
                    totals.unexplained += changed_lines(file);
                }
            }
        }
        totals
    }

    /// The title, the description, and the table of counts.
    pub(crate) fn summary(&self) -> String {
        let totals = self.totals();
        let mut out = String::from("# Oracle report\n\n");
        if !self.description.is_empty() {
            let _ = writeln!(out, "{}\n", self.description);
        }
        out.push_str("| Result | Count |\n|---|---:|\n");
        for (label, count) in [
            ("Runs", totals.runs),
            ("Files compared", totals.files),
            ("Identical files", totals.identical),
            ("Differing files", totals.differing),
            ("Allow-list entries applied", totals.applied),
            ("Unexplained lines", totals.unexplained),
            ("Runs with another exit code", totals.exit_codes),
            ("Problems", totals.problems),
        ] {
            let _ = writeln!(out, "| {label} | {count} |");
        }
        out.push('\n');
        out.push_str(if totals.clean() {
            "The allow-list explains every difference.\n"
        } else {
            "The allow-list does not explain every difference.\n"
        });
        out
    }

    /// The summary, then a section for each run with a difference.
    pub(crate) fn render(&self) -> String {
        let mut out = self.summary();
        for run in &self.runs {
            if let Some(section) = section(run) {
                out.push('\n');
                out.push_str(&section);
            }
        }
        out
    }
}

/// The section of `run`, or `None` if it has no difference.
fn section(run: &Run) -> Option<String> {
    let mut blocks = Vec::new();
    if !run.problems.is_empty() {
        let mut list = String::new();
        for problem in &run.problems {
            let _ = writeln!(list, "- {problem}");
        }
        blocks.push(list);
    }
    if run.expected_code != run.actual_code {
        blocks.push(format!(
            "The reference {} and workflint {}.\n",
            exit(run.expected_code),
            exit(run.actual_code)
        ));
        for (tool, stderr) in ["The reference", "workflint"].iter().zip(&run.stderr) {
            let stderr = stderr.trim_end();
            if !stderr.is_empty() {
                blocks.push(format!("{tool}'s standard error:\n"));
                blocks.push(format!("```text\n{stderr}\n```\n"));
            }
        }
    }
    for file in run.files.iter().filter(|file| !file.identical) {
        blocks.push(match &file.path {
            Some(path) => format!("`{path}`:\n"),
            None => "Lines without a file path:\n".to_owned(),
        });
        blocks.push(format!("```diff\n{}```\n", diff(file)));
    }
    (!blocks.is_empty()).then(|| format!("## {}\n\n{}", run.name, blocks.join("\n")))
}

fn exit(code: Option<i32>) -> String {
    code.map_or_else(
        || "was stopped by a signal".to_owned(),
        |code| format!("exited with code {code}"),
    )
}

fn ops(file: &File) -> Vec<DiffOp> {
    capture_diff_slices(Algorithm::Myers, &file.expected, &file.actual)
}

/// The changed lines of `file`, with three lines of context around each
/// change.
fn diff(file: &File) -> String {
    let mut out = String::new();
    for group in group_diff_ops(ops(file), 3) {
        for op in group {
            for change in op.iter_changes(&file.expected, &file.actual) {
                let sign = match change.tag() {
                    ChangeTag::Equal => ' ',
                    ChangeTag::Delete => '-',
                    ChangeTag::Insert => '+',
                };
                let _ = writeln!(out, "{sign}{}", change.value());
            }
        }
    }
    out
}

/// The number of lines that only one side of `file` has.
fn changed_lines(file: &File) -> usize {
    ops(file)
        .iter()
        .flat_map(|op| op.iter_changes(&file.expected, &file.actual))
        .filter(|change| change.tag() != ChangeTag::Equal)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: Option<&str>, expected: &[&str], actual: &[&str]) -> File {
        File {
            path: path.map(str::to_owned),
            expected: expected.iter().map(|line| (*line).to_owned()).collect(),
            actual: actual.iter().map(|line| (*line).to_owned()).collect(),
            identical: expected == actual,
        }
    }

    fn run(name: &str, files: Vec<File>, codes: (i32, i32)) -> Run {
        Run {
            name: name.to_owned(),
            files,
            expected_code: Some(codes.0),
            actual_code: Some(codes.1),
            stderr: [String::new(), "workflint: oops\n".to_owned()],
            applied: 0,
            problems: Vec::new(),
        }
    }

    fn report() -> Report {
        Report {
            description: "Compared the test runs.".to_owned(),
            runs: vec![
                run(
                    "err/one_error",
                    vec![file(
                        Some("test.yaml"),
                        &[
                            "test.yaml:1:1: a [expression]",
                            "test.yaml:2:1: b [expression]",
                        ],
                        &[
                            "test.yaml:2:1: b [expression]",
                            "test.yaml:3:1: c [expression]",
                        ],
                    )],
                    (1, 1),
                ),
                run(
                    "ok/fine.yaml",
                    vec![
                        file(Some("test.yaml"), &[], &[]),
                        file(None, &[], &["Hello, world!"]),
                    ],
                    (0, 0),
                ),
                run(
                    "realworld",
                    vec![
                        file(
                            Some("a.yml"),
                            &["a.yml:1:1: a [action]"],
                            &["a.yml:1:1: a [action]"],
                        ),
                        file(Some("b.yml"), &[], &[]),
                    ],
                    (1, 0),
                ),
            ],
        }
    }

    #[test]
    fn totals_count_files_lines_and_exit_codes() {
        assert_eq!(
            report().totals(),
            Totals {
                runs: 3,
                files: 5,
                identical: 3,
                differing: 2,
                applied: 0,
                unexplained: 3,
                exit_codes: 1,
                problems: 0,
            }
        );
    }

    #[test]
    fn a_report_without_differences_is_clean() {
        let report = Report {
            description: String::new(),
            runs: vec![run(
                "ok/fine.yaml",
                vec![file(Some("test.yaml"), &[], &[])],
                (0, 0),
            )],
        };
        assert!(report.totals().clean());
        assert!(!self::report().totals().clean());
    }

    #[test]
    fn a_problem_makes_the_report_unclean() {
        let mut problem = run("err/x", vec![file(Some("test.yaml"), &[], &[])], (0, 0));
        problem
            .problems
            .push("rule ID \"x\" has no actionlint kind".to_owned());
        let report = Report {
            description: String::new(),
            runs: vec![problem],
        };
        assert_eq!(report.totals().problems, 1);
        assert!(!report.totals().clean());
    }

    #[test]
    fn the_summary_has_a_table_of_counts() {
        let summary = report().summary();
        assert!(
            summary.starts_with("# Oracle report\n\nCompared the test runs.\n"),
            "{summary}"
        );
        assert!(summary.contains("| Files compared | 5 |"), "{summary}");
        assert!(summary.contains("| Identical files | 3 |"), "{summary}");
        assert!(summary.contains("| Differing files | 2 |"), "{summary}");
        assert!(
            summary.contains("| Allow-list entries applied | 0 |"),
            "{summary}"
        );
        assert!(summary.contains("| Unexplained lines | 3 |"), "{summary}");
        assert!(
            summary.contains("| Runs with another exit code | 1 |"),
            "{summary}"
        );
        assert!(!summary.contains("```"), "{summary}");
    }

    #[test]
    fn the_report_has_a_diff_for_each_differing_file() {
        let rendered = report().render();
        assert!(rendered.starts_with(&report().summary()), "{rendered}");
        let section = "\
## err/one_error

`test.yaml`:

```diff
-test.yaml:1:1: a [expression]
 test.yaml:2:1: b [expression]
+test.yaml:3:1: c [expression]
```
";
        assert!(rendered.contains(section), "{rendered}");
        assert!(
            rendered.contains("Lines without a file path:\n\n```diff\n+Hello, world!\n```"),
            "{rendered}"
        );
        assert!(!rendered.contains("`b.yml`"), "{rendered}");
        assert!(!rendered.contains("`a.yml`"), "{rendered}");
    }

    #[test]
    fn the_report_shows_another_exit_code_with_standard_error() {
        let rendered = report().render();
        let section = "\
## realworld

The reference exited with code 1 and workflint exited with code 0.

workflint's standard error:

```text
workflint: oops
```
";
        assert!(rendered.contains(section), "{rendered}");
    }

    #[test]
    fn a_large_diff_keeps_three_lines_of_context() {
        fn refs(lines: &[String]) -> Vec<&str> {
            lines.iter().map(String::as_str).collect()
        }

        let expected: Vec<String> = (1..=10).map(|n| format!("t.yaml:{n}:1: m [x]")).collect();
        let mut actual = expected.clone();
        actual[9] = "t.yaml:10:1: other [x]".to_owned();
        let report = Report {
            description: String::new(),
            runs: vec![run(
                "err/x",
                vec![file(Some("t.yaml"), &refs(&expected), &refs(&actual))],
                (1, 1),
            )],
        };
        let rendered = report.render();
        assert!(!rendered.contains("t.yaml:6:1: m [x]"), "{rendered}");
        assert!(rendered.contains(" t.yaml:7:1: m [x]"), "{rendered}");
        assert!(
            rendered.contains("-t.yaml:10:1: m [x]\n+t.yaml:10:1: other [x]"),
            "{rendered}"
        );
        assert_eq!(report.totals().unexplained, 2);
    }

    #[test]
    fn problems_are_listed_in_their_run() {
        let mut problem = run("err/x", vec![file(Some("test.yaml"), &[], &[])], (0, 0));
        problem
            .problems
            .push("rule ID \"x\" has no actionlint kind".to_owned());
        let report = Report {
            description: String::new(),
            runs: vec![problem],
        };
        let rendered = report.render();
        assert!(
            rendered.contains("## err/x\n\n- rule ID \"x\" has no actionlint kind\n"),
            "{rendered}"
        );
    }
}
