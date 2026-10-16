//! The benchmark scenarios and targets of ADR 0019.

use std::path::Path;
use std::time::Duration;

use super::results::{Measurement, TargetResult, Unit};

/// What a scenario lints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Input {
    /// Every file of the corpus in `.reference/realworld`.
    Corpus,
    /// One file in the vendored `testdata/bench/`.
    BenchFile(&'static str),
}

/// One scenario: what both binaries lint, with which flags, and how often.
#[derive(Debug)]
pub(crate) struct Scenario {
    pub(crate) id: &'static str,
    pub(crate) title: &'static str,
    pub(crate) input: Input,
    /// Whether the binaries run shellcheck. The scenario needs the pinned
    /// shellcheck.
    pub(crate) shellcheck: bool,
    pub(crate) warmup: u32,
    pub(crate) runs: u32,
    /// The time limit for one run.
    pub(crate) timeout: Duration,
    pub(crate) targets: &'static [Target],
}

/// An ADR 0019 target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Target {
    /// workflint's mean wall time is at most this fraction of the
    /// reference's.
    WallTimeRatio { at_most: f64, label: &'static str },
    /// workflint's peak memory is at most this fraction of the reference's.
    PeakMemoryRatio { at_most: f64, label: &'static str },
    /// workflint's mean wall time is under this many seconds.
    WallTimeUnder { seconds: f64, label: &'static str },
    /// No run of workflint times out.
    NeverTimesOut,
}

/// The scenarios, in the order the command runs them.
pub(crate) const SCENARIOS: [Scenario; 3] = [
    Scenario {
        id: "corpus",
        title: "Corpus, external tools off",
        input: Input::Corpus,
        shellcheck: false,
        warmup: 3,
        runs: 10,
        timeout: Duration::from_secs(120),
        targets: &[
            Target::WallTimeRatio {
                at_most: 1.0 / 3.0,
                label: "1/3",
            },
            Target::PeakMemoryRatio {
                at_most: 0.5,
                label: "1/2",
            },
        ],
    },
    Scenario {
        id: "corpus-shellcheck",
        title: "Corpus, shellcheck on",
        input: Input::Corpus,
        shellcheck: true,
        // The run before hyperfine starts warms the caches. One run of the
        // reference takes about 15 s on a GitHub-hosted Linux runner and
        // minutes on macOS.
        warmup: 0,
        runs: 3,
        timeout: Duration::from_mins(10),
        targets: &[
            Target::WallTimeRatio {
                at_most: 0.2,
                label: "1/5",
            },
            Target::NeverTimesOut,
        ],
    },
    Scenario {
        id: "small",
        title: "small.yaml, end to end",
        input: Input::BenchFile("small.yaml"),
        shellcheck: false,
        warmup: 5,
        runs: 100,
        timeout: Duration::from_secs(10),
        targets: &[Target::WallTimeUnder {
            seconds: 0.010,
            label: "10 ms",
        }],
    },
];

impl Scenario {
    /// The flags both binaries run with. `shellcheck` is the pinned
    /// shellcheck, which the scenarios with shellcheck on pass explicitly so
    /// that both binaries run the same one.
    pub(crate) fn flags(&self, shellcheck: Option<&Path>) -> Vec<String> {
        let shellcheck = match (self.shellcheck, shellcheck) {
            (true, Some(path)) => format!("-shellcheck={}", path.display()),
            _ => "-shellcheck=".to_owned(),
        };
        vec![shellcheck, "-pyflakes=".to_owned()]
    }
}

impl Target {
    /// Compares the measurements with the target.
    pub(crate) fn evaluate(self, workflint: &Measurement, reference: &Measurement) -> TargetResult {
        match self {
            Self::WallTimeRatio { at_most, label } => ratio_target(
                format!("wall time at most {label} of the reference"),
                at_most,
                workflint.mean().zip(reference.mean()),
            ),
            Self::PeakMemoryRatio { at_most, label } => ratio_target(
                format!("peak memory at most {label} of the reference"),
                at_most,
                workflint
                    .peak_memory()
                    .zip(reference.peak_memory())
                    .map(|(w, r)| (bytes(w), bytes(r))),
            ),
            Self::WallTimeUnder { seconds, label } => {
                let value = workflint.mean();
                let ratio = value.map(|value| value / seconds);
                TargetResult {
                    target: format!("wall time under {label}"),
                    unit: Some(Unit::Seconds),
                    limit: Some(seconds),
                    value,
                    ratio,
                    met: ratio.map(|ratio| ratio < 1.0),
                }
            }
            Self::NeverTimesOut => TargetResult {
                target: "no run of workflint times out".to_owned(),
                unit: None,
                limit: None,
                value: None,
                ratio: None,
                met: match workflint {
                    Measurement::Completed { .. } => Some(true),
                    Measurement::TimedOut => Some(false),
                    Measurement::Failed { .. } => None,
                },
            },
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn bytes(count: u64) -> f64 {
    count as f64
}

/// A target on workflint's value divided by the reference's. `pair` is
/// `None` when either measurement is missing.
fn ratio_target(target: String, at_most: f64, pair: Option<(f64, f64)>) -> TargetResult {
    let value = pair
        .filter(|(_, reference)| *reference > 0.0)
        .map(|(workflint, reference)| workflint / reference);
    let ratio = value.map(|value| value / at_most);
    TargetResult {
        target,
        unit: Some(Unit::Ratio),
        limit: Some(at_most),
        value,
        ratio,
        met: ratio.map(|ratio| ratio <= 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::hyperfine;
    use crate::bench::results::tests::completed;

    #[test]
    fn scenario_ids_are_unique() {
        let mut ids: Vec<_> = SCENARIOS.iter().map(|scenario| scenario.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), SCENARIOS.len());
    }

    #[test]
    fn only_the_shellcheck_scenario_passes_shellcheck() {
        let shellcheck = Path::new("/usr/bin/shellcheck");
        assert_eq!(
            SCENARIOS[0].flags(Some(shellcheck)),
            ["-shellcheck=", "-pyflakes="]
        );
        assert_eq!(
            SCENARIOS[1].flags(Some(shellcheck)),
            ["-shellcheck=/usr/bin/shellcheck", "-pyflakes="]
        );
    }

    #[test]
    fn wall_time_ratio_compares_workflint_with_the_reference() {
        let target = Target::WallTimeRatio {
            at_most: 1.0 / 3.0,
            label: "1/3",
        };
        let met = target.evaluate(&completed(0.1), &completed(0.4));
        assert_eq!(met.value, Some(0.25));
        assert_eq!(met.met, Some(true));
        let missed = target.evaluate(&completed(0.2), &completed(0.4));
        assert_eq!(missed.met, Some(false));
        assert!((missed.ratio.unwrap() - 1.5).abs() < 1e-9);
    }

    #[test]
    fn target_against_a_timed_out_reference_is_unknown() {
        let target = Target::WallTimeRatio {
            at_most: 0.2,
            label: "1/5",
        };
        let result = target.evaluate(&completed(0.1), &Measurement::TimedOut);
        assert_eq!(result.value, None);
        assert_eq!(result.met, None);
    }

    #[test]
    fn small_workflow_target_is_strictly_under_ten_milliseconds() {
        let target = SCENARIOS[2].targets[0];
        let reference = completed(0.005);
        assert_eq!(
            target.evaluate(&completed(0.009), &reference).met,
            Some(true)
        );
        assert_eq!(
            target.evaluate(&completed(0.010), &reference).met,
            Some(false)
        );
    }

    /// The time the nightly job needs besides the scenarios: the tool
    /// installs and the builds of both binaries.
    const SETUP: Duration = Duration::from_mins(25);

    /// `timeout-minutes` of the job in `.github/workflows/benchmarks.yml`.
    fn job_timeout() -> Duration {
        let workflow = std::fs::read_to_string(
            crate::reference::Layout::repository()
                .root()
                .join(".github/workflows/benchmarks.yml"),
        )
        .unwrap();
        let minutes: Vec<u64> = workflow
            .lines()
            .filter_map(|line| line.trim().strip_prefix("timeout-minutes:"))
            .map(|value| value.trim().parse().unwrap())
            .collect();
        assert_eq!(minutes.len(), 1, "expected one job with timeout-minutes");
        Duration::from_mins(minutes[0])
    }

    #[test]
    fn nightly_job_timeout_covers_every_scenario_timing_out_for_both_binaries() {
        // Per binary: the first run, then the hyperfine series.
        let worst_case: Duration = SCENARIOS
            .iter()
            .map(|scenario| {
                scenario.timeout
                    + hyperfine::series_limit(scenario.timeout, scenario.warmup, scenario.runs)
            })
            .sum::<Duration>()
            * 2;
        assert!(
            worst_case + SETUP <= job_timeout(),
            "the scenarios can take {} minutes, plus {} for the setup",
            worst_case.as_secs() / 60,
            SETUP.as_secs() / 60
        );
    }

    #[test]
    fn a_timed_out_workflint_misses_the_no_hang_target() {
        let result = Target::NeverTimesOut.evaluate(&Measurement::TimedOut, &completed(1.0));
        assert_eq!(result.met, Some(false));
    }
}
