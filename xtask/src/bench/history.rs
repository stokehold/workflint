//! `cargo xtask bench-history`: appends a run to the history file and compares
//! it with the run before.
//!
//! The history file has one `results.json` per line, oldest first. The nightly
//! benchmark job keeps it on the `benchmarks` branch.

use std::fmt::Write as _;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use anyhow::{Context, Result};

use super::results::{Measurement, Results, SCHEMA_VERSION, ScenarioOutcome, format_seconds};

/// A scenario regresses when its wall time grows by more than this factor
/// against the previous run.
pub(crate) const REGRESSION_FACTOR: f64 = 1.10;

/// The first workflint version that the targets apply to (ADR 0019).
const TARGETS_FROM: (u64, u64) = (0, 1);

/// A scenario that got slower against the previous run.
#[derive(Debug, PartialEq)]
pub(crate) struct Regression {
    pub(crate) scenario: String,
    pub(crate) kind: RegressionKind,
}

#[derive(Debug, PartialEq)]
pub(crate) enum RegressionKind {
    /// workflint's wall time grew by `factor`. When both runs measured the
    /// reference, the times are relative to it, which cancels most of the
    /// difference between runners.
    Slower {
        previous: f64,
        current: f64,
        factor: f64,
        relative: bool,
    },
    /// workflint completed in the previous run and timed out in this one.
    TimedOut,
}

/// The value the regression check compares for one scenario: workflint's
/// mean wall time divided by the reference's when `relative`, else
/// workflint's mean wall time in seconds.
fn compared_value(workflint: &Measurement, reference: &Measurement, relative: bool) -> Option<f64> {
    let workflint = workflint.mean()?;
    if relative {
        Some(workflint / reference.mean().filter(|mean| *mean > 0.0)?)
    } else {
        Some(workflint)
    }
}

/// Compares workflint in `current` with `previous`, scenario by scenario.
/// Scenarios that either run skipped are not compared.
pub(crate) fn regressions(previous: &Results, current: &Results) -> Vec<Regression> {
    let mut found = Vec::new();
    for scenario in &current.scenarios {
        let Some((workflint, reference)) = scenario.measurements() else {
            continue;
        };
        let Some((previous_workflint, previous_reference)) = previous
            .scenarios
            .iter()
            .find(|candidate| candidate.id == scenario.id)
            .and_then(|candidate| candidate.measurements())
        else {
            continue;
        };
        if previous_workflint.mean().is_some() && matches!(workflint, Measurement::TimedOut) {
            found.push(Regression {
                scenario: scenario.id.clone(),
                kind: RegressionKind::TimedOut,
            });
            continue;
        }
        let relative = reference.mean().is_some() && previous_reference.mean().is_some();
        let (Some(previous_value), Some(current_value)) = (
            compared_value(previous_workflint, previous_reference, relative),
            compared_value(workflint, reference, relative),
        ) else {
            continue;
        };
        if previous_value <= 0.0 {
            continue;
        }
        let factor = current_value / previous_value;
        if factor > REGRESSION_FACTOR {
            found.push(Regression {
                scenario: scenario.id.clone(),
                kind: RegressionKind::Slower {
                    previous: previous_value,
                    current: current_value,
                    factor,
                    relative,
                },
            });
        }
    }
    found
}

/// Whether the ADR 0019 targets apply to workflint `version`.
pub(crate) fn targets_apply(version: &str) -> bool {
    let mut parts = version.split(['.', '-', '+']);
    let major = parts.next().and_then(|part| part.parse::<u64>().ok());
    let minor = parts.next().and_then(|part| part.parse::<u64>().ok());
    match (major, minor) {
        (Some(major), Some(minor)) => (major, minor) >= TARGETS_FROM,
        _ => false,
    }
}

/// The targets that `results` missed, as `(scenario title, target)`.
fn missed_targets(results: &Results) -> Vec<(&str, &str)> {
    let mut missed = Vec::new();
    for scenario in &results.scenarios {
        if let ScenarioOutcome::Measured { targets, .. } = &scenario.outcome {
            for target in targets {
                if target.met == Some(false) {
                    missed.push((scenario.title.as_str(), target.target.as_str()));
                }
            }
        }
    }
    missed
}

/// Parses the last entry of the history, if there is one and this version of
/// the command can read it.
fn last_entry(history: &str) -> Option<Results> {
    let line = history.lines().rev().find(|line| !line.trim().is_empty())?;
    let entry: Results = serde_json::from_str(line).ok()?;
    (entry.schema_version == SCHEMA_VERSION).then_some(entry)
}

/// What [`compare`] found.
#[derive(Debug)]
pub(crate) struct Report {
    /// The comparison with the previous entry, as Markdown, for the job
    /// summary.
    pub(crate) summary: String,
    /// The body of the `performance` issue, if there is something to report.
    pub(crate) issue: Option<String>,
}

