//! Compares the outcomes of both checkers and applies the allow-list in
//! `tests/expr-parity/allowed-differences.toml`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::outcome::Outcome;
use super::setups::Setup;

/// One expression to check in one setup. The Go program reads it as JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Case {
    /// The index of the setup.
    pub(crate) setup: usize,
    pub(crate) expression: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    difference: Vec<Record>,
    #[serde(default, rename = "type")]
    types: Vec<TypeRecord>,
}

/// A difference that `docs/differences.md` explains.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    /// An anchor in `docs/differences.md`, without the `#`.
    pub(crate) anchor: String,
    /// The setups the record applies to. Empty means every setup.
    #[serde(default)]
    pub(crate) setups: Vec<String>,
    pub(crate) expression: String,
    /// The reference's outcome, as [`Outcome`] displays it.
    pub(crate) upstream: String,
    /// workflint's outcome, as [`Outcome`] displays it.
    pub(crate) workflint: String,
}

impl Record {
    fn matches(&self, setup: &str, expression: &str, upstream: &str, workflint: &str) -> bool {
        self.expression == expression
            && (self.setups.is_empty() || self.setups.iter().any(|name| name == setup))
            && self.upstream.trim_end_matches('\n') == upstream
            && self.workflint.trim_end_matches('\n') == workflint
    }

    fn describe(&self) -> String {
        format!(
            "the record for {:?} with anchor {}",
            self.expression, self.anchor
        )
    }
}

/// A type that workflint prints differently in every outcome, because
/// `docs/differences.md` explains a change to it, such as an added property of
/// a context.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TypeRecord {
    /// An anchor in `docs/differences.md`, without the `#`.
    pub(crate) anchor: String,
    /// The type as the reference prints it.
    pub(crate) upstream: String,
    /// The type as workflint prints it.
    pub(crate) workflint: String,
}

impl TypeRecord {
    fn describe(&self) -> String {
        format!(
            "the type record for {:?} with anchor {}",
            self.upstream, self.anchor
        )
    }
}

/// The records of `tests/expr-parity/allowed-differences.toml`.
#[derive(Debug, Default)]
pub(crate) struct AllowList {
    records: Vec<Record>,
    types: Vec<TypeRecord>,
}

impl AllowList {
    /// Parses the file.
    pub(crate) fn parse(text: &str) -> Result<Self> {
        let file: File = toml::from_str(text).context("cannot parse the allow-list")?;
        Ok(Self {
            records: file.difference,
            types: file.types,
        })
    }

    /// Rewrites each type of the type records in the reference's outcome
    /// `upstream` as workflint prints it. Returns the rewritten outcome and the
    /// indexes of the records that changed it.
    fn rewrite_types(&self, upstream: &str) -> (String, Vec<usize>) {
        let mut text = upstream.to_owned();
        let mut applied = Vec::new();
        for (index, record) in self.types.iter().enumerate() {
            if text.contains(&record.upstream) {
                text = text.replace(&record.upstream, &record.workflint);
                applied.push(index);
            }
        }
        (text, applied)
    }

    /// Fails unless every record names an anchor in `anchors` and only setups
    /// in `setups`.
    pub(crate) fn validate(&self, anchors: &BTreeSet<String>, setups: &[Setup]) -> Result<()> {
        for record in &self.types {
            if !anchors.contains(&record.anchor) {
                bail!(
                    "anchor {:?} of {} does not exist in docs/differences.md",
                    record.anchor,
                    record.describe()
                );
            }
        }
        for record in &self.records {
            if !anchors.contains(&record.anchor) {
                bail!(
                    "anchor {:?} of {} does not exist in docs/differences.md",
                    record.anchor,
                    record.describe()
                );
            }
            for name in &record.setups {
                if !setups.iter().any(|setup| &setup.name == name) {
                    bail!("setup {name:?} of {} does not exist", record.describe());
                }
            }
        }
        Ok(())
    }
}

/// A difference that no record explains.
#[derive(Debug)]
pub(crate) struct Difference {
    pub(crate) setup: String,
    pub(crate) expression: String,
    pub(crate) upstream: Outcome,
    pub(crate) workflint: Outcome,
}

/// The result of a comparison.
#[derive(Debug, Default)]
pub(crate) struct Report {
    /// The number of cases.
    pub(crate) compared: usize,
    /// The number of setups.
    pub(crate) setups: usize,
    /// The number of differences each anchor explains.
    pub(crate) allowed: BTreeMap<String, usize>,
    pub(crate) unexplained: Vec<Difference>,
    /// The records that explain no difference.
    pub(crate) stale: Vec<String>,
}

