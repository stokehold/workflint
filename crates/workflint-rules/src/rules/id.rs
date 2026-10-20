//! The `id` rule: job IDs, the job IDs in `needs`, and step IDs that break the naming
//! convention, and step IDs that appear twice in a job.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `rule_id.go`.

use std::borrow::Cow;
use std::collections::BTreeMap;

use workflint_diagnostics::quote::quote;
use workflint_diagnostics::{Category, RuleMeta, Severity, Span};
use workflint_syntax::{Job, KeyedEntry, Step, contains_expression};
use workflint_yaml::simple_lowercase;

use crate::kind::{self, Kind};
use crate::{Context, Rule};

/// A job ID, or a job ID in `needs`, does not match `^[a-zA-Z_][a-zA-Z0-9_-]*$`.
pub(crate) static INVALID_JOB_ID: RuleMeta = RuleMeta {
    id: "invalid-job-id",
    description: "A job ID, or a job ID in `needs`, does not start with an ASCII letter or `_`, or has a character other than an ASCII letter, a digit, `-`, or `_`",
    kind: "id",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-job-id.md",
};

/// A step ID does not match `^[a-zA-Z_][a-zA-Z0-9_-]*$`.
pub(crate) static INVALID_STEP_ID: RuleMeta = RuleMeta {
    id: "invalid-step-id",
    description: "A step ID does not start with an ASCII letter or `_`, or has a character other than an ASCII letter, a digit, `-`, or `_`",
    kind: "id",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-step-id.md",
};

/// Two steps of one job have the same ID, ignoring case.
pub(crate) static DUPLICATE_STEP_ID: RuleMeta = RuleMeta {
    id: "duplicate-step-id",
    description: "Two steps of one job have the same ID, ignoring case",
    kind: "id",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/duplicate-step-id.md",
};

/// The reference's `RuleID`. It keeps the step IDs of the current job, folded, with the span of
/// the first step that has each.
#[derive(Default)]
pub(crate) struct Id<'a> {
    seen: BTreeMap<Cow<'a, str>, Span>,
}

impl<'a> Rule<'a> for Id<'a> {
    fn kind(&self) -> &'static Kind {
        &kind::ID
    }

    fn job_pre(&mut self, job: &'a KeyedEntry<'a, Job<'a>>, cx: &mut Context<'_>) {
        self.seen.clear();
        check_convention(job.key.value(), job.key.span(), &INVALID_JOB_ID, "job", cx);
        // An item that is not a valid string is `None`, and the parser has reported it.
        for need in job
            .value
            .needs
            .value()
            .into_iter()
            .flat_map(|needs| needs.iter().flatten())
        {
            check_convention(need.value(), need.span(), &INVALID_JOB_ID, "job", cx);
        }
    }

    fn step(&mut self, step: &'a Step<'a>, cx: &mut Context<'_>) {
        let Some(id) = step.id.value() else {
            return;
        };
        check_convention(id.value(), id.span(), &INVALID_STEP_ID, "step", cx);
        let folded = simple_lowercase(id.value());
        if let Some(previous) = self.seen.get(&folded) {
            cx.report_in(
                id.source,
                &DUPLICATE_STEP_ID,
                id.span(),
                format!(
                    "step ID {} duplicates. previously defined at {}. step ID must be unique within a job. note that step ID is case insensitive",
                    quote(id.value()),
                    cx.file().line_col(previous.start),
                ),
            );
            return;
        }
        self.seen.insert(folded, id.span());
    }
}

/// Reports `id` with `rule` unless it is empty, contains an expression, or matches
/// `^[a-zA-Z_][a-zA-Z0-9_-]*$`. `what` is `job` or `step`.
fn check_convention(
    id: &str,
    span: Span,
    rule: &'static RuleMeta,
    what: &str,
    cx: &mut Context<'_>,
) {
    if id.is_empty() || contains_expression(id) || follows_convention(id) {
        return;
    }
    cx.report(
        rule,
        span,
        format!(
            "invalid {what} ID {}. {what} ID must start with a letter or _ and contain only alphanumeric characters, -, or _",
            quote(id)
        ),
    );
}

