//! The oracle report: a summary, then a diff for each file that differs.

use std::fmt::Write as _;

use similar::{Algorithm, ChangeTag, DiffOp, capture_diff_slices, group_diff_ops};
use xtask::conformance::matcher::{line_matches, pattern};

use super::compare::{File, Run};
use super::kinds::Kinds;

/// The runs of one oracle invocation.
#[derive(Debug, Default)]
pub(crate) struct Report {
    /// A sentence about what the runs compared, shown under the title.
    pub(crate) description: String,
    /// Whether the reference ran against itself.
    pub(crate) self_check: bool,
    /// The kinds that the runs compared.
    pub(crate) kinds: Kinds,
    /// The IDs of the fixtures on which the reference's output varies
    /// between runs, which no run compares.
    pub(crate) skipped: Vec<String>,
    /// The IDs of the fixtures whose `.out` file the reference fails, which
    /// no run compares.
    pub(crate) reference_failures: Vec<String>,
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
    /// Runs in which workflint printed to standard error.
    pub(crate) stderr: usize,
    pub(crate) problems: usize,
}

impl Totals {
    /// Whether every difference is covered by the allow-list.
    pub(crate) fn clean(&self) -> bool {
        self.unexplained == 0 && self.exit_codes == 0 && self.stderr == 0 && self.problems == 0
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
            totals.exit_codes += usize::from(self.codes_differ(run));
            totals.stderr += usize::from(workflint_stderr(run));
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
        if !self.kinds.is_all() {
            let names: Vec<String> = self
                .kinds
                .names()
                .iter()
                .map(|name| format!("`{name}`"))
                .collect();
            let (noun, list) = match names.split_last() {
                Some((last, [])) => ("kind", last.clone()),
                Some((last, rest)) => ("kinds", format!("{} and {last}", rest.join(", "))),
                None => ("kinds", String::new()),
            };
            let _ = writeln!(
                out,
                "Only the diagnostics of the {noun} {list} were compared. Exit codes 0 and 1 were \
                 not compared, because the lines of the other kinds decide them.\n"
            );
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
            (
                "Runs with workflint output on standard error",
                totals.stderr,
            ),
            ("Problems", totals.problems),
            (
                "Skipped fixtures",
                self.skipped.len() + self.reference_failures.len(),
            ),
        ] {
            let _ = writeln!(out, "| {label} | {count} |");
        }
        out.push('\n');
        out.push_str(match (self.self_check, totals.clean()) {
            (true, true) => "The two runs of the reference printed the same output.\n",
            (true, false) => "The two runs of the reference printed different output.\n",
            (false, true) => "The allow-list explains every difference.\n",
            (false, false) => "The allow-list does not explain every difference.\n",
        });
        if !self.skipped.is_empty() {
            out.push_str(
                "\nThese fixtures were skipped, because the reference's output on them varies \
                 between runs (`docs/differences.md#cached-metadata-errors`):\n\n",
            );
            for id in &self.skipped {
                let _ = writeln!(out, "- `{id}`");
            }
        }
        if !self.reference_failures.is_empty() {
            out.push_str(
                "\nThese fixtures were skipped, because the reference fails their .out file \
                 (`REFERENCE_FAILURES` in `xtask/src/conformance/runner.rs`), so the \
                 allow-list records of their lines cannot apply to its output:\n\n",
            );
            for id in &self.reference_failures {
                let _ = writeln!(out, "- `{id}`");
            }
        }
        out
    }

    /// The summary, then a section for each run with a difference.
    pub(crate) fn render(&self) -> String {
        let mut out = self.summary();
        for run in &self.runs {
            if let Some(section) = section(run, self.codes_differ(run)) {
                out.push('\n');
                out.push_str(&section);
            }
        }
        out
    }

    /// Whether the exit codes of `run` differ. With selected kinds, 0 and 1
    /// count as equal.
    fn codes_differ(&self, run: &Run) -> bool {
        let decided_by_lines = |code| matches!(code, Some(0 | 1));
        run.expected_code != run.actual_code
            && (self.kinds.is_all()
                || !decided_by_lines(run.expected_code)
                || !decided_by_lines(run.actual_code))
    }
}