/// Compares `upstream`, the reference's outcome of each case, with the outcome
/// that `workflint` gives for the setup index and the expression.
pub(crate) fn compare(
    setups: &[Setup],
    cases: &[Case],
    upstream: &[Outcome],
    mut workflint: impl FnMut(usize, &str) -> Outcome,
    allow_list: &AllowList,
) -> Result<Report> {
    if cases.len() != upstream.len() {
        bail!(
            "the reference checked {} expressions, but {} were sent",
            upstream.len(),
            cases.len()
        );
    }
    let mut report = Report {
        compared: cases.len(),
        setups: setups.len(),
        ..Report::default()
    };
    let mut used = vec![false; allow_list.records.len()];
    let mut used_types = vec![false; allow_list.types.len()];
    for (case, upstream) in cases.iter().zip(upstream) {
        let setup = setups
            .get(case.setup)
            .with_context(|| format!("case {:?} has no setup", case.expression))?;
        let upstream = upstream.clone().normalized();
        let workflint = workflint(case.setup, &case.expression).normalized();
        if upstream == workflint {
            continue;
        }
        let (upstream_text, workflint_text) = (upstream.to_string(), workflint.to_string());
        let record = allow_list.records.iter().position(|record| {
            record.matches(
                &setup.name,
                &case.expression,
                &upstream_text,
                &workflint_text,
            )
        });
        let (rewritten, applied) = allow_list.rewrite_types(&upstream_text);
        if let Some(index) = record {
            used[index] = true;
            *report
                .allowed
                .entry(allow_list.records[index].anchor.clone())
                .or_default() += 1;
        } else if !applied.is_empty() && rewritten == workflint_text {
            for &index in &applied {
                used_types[index] = true;
            }
            let anchor = applied
                .first()
                .and_then(|&index| allow_list.types.get(index))
                .map(|record| record.anchor.clone())
                .unwrap_or_default();
            *report.allowed.entry(anchor).or_default() += 1;
        } else {
            report.unexplained.push(Difference {
                setup: setup.name.clone(),
                expression: case.expression.clone(),
                upstream,
                workflint,
            });
        }
    }
    report.stale = allow_list
        .records
        .iter()
        .zip(used)
        .filter(|(_, used)| !used)
        .map(|(record, _)| record.describe())
        .chain(
            allow_list
                .types
                .iter()
                .zip(used_types)
                .filter(|(_, used)| !used)
                .map(|(record, _)| record.describe()),
        )
        .collect();
    Ok(report)
}

impl Report {
    /// The summary, then each unexplained difference and each stale record.
    pub(crate) fn render(&self) -> String {
        let allowed: usize = self.allowed.values().sum();
        let mut text = format!(
            "Compared {} expressions in {} setups: {} identical, {allowed} explained by \
             docs/differences.md, {} unexplained, {} stale allow-list records.\n",
            self.compared,
            self.setups,
            self.compared - allowed - self.unexplained.len(),
            self.unexplained.len(),
            self.stale.len(),
        );
        for (anchor, count) in &self.allowed {
            let _ = writeln!(text, "  {anchor}: {count}");
        }
        for difference in &self.unexplained {
            let _ = writeln!(
                text,
                "\nsetup {:?}, expression {:?}\n  reference:\n{}\n  workflint:\n{}",
                difference.setup,
                difference.expression,
                indent(&difference.upstream.to_string()),
                indent(&difference.workflint.to_string()),
            );
        }
        if !self.stale.is_empty() {
            text.push_str("\nAllow-list records that explain no difference:\n");
            for stale in &self.stale {
                let _ = writeln!(text, "  {stale}");
            }
        }
        text
    }

