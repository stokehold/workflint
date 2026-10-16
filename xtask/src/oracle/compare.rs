//! Compares what the reference and workflint printed for one run.

use std::collections::{BTreeMap, BTreeSet};

use xtask::conformance::allow_list::{self, Entry};
use xtask::conformance::matcher::line_matches;
use xtask::conformance::output::{Diagnostic, parse_line};
use xtask::conformance::rule_kinds::actionlint_kind;

/// What one binary did in one run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Output {
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    /// `None` when a signal ended the process.
    pub(crate) code: Option<i32>,
}

/// The lines of one file in a run, from both binaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct File {
    /// The path the binaries printed, or `None` for lines without a
    /// position.
    pub(crate) path: Option<String>,
    /// The reference's lines, after the allow-list.
    pub(crate) expected: Vec<String>,
    /// workflint's lines, with kinds in brackets.
    pub(crate) actual: Vec<String>,
    pub(crate) identical: bool,
}

/// How one run compares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Run {
    /// The fixture ID, or `realworld` for the corpus.
    pub(crate) name: String,
    /// The input files first, in argument order, then the other paths in
    /// the output, sorted, then the lines without a position.
    pub(crate) files: Vec<File>,
    /// The exit code the reference's output calls for, after the allow-list.
    pub(crate) expected_code: Option<i32>,
    pub(crate) actual_code: Option<i32>,
    /// Standard error of the reference and of workflint.
    pub(crate) stderr: [String; 2],
    /// The number of allow-list entries that changed the expected lines.
    pub(crate) applied: usize,
    /// Allow-list entries that do not apply and rule IDs without a kind.
    pub(crate) problems: Vec<String>,
}

/// Compares the outputs of one run, named `name`, that linted `inputs`.
///
/// The reference's lines stay in the order it printed them, and `entries`
/// change them as in the conformance runner. workflint's lines are sorted as
/// the reference sorts errors and, with `map_kinds`, get the actionlint kind
/// of their rule ID in brackets. Then both are grouped by file.
pub(crate) fn compare(
    name: &str,
    inputs: &[String],
    reference: &Output,
    workflint: &Output,
    entries: &[&Entry],
    map_kinds: bool,
) -> Run {
    let mut problems = BTreeSet::new();

    let printed: Vec<String> = reference.stdout.lines().map(normalize).collect();
    let (expected, applied) = match allow_list::apply(&printed, entries) {
        Ok(lines) => (lines, entries.len()),
        Err(problem) => {
            problems.insert(problem);
            (printed, 0)
        }
    };

    let mut diagnostics = Vec::new();
    let mut unparsed = Vec::new();
    for line in workflint.stdout.lines() {
        match parse_line(line) {
            Ok(diagnostic) => diagnostics.push(diagnostic),
            Err(_) => unparsed.push(line.to_owned()),
        }
    }
    diagnostics.sort_by(Diagnostic::reference_order);
    if map_kinds {
        for diagnostic in &mut diagnostics {
            match actionlint_kind(&diagnostic.code) {
                Some(kind) => kind.clone_into(&mut diagnostic.code),
                None => {
                    problems.insert(format!(
                        "rule ID {:?} has no actionlint kind in tests/conformance/rule-kinds.toml",
                        diagnostic.code
                    ));
                }
            }
        }
    }
    let actual: Vec<String> = diagnostics
        .iter()
        .map(ToString::to_string)
        .chain(unparsed)
        .collect();

    let expected_code = match reference.code {
        Some(0 | 1) => Some(i32::from(!expected.is_empty())),
        other => other,
    };
    let mut expected = group(expected);
    let mut actual = group(actual);
    let others: BTreeSet<&String> = expected
        .keys()
        .chain(actual.keys())
        .flatten()
        .filter(|path| !inputs.contains(path))
        .collect();
    let mut paths: Vec<Option<String>> = inputs.iter().cloned().map(Some).collect();
    paths.extend(others.into_iter().cloned().map(Some));
    if expected.contains_key(&None) || actual.contains_key(&None) {
        paths.push(None);
    }

    let files = paths
        .into_iter()
        .map(|path| {
            let expected = expected.remove(&path).unwrap_or_default();
            let actual = actual.remove(&path).unwrap_or_default();
            let identical = expected.len() == actual.len()
                && expected.iter().zip(&actual).all(|(want, have)| {
                    line_matches(want, have).unwrap_or_else(|problem| {
                        problems.insert(format!("allow-list line {want:?} {problem}"));
                        false
                    })
                });
            File {
                path,
                expected,
                actual,
                identical,
            }
        })
        .collect();

    Run {
        name: name.to_owned(),
        files,
        expected_code,
        actual_code: workflint.code,
        stderr: [reference.stderr.clone(), workflint.stderr.clone()],
        applied,
        problems: problems.into_iter().collect(),
    }
}