/// Compares `current` with the last entry of `history`.
pub(crate) fn compare(history: &str, current: &Results) -> Report {
    let mut summary = String::from("## Compared with the previous run\n\n");
    let previous = last_entry(history);
    let found = previous
        .as_ref()
        .map(|previous| regressions(previous, current))
        .unwrap_or_default();
    match &previous {
        None => summary
            .push_str("The history has no earlier entry that this version of the command reads.\n"),
        Some(previous) => {
            let _ = writeln!(
                summary,
                "Previous run: {}, workflint commit {}.\n",
                previous.date,
                previous.workflint.commit.as_deref().unwrap_or("unknown")
            );
            if found.is_empty() {
                let _ = writeln!(
                    summary,
                    "No scenario's wall time grew by more than {:.0}%.",
                    (REGRESSION_FACTOR - 1.0) * 100.0
                );
            } else {
                summary.push_str(&render_regressions(&found));
            }
        }
    }

    let missed = if targets_apply(&current.workflint.version) {
        missed_targets(current)
    } else {
        Vec::new()
    };
    let issue = (!found.is_empty() || !missed.is_empty()).then(|| {
        let mut body = format!(
            "The benchmark run of {} for workflint {} at commit {} found a problem.\n",
            current.date,
            current.workflint.version,
            current.workflint.commit.as_deref().unwrap_or("unknown")
        );
        if !found.is_empty() {
            body.push_str("\n### Regressions against the previous run\n\n");
            body.push_str(&render_regressions(&found));
        }
        if !missed.is_empty() {
            body.push_str("\n### Missed targets\n\n");
            for (scenario, target) in &missed {
                let _ = writeln!(body, "- {scenario}: {target}");
            }
        }
        body
    });
    Report { summary, issue }
}

fn render_regressions(found: &[Regression]) -> String {
    let mut out = String::from("| Scenario | Previous | Current | Change |\n|---|---|---|---|\n");
    for regression in found {
        match &regression.kind {
            RegressionKind::Slower {
                previous,
                current,
                factor,
                relative,
            } => {
                let (previous, current) = if *relative {
                    (
                        format!("{previous:.3} of the reference"),
                        format!("{current:.3} of the reference"),
                    )
                } else {
                    (format_seconds(*previous), format_seconds(*current))
                };
                let _ = writeln!(
                    out,
                    "| {} | {previous} | {current} | +{:.1}% |",
                    regression.scenario,
                    (factor - 1.0) * 100.0
                );
            }
            RegressionKind::TimedOut => {
                let _ = writeln!(out, "| {} | completed | timed out | |", regression.scenario);
            }
        }
    }
    out
}