    /// Fails if a difference is unexplained or a record is stale.
    pub(crate) fn check(&self) -> Result<()> {
        match (self.unexplained.len(), self.stale.len()) {
            (0, 0) => Ok(()),
            (unexplained, stale) => {
                bail!("{unexplained} unexplained differences and {stale} stale allow-list records")
            }
        }
    }
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|line| format!("    {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use workflint_expr::TypeEnv;

    use super::super::outcome::{self, Message};
    use super::super::setups;
    use super::*;

    fn setups() -> Vec<Setup> {
        setups::parse(
            r#"
[[setup]]
name = "run"
key = "jobs.<job_id>.steps.run"
expressions = ["cases"]
untrusted = true
matrix = "{os: string}"

[[setup]]
name = "if"
key = "jobs.<job_id>.if"
expressions = ["cases"]
"#,
        )
        .unwrap()
    }

    const EXPRESSIONS: [&str; 10] = [
        "matrix",
        "matrix.arch && foo",
        "matrix.os",
        "matrix.arch",
        "github.event.issue.title",
        "contains(github.ref, 'x') && success()",
        "format('{0} {1}', 1)",
        "steps.*.outputs",
        "1 == 1",
        "a.",
    ];

    fn cases(setups: &[Setup]) -> Vec<Case> {
        (0..setups.len())
            .flat_map(|setup| {
                EXPRESSIONS.iter().map(move |expression| Case {
                    setup,
                    expression: (*expression).to_owned(),
                })
            })
            .collect()
    }

    fn envs(setups: &[Setup]) -> Vec<TypeEnv> {
        setups.iter().map(Setup::type_env).collect()
    }

    /// The outcomes of the real checker, standing in for the reference's.
    fn upstream(setups: &[Setup], cases: &[Case]) -> Vec<Outcome> {
        let envs = envs(setups);
        cases
            .iter()
            .map(|case| outcome::check(&envs[case.setup], &case.expression))
            .collect()
    }

    /// Compares the real checker's outcomes with those of `checker`, which
    /// changes them.
    fn compare_with(
        allow_list: &AllowList,
        mut break_outcome: impl FnMut(&str, Outcome) -> Outcome,
    ) -> Report {
        let setups = setups();
        let cases = cases(&setups);
        let upstream = upstream(&setups, &cases);
        let envs = envs(&setups);
        compare(
            &setups,
            &cases,
            &upstream,
            |setup, expression| break_outcome(expression, outcome::check(&envs[setup], expression)),
            allow_list,
        )
        .unwrap()
    }

    #[test]
    fn identical_checkers_pass() {
        let report = compare_with(&AllowList::default(), |_, outcome| outcome);
        assert_eq!(report.compared, 2 * EXPRESSIONS.len());
        report.check().unwrap();
    }

    fn errors_mut(outcome: &mut Outcome) -> Option<&mut Vec<Message>> {
        match outcome {
            Outcome::Checked { errors, .. } => Some(errors),
            Outcome::ParseError { .. } => None,
        }
    }

    #[test]
    fn a_checker_that_drops_an_error_fails() {
        let report = compare_with(&AllowList::default(), |_, mut outcome| {
            if let Some(errors) = errors_mut(&mut outcome) {
                errors.pop();
            }
            outcome
        });
        // Every setup reports `matrix.arch` and the format error, and only
        // the first reports untrusted input and the special function.
        assert!(report.unexplained.len() >= 4, "{}", report.render());
        assert!(report.check().is_err());
    }

    #[test]
    fn a_checker_that_reorders_errors_fails() {
        let report = compare_with(&AllowList::default(), |_, mut outcome| {
            if let Some(errors) = errors_mut(&mut outcome) {
                errors.reverse();
            }
            outcome
        });
        // `matrix.arch && foo` has errors at two positions.
        assert!(!report.unexplained.is_empty(), "{}", report.render());
        assert!(report.check().is_err());
    }

    #[test]
    fn a_checker_with_another_column_fails() {
        let report = compare_with(&AllowList::default(), |_, mut outcome| {
            match &mut outcome {
                Outcome::ParseError { parse_error } => parse_error.column += 1,
                Outcome::Checked { errors, .. } => {
                    for error in errors {
                        error.column += 1;
                    }
                }
            }
            outcome
        });
        let parse_errors = report
            .unexplained
            .iter()
            .filter(|d| d.expression == "a.")
            .count();
        assert_eq!(parse_errors, 2, "{}", report.render());
        assert!(report.check().is_err());
    }

    #[test]
    fn a_checker_with_another_type_or_constant_flag_fails() {
        for flip_constant in [false, true] {
            let report = compare_with(&AllowList::default(), |expression, mut outcome| {
                if let (Outcome::Checked { ty, constant, .. }, "1 == 1") =
                    (&mut outcome, expression)
                {
                    if flip_constant {
                        *constant = !*constant;
                    } else {
                        *ty = "string".to_owned();
                    }
                }
                outcome
            });
            assert_eq!(report.unexplained.len(), 2, "{}", report.render());
            assert!(report.check().is_err());
        }
    }

    fn changed_type(expression: &str, mut outcome: Outcome) -> Outcome {
        if let (Outcome::Checked { ty, .. }, "1 == 1") = (&mut outcome, expression) {
            *ty = "number".to_owned();
        }
        outcome
    }

    #[test]
    fn a_record_explains_the_difference_it_names() {
        let allow_list = AllowList::parse(
            r#"
[[difference]]
anchor = "comparison-precedence"
setups = ["run"]
expression = "1 == 1"
upstream = "type: bool, constant: true"
workflint = """
type: number, constant: true
"""
"#,
        )
        .unwrap();
        let report = compare_with(&allow_list, changed_type);
        assert_eq!(report.allowed["comparison-precedence"], 1);
        // The record names only the first setup.
        assert_eq!(report.unexplained.len(), 1, "{}", report.render());
        assert_eq!(report.unexplained[0].setup, "if");

        let every_setup = AllowList::parse(
            r#"
[[difference]]
anchor = "comparison-precedence"
expression = "1 == 1"
upstream = "type: bool, constant: true"
workflint = "type: number, constant: true"
"#,
        )
        .unwrap();
        let report = compare_with(&every_setup, changed_type);
        assert_eq!(report.allowed["comparison-precedence"], 2);
        report.check().unwrap();
    }

    #[test]
    fn a_record_whose_outcome_changed_is_stale() {
        let allow_list = AllowList::parse(
            r#"
[[difference]]
anchor = "comparison-precedence"
expression = "1 == 1"
upstream = "type: bool, constant: true"
workflint = "type: string, constant: true"
"#,
        )
        .unwrap();
        let report = compare_with(&allow_list, changed_type);
        assert_eq!(report.unexplained.len(), 2);
        assert_eq!(report.stale.len(), 1);
        let rendered = report.render();
        assert!(rendered.contains("explain no difference"), "{rendered}");
        assert!(report.check().is_err());
    }

    /// Adds the property `extra` to the type `{os: string}` of `matrix`.
    fn added_property(_: &str, mut outcome: Outcome) -> Outcome {
        if let Outcome::Checked { ty, .. } = &mut outcome {
            *ty = ty.replace("{os: string}", "{extra: string; os: string}");
        }
        outcome
    }

    #[test]
    fn a_type_record_explains_every_outcome_that_prints_the_type() {
        let allow_list = AllowList::parse(
            r#"
[[type]]
anchor = "platform-data-updates"
upstream = "{os: string}"
workflint = "{extra: string; os: string}"
"#,
        )
        .unwrap();
        let report = compare_with(&allow_list, added_property);
        // Only the first setup has a matrix, and only `matrix` prints it.
        assert_eq!(report.allowed["platform-data-updates"], 1);
        report.check().unwrap();
    }

    #[test]
    fn a_type_record_does_not_explain_other_changes() {
        let allow_list = AllowList::parse(
            r#"
[[type]]
anchor = "platform-data-updates"
upstream = "{os: string}"
workflint = "{extra: string; os: string}"
"#,
        )
        .unwrap();
        let report = compare_with(&allow_list, |expression, outcome| {
            changed_type(expression, added_property(expression, outcome))
        });
        assert_eq!(report.unexplained.len(), 2, "{}", report.render());
        assert!(report.unexplained.iter().all(|d| d.expression == "1 == 1"));
    }

    #[test]
    fn a_type_record_that_applies_nowhere_is_stale() {
        let allow_list = AllowList::parse(
            r#"
[[type]]
anchor = "platform-data-updates"
upstream = "{arch: string}"
workflint = "{arch: string; extra: string}"
"#,
        )
        .unwrap();
        let report = compare_with(&allow_list, |_, outcome| outcome);
        assert_eq!(report.stale.len(), 1);
        assert!(report.check().is_err());
    }

    #[test]
    fn records_must_name_known_anchors_and_setups() {
        let setups = setups();
        let anchors = BTreeSet::from(["literal-index-case".to_owned()]);
        let record = |anchor: &str, setup: &str| {
            AllowList::parse(&format!(
                "[[difference]]\nanchor = {anchor:?}\nsetups = [{setup:?}]\n\
                 expression = \"x\"\nupstream = \"a\"\nworkflint = \"b\"\n"
            ))
            .unwrap()
        };
        record("literal-index-case", "run")
            .validate(&anchors, &setups)
            .unwrap();
        let error = record("no-such-anchor", "run")
            .validate(&anchors, &setups)
            .unwrap_err();
        assert!(format!("{error:#}").contains("no-such-anchor"), "{error:#}");
        let error = record("literal-index-case", "other")
            .validate(&anchors, &setups)
            .unwrap_err();
        assert!(format!("{error:#}").contains("\"other\""), "{error:#}");
        assert!(AllowList::parse("[[difference]]\nanchor = \"a\"\n").is_err());
        let error = AllowList::parse(
            "[[type]]\nanchor = \"no-such-anchor\"\nupstream = \"a\"\nworkflint = \"b\"\n",
        )
        .unwrap()
        .validate(&anchors, &setups)
        .unwrap_err();
        assert!(format!("{error:#}").contains("no-such-anchor"), "{error:#}");
    }

    #[test]
    fn a_missing_outcome_is_an_error() {
        let setups = setups();
        let cases = cases(&setups);
        let error = compare(
            &setups,
            &cases,
            &[],
            |_, _| unreachable!(),
            &AllowList::default(),
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("were sent"), "{error:#}");
    }
}