/// Writes a line that parses as `path:line:col: message [code]` with `/`
/// separators in the path, and keeps any other line as it is.
fn normalize(line: &str) -> String {
    parse_line(line).map_or_else(|_| line.to_owned(), |diagnostic| diagnostic.to_string())
}

/// The lines by their path, in their order. Lines without a position go
/// under `None`.
fn group(lines: Vec<String>) -> BTreeMap<Option<String>, Vec<String>> {
    let mut groups: BTreeMap<Option<String>, Vec<String>> = BTreeMap::new();
    for line in lines {
        groups
            .entry(allow_list::line_path(&line))
            .or_default()
            .push(line);
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(stdout: &str, code: i32) -> Output {
        Output {
            stdout: stdout.to_owned(),
            stderr: String::new(),
            code: Some(code),
        }
    }

    fn entry(upstream: Option<&str>, workflint: Option<&str>) -> Entry {
        Entry {
            anchor: "yaml-error-positions".into(),
            fixture: "err/x".into(),
            upstream: upstream.map(str::to_owned),
            workflint: workflint.map(str::to_owned),
        }
    }

    fn inputs(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|path| (*path).to_owned()).collect()
    }

    fn paths(run: &Run) -> Vec<Option<&str>> {
        run.files.iter().map(|file| file.path.as_deref()).collect()
    }

    #[test]
    fn output_is_grouped_by_file_in_input_order() {
        let stdout = "\
workflows/b.yaml:1:1: b [expression]
workflows/a.yaml:2:1: a [expression]
workflows/a.yaml:3:1: c [expression]
";
        let run = compare(
            "projects/x",
            &inputs(&["workflows/b.yaml", "workflows/a.yaml", "workflows/c.yaml"]),
            &output(stdout, 1),
            &output("", 0),
            &[],
            false,
        );
        assert_eq!(
            paths(&run),
            [
                Some("workflows/b.yaml"),
                Some("workflows/a.yaml"),
                Some("workflows/c.yaml")
            ]
        );
        assert_eq!(
            run.files[1].expected,
            [
                "workflows/a.yaml:2:1: a [expression]",
                "workflows/a.yaml:3:1: c [expression]"
            ]
        );
        assert!(run.files[2].identical);
        assert!(!run.files[0].identical);
    }

    #[test]
    fn paths_that_are_not_inputs_follow_sorted_and_lines_without_a_path_come_last() {
        let run = compare(
            "projects/x",
            &inputs(&["workflows/a.yaml"]),
            &output(".github/actions/z/action.yaml:1:1: m [action]\n", 1),
            &output(
                "Hello, world!\n.github/actions/y/action.yaml:1:1: m [action]\n",
                0,
            ),
            &[],
            false,
        );
        assert_eq!(
            paths(&run),
            [
                Some("workflows/a.yaml"),
                Some(".github/actions/y/action.yaml"),
                Some(".github/actions/z/action.yaml"),
                None
            ]
        );
        assert_eq!(run.files[3].actual, ["Hello, world!"]);
    }

    #[test]
    fn rule_ids_map_to_kinds_and_errors_sort_by_message() {
        let run = compare(
            "err/x",
            &inputs(&["test.yaml"]),
            &output(
                "test.yaml:1:1: a [expression]\ntest.yaml:1:1: b [syntax-check]\n",
                1,
            ),
            &output(
                "test.yaml:1:1: b [yaml-syntax-error]\ntest.yaml:1:1: a [undefined-variable]\n",
                1,
            ),
            &[],
            true,
        );
        assert_eq!(
            run.files[0].actual,
            [
                "test.yaml:1:1: a [expression]",
                "test.yaml:1:1: b [syntax-check]"
            ]
        );
        assert!(run.files[0].identical);
        assert_eq!(run.problems, Vec::<String>::new());
    }

    #[test]
    fn without_kind_mapping_the_brackets_stay() {
        let stdout = "test.yaml:1:1: a [expression]\n";
        let run = compare(
            "err/x",
            &inputs(&["test.yaml"]),
            &output(stdout, 1),
            &output(stdout, 1),
            &[],
            false,
        );
        assert!(run.files[0].identical);
        assert_eq!(run.problems, Vec::<String>::new());
    }

    #[test]
    fn a_rule_id_without_a_kind_is_a_problem() {
        let run = compare(
            "err/x",
            &inputs(&["test.yaml"]),
            &output("test.yaml:1:1: a [expression]\n", 1),
            &output("test.yaml:1:1: a [no-such-rule]\n", 1),
            &[],
            true,
        );
        assert_eq!(run.files[0].actual, ["test.yaml:1:1: a [no-such-rule]"]);
        assert!(!run.files[0].identical);
        assert_eq!(run.problems.len(), 1);
        assert!(run.problems[0].contains("\"no-such-rule\""), "{run:?}");
    }

    #[test]
    fn an_allow_list_entry_explains_a_difference() {
        let replaced = entry(
            Some("test.yaml:1:1: a [syntax-check]"),
            Some("test.yaml:2:5: a [syntax-check]"),
        );
        let run = compare(
            "err/x",
            &inputs(&["test.yaml"]),
            &output("test.yaml:1:1: a [syntax-check]\n", 1),
            &output("test.yaml:2:5: a [yaml-syntax-error]\n", 1),
            &[&replaced],
            true,
        );
        assert!(run.files[0].identical, "{run:?}");
        assert_eq!(run.applied, 1);
        assert_eq!(run.expected_code, Some(1));
        assert_eq!(run.problems, Vec::<String>::new());
    }

    #[test]
    fn an_allow_list_pattern_line_matches_anywhere() {
        let added = entry(None, Some(r"/test\.yaml:3:1: .+ \[expression\]/"));
        let run = compare(
            "err/x",
            &inputs(&["test.yaml"]),
            &output("", 0),
            &output("test.yaml:3:1: anything [undefined-variable]\n", 1),
            &[&added],
            true,
        );
        assert!(run.files[0].identical, "{run:?}");
        assert_eq!(run.expected_code, Some(1));
    }

    #[test]
    fn removing_the_only_error_expects_exit_code_zero() {
        let removed = entry(Some("test.yaml:1:1: a [syntax-check]"), None);
        let run = compare(
            "err/x",
            &inputs(&["test.yaml"]),
            &output("test.yaml:1:1: a [syntax-check]\n", 1),
            &output("", 0),
            &[&removed],
            true,
        );
        assert!(run.files[0].identical);
        assert_eq!((run.expected_code, run.actual_code), (Some(0), Some(0)));
    }

    #[test]
    fn an_unexplained_difference_leaves_the_file_differing() {
        let run = compare(
            "err/x",
            &inputs(&["test.yaml"]),
            &output("test.yaml:1:1: a [syntax-check]\n", 1),
            &output("test.yaml:1:2: a [yaml-syntax-error]\n", 1),
            &[],
            true,
        );
        assert!(!run.files[0].identical);
        assert_eq!(run.files[0].expected, ["test.yaml:1:1: a [syntax-check]"]);
        assert_eq!(run.files[0].actual, ["test.yaml:1:2: a [syntax-check]"]);
        assert_eq!(run.applied, 0);
    }

    #[test]
    fn an_entry_that_does_not_apply_is_a_problem_and_is_skipped() {
        let stale = entry(Some("test.yaml:9:9: gone [syntax-check]"), None);
        let stdout = "test.yaml:1:1: a [syntax-check]\n";
        let run = compare(
            "err/x",
            &inputs(&["test.yaml"]),
            &output(stdout, 1),
            &output("test.yaml:1:1: a [yaml-syntax-error]\n", 1),
            &[&stale],
            true,
        );
        assert!(run.files[0].identical);
        assert_eq!(run.applied, 0);
        assert_eq!(run.problems.len(), 1);
        assert!(run.problems[0].contains("matches no line"), "{run:?}");
    }

    #[test]
    fn the_exit_code_of_a_fatal_error_is_kept() {
        let run = compare(
            "err/x",
            &inputs(&["test.yaml"]),
            &output("", 3),
            &output("", 0),
            &[],
            true,
        );
        assert_eq!((run.expected_code, run.actual_code), (Some(3), Some(0)));
    }

    #[test]
    fn windows_paths_in_workflint_output_group_with_slash_paths() {
        let run = compare(
            "projects/x",
            &inputs(&["workflows/nested/a.yaml"]),
            &output("workflows/nested/a.yaml:1:1: m [expression]\n", 1),
            &output("workflows\\nested\\a.yaml:1:1: m [undefined-variable]\n", 1),
            &[],
            true,
        );
        assert_eq!(paths(&run), [Some("workflows/nested/a.yaml")]);
        assert!(run.files[0].identical);
    }
}