/// Runs `cargo xtask bench-history`: compares `results` with the last entry of
/// `history`, appends `results` to `history`, prints the comparison, and
/// writes the issue body to `issue_body` if there is something to report.
/// Otherwise it removes `issue_body`.
pub(crate) fn run(history: &Path, results: &Path, issue_body: Option<&Path>) -> Result<()> {
    let text = fs::read_to_string(results)
        .with_context(|| format!("cannot read {}", results.display()))?;
    let current: Results = serde_json::from_str(&text)
        .with_context(|| format!("{} is not a benchmark result", results.display()))?;
    let past = match fs::read_to_string(history) {
        Ok(past) => past,
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(error).with_context(|| format!("cannot read {}", history.display()));
        }
    };

    let report = compare(&past, &current);
    print!("{}", report.summary);

    let mut updated = past;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(&serde_json::to_string(&current)?);
    updated.push('\n');
    fs::write(history, updated).with_context(|| format!("cannot write {}", history.display()))?;

    if let Some(path) = issue_body {
        match &report.issue {
            Some(body) => {
                fs::write(path, body)
                    .with_context(|| format!("cannot write {}", path.display()))?;
            }
            None => match fs::remove_file(path) {
                Err(error) if error.kind() != ErrorKind::NotFound => {
                    return Err(error).with_context(|| format!("cannot remove {}", path.display()));
                }
                _ => {}
            },
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::results::tests::sample;

    #[test]
    fn regression_check_flags_a_slowdown_above_ten_percent() {
        let previous = sample(0.100, 1.0);
        let current = sample(0.1101, 1.0);
        let found = regressions(&previous, &current);
        assert_eq!(found.len(), 1, "{found:?}");
        let RegressionKind::Slower { factor, .. } = found[0].kind else {
            panic!("expected a slowdown, got {found:?}");
        };
        assert!((factor - 1.101).abs() < 1e-9, "{factor}");
    }

    #[test]
    fn regression_check_ignores_a_slowdown_below_ten_percent() {
        let previous = sample(0.100, 1.0);
        let current = sample(0.1099, 1.0);
        assert_eq!(regressions(&previous, &current), []);
    }

    #[test]
    fn a_slower_runner_that_slows_both_binaries_is_no_regression() {
        let previous = sample(0.100, 1.0);
        let current = sample(0.130, 1.3);
        assert_eq!(regressions(&previous, &current), []);
    }

    #[test]
    fn without_the_reference_the_check_compares_wall_time() {
        let mut previous = sample(0.100, 1.0);
        let mut current = sample(0.115, 1.0);
        for results in [&mut previous, &mut current] {
            if let ScenarioOutcome::Measured { reference, .. } = &mut results.scenarios[0].outcome {
                *reference = Measurement::TimedOut;
            }
        }
        let found = regressions(&previous, &current);
        assert!(
            matches!(
                found[..],
                [Regression {
                    kind: RegressionKind::Slower {
                        relative: false,
                        ..
                    },
                    ..
                }]
            ),
            "{found:?}"
        );
    }

    #[test]
    fn a_new_timeout_of_workflint_is_a_regression() {
        let previous = sample(0.100, 1.0);
        let mut current = sample(0.100, 1.0);
        if let ScenarioOutcome::Measured { workflint, .. } = &mut current.scenarios[0].outcome {
            *workflint = Measurement::TimedOut;
        }
        assert_eq!(
            regressions(&previous, &current),
            [Regression {
                scenario: "corpus".to_owned(),
                kind: RegressionKind::TimedOut
            }]
        );
    }

    #[test]
    fn targets_apply_from_version_0_1_0() {
        assert!(!targets_apply("0.0.1"));
        assert!(targets_apply("0.1.0"));
        assert!(targets_apply("0.1.0-rc.1"));
        assert!(targets_apply("1.2.3"));
        assert!(!targets_apply("unknown"));
    }

    #[test]
    fn missed_target_before_0_1_0_opens_no_issue() {
        // workflint at 0.5 of the reference misses the 1/3 target.
        let current = sample(0.5, 1.0);
        let report = compare("", &current);
        assert_eq!(report.issue, None);
    }

    #[test]
    fn missed_target_from_0_1_0_opens_an_issue() {
        let mut current = sample(0.5, 1.0);
        current.workflint.version = "0.1.0".to_owned();
        let issue = compare("", &current).issue.unwrap();
        assert!(issue.contains("### Missed targets"), "{issue}");
        assert!(issue.contains("wall time at most 1/3 of the reference"));
    }

    #[test]
    fn regression_against_the_last_history_line_opens_an_issue() {
        let older = serde_json::to_string(&sample(0.050, 1.0)).unwrap();
        let previous = serde_json::to_string(&sample(0.100, 1.0)).unwrap();
        let history = format!("{older}\n{previous}\n");
        let report = compare(&history, &sample(0.120, 1.0));
        let issue = report.issue.unwrap();
        assert!(
            issue.contains("| corpus | 0.100 of the reference | 0.120 of the reference | +20.0% |"),
            "{issue}"
        );
        assert!(report.summary.contains("+20.0%"));
    }

    #[test]
    fn empty_history_reports_no_earlier_entry() {
        let report = compare("", &sample(0.1, 1.0));
        assert!(
            report.summary.contains("no earlier entry"),
            "{}",
            report.summary
        );
        assert_eq!(report.issue, None);
    }

    #[test]
    fn run_appends_one_line_and_writes_the_issue_body_only_on_a_problem() {
        let directory = tempfile::tempdir().unwrap();
        let history = directory.path().join("history.jsonl");
        let results = directory.path().join("results.json");
        let issue = directory.path().join("issue.md");

        fs::write(
            &results,
            serde_json::to_string_pretty(&sample(0.1, 1.0)).unwrap(),
        )
        .unwrap();
        run(&history, &results, Some(&issue)).unwrap();
        assert_eq!(fs::read_to_string(&history).unwrap().lines().count(), 1);
        assert!(!issue.exists());

        fs::write(
            &results,
            serde_json::to_string_pretty(&sample(0.2, 1.0)).unwrap(),
        )
        .unwrap();
        run(&history, &results, Some(&issue)).unwrap();
        let lines = fs::read_to_string(&history).unwrap();
        assert_eq!(lines.lines().count(), 2);
        assert_eq!(last_entry(&lines), Some(sample(0.2, 1.0)));
        assert!(fs::read_to_string(&issue).unwrap().contains("+100.0%"));

        fs::write(
            &results,
            serde_json::to_string_pretty(&sample(0.2, 1.0)).unwrap(),
        )
        .unwrap();
        run(&history, &results, Some(&issue)).unwrap();
        assert!(!issue.exists());
    }
}