/// The section of `run`, or `None` if it has no difference. `codes_differ`
/// says whether its exit codes differ.
fn section(run: &Run, codes_differ: bool) -> Option<String> {
    let mut blocks = Vec::new();
    if !run.problems.is_empty() {
        let mut list = String::new();
        for problem in &run.problems {
            let _ = writeln!(list, "- {problem}");
        }
        blocks.push(list);
    }
    if codes_differ {
        blocks.push(format!(
            "The reference {} and workflint {}.\n",
            exit(run.expected_code),
            exit(run.actual_code)
        ));
    }
    if codes_differ || run.stderr.iter().any(|stderr| !stderr.trim().is_empty()) {
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

/// Whether workflint printed to standard error in `run`. workflint prints
/// nothing there when it lints, so any output counts as a difference.
fn workflint_stderr(run: &Run) -> bool {
    !run.stderr[1].trim().is_empty()
}

fn exit(code: Option<i32>) -> String {
    code.map_or_else(
        || "was stopped by a signal".to_owned(),
        |code| format!("exited with code {code}"),
    )
}

/// The expected lines of `file`, with each `/pattern/` line that matches a
/// line of workflint replaced by that line, so that the diff counts the two
/// as equal. A pattern takes the first line it matches that no other pattern
/// took and that no exact expected line equals.
fn aligned(file: &File) -> Vec<String> {
    let mut taken = vec![false; file.actual.len()];
    file.expected
        .iter()
        .map(|want| {
            if pattern(want).is_none() {
                return want.clone();
            }
            let found = file.actual.iter().enumerate().position(|(index, have)| {
                !taken[index]
                    && !file.expected.contains(have)
                    && line_matches(want, have) == Ok(true)
            });
            match found {
                Some(index) => {
                    taken[index] = true;
                    file.actual[index].clone()
                }
                None => want.clone(),
            }
        })
        .collect()
}

fn ops(expected: &[String], file: &File) -> Vec<DiffOp> {
    capture_diff_slices(Algorithm::Myers, expected, &file.actual)
}

/// The changed lines of `file`, with three lines of context around each
/// change.
fn diff(file: &File) -> String {
    let expected = aligned(file);
    let mut out = String::new();
    for group in group_diff_ops(ops(&expected, file), 3) {
        for op in group {
            for change in op.iter_changes(&expected, &file.actual) {
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

/// The number of lines that only one side of `file` has. A line that a
/// `/pattern/` line matches is on both sides.
fn changed_lines(file: &File) -> usize {
    let expected = aligned(file);
    ops(&expected, file)
        .iter()
        .flat_map(|op| op.iter_changes(&expected, &file.actual))
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
            stderr: [String::new(), String::new()],
            applied: 0,
            problems: Vec::new(),
        }
    }

    fn report() -> Report {
        Report {
            description: "Compared the test runs.".to_owned(),
            self_check: false,
            kinds: Kinds::default(),
            skipped: Vec::new(),
            reference_failures: Vec::new(),
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
                Run {
                    stderr: [String::new(), "workflint: oops\n".to_owned()],
                    ..run(
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
                    )
                },
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
                stderr: 1,
                problems: 0,
            }
        );
    }

    #[test]
    fn a_report_without_differences_is_clean() {
        let report = Report {
            description: String::new(),
            self_check: false,
            kinds: Kinds::default(),
            skipped: Vec::new(),
            reference_failures: Vec::new(),
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
    fn workflint_output_on_standard_error_is_a_difference_with_equal_exit_codes() {
        let noisy = Run {
            stderr: [String::new(), "warning: x\n".to_owned()],
            ..run(
                "ok/fine.yaml",
                vec![file(Some("test.yaml"), &[], &[])],
                (0, 0),
            )
        };
        let report = Report {
            runs: vec![noisy],
            ..Report::default()
        };
        assert_eq!(report.totals().stderr, 1);
        assert!(!report.totals().clean());
        let rendered = report.render();
        assert!(
            rendered.contains(
                "## ok/fine.yaml\n\nworkflint's standard error:\n\n```text\nwarning: x\n```\n"
            ),
            "{rendered}"
        );
        assert!(
            rendered.contains("| Runs with workflint output on standard error | 1 |"),
            "{rendered}"
        );
    }

    #[test]
    fn reference_output_on_standard_error_is_shown_with_its_problem() {
        let mut noisy = run("err/x", vec![file(Some("test.yaml"), &[], &[])], (3, 3));
        noisy.stderr[0] = "could not read test.yaml\n".to_owned();
        noisy
            .problems
            .push("the reference exited with code 3; only 0 and 1 are normal".to_owned());
        let report = Report {
            runs: vec![noisy],
            ..Report::default()
        };
        assert!(!report.totals().clean());
        let rendered = report.render();
        assert!(
            rendered.contains(
                "The reference's standard error:\n\n```text\ncould not read test.yaml\n```"
            ),
            "{rendered}"
        );
    }

    #[test]
    fn a_problem_makes_the_report_unclean() {
        let mut problem = run("err/x", vec![file(Some("test.yaml"), &[], &[])], (0, 0));
        problem
            .problems
            .push("rule ID \"x\" has no actionlint kind".to_owned());
        let report = Report {
            description: String::new(),
            self_check: false,
            kinds: Kinds::default(),
            skipped: Vec::new(),
            reference_failures: Vec::new(),
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
            self_check: false,
            kinds: Kinds::default(),
            skipped: Vec::new(),
            reference_failures: Vec::new(),
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
    fn a_line_that_matched_a_pattern_is_not_unexplained() {
        let mut differing = file(
            Some("test.yaml"),
            &[r"/test\.yaml:1:1: .+ \[x\]/", "test.yaml:2:1: b [x]"],
            &["test.yaml:1:1: anything [x]", "test.yaml:3:1: c [x]"],
        );
        differing.identical = false;
        let report = Report {
            runs: vec![run("err/x", vec![differing], (1, 1))],
            ..Report::default()
        };
        assert_eq!(report.totals().unexplained, 2);
        let rendered = report.render();
        assert!(
            rendered.contains(
                "```diff\n test.yaml:1:1: anything [x]\n-test.yaml:2:1: b [x]\n+test.yaml:3:1: c [x]\n```"
            ),
            "{rendered}"
        );
    }

    #[test]
    fn a_pattern_that_matches_nothing_is_unexplained() {
        let differing = file(
            Some("test.yaml"),
            &[r"/test\.yaml:1:1: z/"],
            &["test.yaml:1:1: a [x]"],
        );
        let report = Report {
            runs: vec![run("err/x", vec![differing], (1, 1))],
            ..Report::default()
        };
        assert_eq!(report.totals().unexplained, 2);
    }

    #[test]
    fn the_summary_lists_the_skipped_fixtures() {
        let report = Report {
            skipped: vec!["projects/broken_local_action".to_owned()],
            reference_failures: vec!["projects/local_action_invalid".to_owned()],
            ..Report::default()
        };
        let summary = report.summary();
        assert!(summary.contains("| Skipped fixtures | 2 |"), "{summary}");
        assert!(
            summary.contains("cannot apply to its output:\n\n- `projects/local_action_invalid`\n"),
            "{summary}"
        );
        assert!(
            summary.contains("cached-metadata-errors`):\n\n- `projects/broken_local_action`\n"),
            "{summary}"
        );
    }

    #[test]
    fn the_self_check_summary_does_not_mention_the_allow_list() {
        let mut report = self::report();
        report.self_check = true;
        let summary = report.summary();
        assert!(
            summary.contains("The two runs of the reference printed different output.\n"),
            "{summary}"
        );
        assert!(!summary.contains("allow-list"), "{summary}");
        let clean = Report {
            self_check: true,
            ..Report::default()
        };
        assert!(
            clean
                .summary()
                .contains("The two runs of the reference printed the same output.\n")
        );
    }

    #[test]
    fn problems_are_listed_in_their_run() {
        let mut problem = run("err/x", vec![file(Some("test.yaml"), &[], &[])], (0, 0));
        problem
            .problems
            .push("rule ID \"x\" has no actionlint kind".to_owned());
        let report = Report {
            description: String::new(),
            self_check: false,
            kinds: Kinds::default(),
            skipped: Vec::new(),
            reference_failures: Vec::new(),
            runs: vec![problem],
        };
        let rendered = report.render();
        assert!(
            rendered.contains("## err/x\n\n- rule ID \"x\" has no actionlint kind\n"),
            "{rendered}"
        );
    }

    fn only(names: &[&str]) -> Kinds {
        let names: Vec<String> = names.iter().map(|name| (*name).to_owned()).collect();
        Kinds::new(&names).unwrap()
    }

    #[test]
    fn the_summary_names_the_kinds_and_that_exit_codes_zero_and_one_are_not_compared() {
        let report = Report {
            kinds: only(&["syntax-check", "expression"]),
            ..Report::default()
        };
        let summary = report.summary();
        assert!(
            summary.contains(
                "Only the diagnostics of the kinds `expression` and `syntax-check` were compared."
            ),
            "{summary}"
        );
        assert!(
            summary.contains("Exit codes 0 and 1 were not compared"),
            "{summary}"
        );
        let one = Report {
            kinds: only(&["syntax-check"]),
            ..Report::default()
        };
        assert!(
            one.summary()
                .contains("Only the diagnostics of the kind `syntax-check` were compared.")
        );
        assert!(!self::report().summary().contains("Only the diagnostics"));
    }

    #[test]
    fn with_kinds_exit_codes_zero_and_one_count_as_equal() {
        let mut report = self::report();
        report.kinds = only(&["action"]);
        assert_eq!(report.totals().exit_codes, 0);
        assert!(!report.render().contains("exited with code"));

        report.runs[2].actual_code = Some(3);
        assert_eq!(report.totals().exit_codes, 1);
        assert!(
            report
                .render()
                .contains("The reference exited with code 1 and workflint exited with code 3.")
        );
        report.runs[2].actual_code = None;
        assert_eq!(report.totals().exit_codes, 1);
    }
}