/// Returns whether `id` matches `^[a-zA-Z_][a-zA-Z0-9_-]*$`.
fn follows_convention(id: &str) -> bool {
    let mut bytes = id.bytes();
    bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use workflint_diagnostics::{Color, FileId, SourceFile, render_oneline};

    use super::{Id, follows_convention};
    use crate::{Context, Rule, visit};

    /// Lints `text` with only the `id` rule and returns its diagnostics in the one-line format.
    fn lint(text: &str) -> Vec<String> {
        let file = SourceFile::new(FileId::new(0), "test.yaml", text).unwrap();
        let document = workflint_yaml::parse(&file).document.unwrap();
        let parsed = workflint_syntax::parse(&file, &document);
        let mut rules: Vec<Box<dyn Rule<'_>>> = vec![Box::new(Id::default())];
        let mut cx = Context::new(&file);
        visit(&parsed.workflow, &mut rules, &mut cx);
        let mut out = Vec::new();
        render_oneline(&mut out, &file, &cx.into_diagnostics(), Color::Off).unwrap();
        String::from_utf8(out)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// The invalid IDs of `TestCheckInvalidJobNames` in `rule_id_test.go`.
    const INVALID: [&str; 5] = ["-foo", "v1.2.3", "hello!", "じょぶ", "12345"];

    /// The valid IDs of `TestCheckValidJobNames` in `rule_id_test.go`.
    const VALID: [&str; 8] = [
        "foo-bar",
        "foo_bar",
        "foo--bar",
        "foo__bar",
        "_FOO123-",
        "abcdefhijklmnopqrstuvwxyzABCDEFHIJKLMNOPQRSTUVWXYZ",
        "_____",
        "_-_-_",
    ];

    /// `TestCheckInvalidJobNames` in `rule_id_test.go`: an invalid job ID, need, or step ID gives
    /// exactly one error, of the matching rule ID.
    #[test]
    fn invalid_job_ids_needs_and_step_ids_are_reported_once() {
        for id in INVALID {
            let job = format!(
                "on: push\njobs:\n  '{id}':\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo\n"
            );
            let needs = format!(
                "on: push\njobs:\n  test:\n    needs: ['{id}']\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo\n"
            );
            let step = format!(
                "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo\n        id: '{id}'\n"
            );
            for (text, expected) in [
                (
                    job,
                    format!(
                        "test.yaml:3:3: invalid job ID \"{id}\". job ID must start with a letter or _ and contain only alphanumeric characters, -, or _ [invalid-job-id]"
                    ),
                ),
                (
                    needs,
                    format!(
                        "test.yaml:4:13: invalid job ID \"{id}\". job ID must start with a letter or _ and contain only alphanumeric characters, -, or _ [invalid-job-id]"
                    ),
                ),
                (
                    step,
                    format!(
                        "test.yaml:7:13: invalid step ID \"{id}\". step ID must start with a letter or _ and contain only alphanumeric characters, -, or _ [invalid-step-id]"
                    ),
                ),
            ] {
                assert_eq!(lint(&text), [expected], "{text}");
            }
        }
    }

    /// `TestCheckValidJobNames` in `rule_id_test.go`: the same valid ID as job ID, need, and step
    /// ID gives no error.
    #[test]
    fn valid_job_ids_needs_and_step_ids_pass() {
        for id in VALID {
            let text = format!(
                "on: push\njobs:\n  '{id}':\n    needs: ['{id}']\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo\n        id: '{id}'\n"
            );
            assert_eq!(lint(&text), Vec::<String>::new(), "{id}");
        }
    }

    #[test]
    fn the_convention_is_checked_byte_by_byte_in_ascii() {
        assert!(follows_convention("a"));
        assert!(follows_convention("_"));
        assert!(!follows_convention("-"));
        assert!(!follows_convention(""));
        assert!(!follows_convention("é"));
        assert!(!follows_convention("a é"));
    }

    #[test]
    fn ids_with_an_expression_skip_the_convention() {
        let text = "on: push\njobs:\n  test:\n    needs: ['${{ x }}.y']\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo\n        id: '${{ x }}.y'\n";
        assert_eq!(lint(text), Vec::<String>::new());
    }

    #[test]
    fn a_step_id_that_repeats_in_another_case_is_reported_at_the_second() {
        let text = "on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo
        id: step_id
      - run: echo
        id: STEP_ID
      - run: echo
        id: Step_Id
";
        assert_eq!(
            lint(text),
            [
                r#"test.yaml:9:13: step ID "STEP_ID" duplicates. previously defined at line:7,col:13. step ID must be unique within a job. note that step ID is case insensitive [duplicate-step-id]"#,
                r#"test.yaml:11:13: step ID "Step_Id" duplicates. previously defined at line:7,col:13. step ID must be unique within a job. note that step ID is case insensitive [duplicate-step-id]"#,
            ]
        );
    }

    #[test]
    fn step_ids_are_unique_per_job() {
        let text = "on: push
jobs:
  a:
    runs-on: ubuntu-latest
    steps:
      - run: echo
        id: build
  b:
    runs-on: ubuntu-latest
    steps:
      - run: echo
        id: build
";
        assert_eq!(lint(text), Vec::<String>::new());
    }
}
