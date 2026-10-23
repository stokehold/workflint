//! The results of a benchmark run, as `results.json` and `results.md`.

use std::fmt::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// The version of the `results.json` format. A change to the format that
/// older readers cannot handle increments it.
pub(crate) const SCHEMA_VERSION: u32 = 1;

/// Everything one run of `cargo xtask bench` measured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Results {
    pub(crate) schema_version: u32,
    /// When the run started, in UTC, for example `2026-10-16T03:17:00Z`.
    pub(crate) date: String,
    /// The GitHub Actions event that started the run, for example `schedule`
    /// or `workflow_dispatch`. `None` outside GitHub Actions. The history
    /// compares each run with the latest `schedule` run.
    pub(crate) event: Option<String>,
    pub(crate) workflint: Build,
    pub(crate) reference: Build,
    pub(crate) tools: Tools,
    pub(crate) runner: Runner,
    pub(crate) scenarios: Vec<ScenarioResult>,
}

/// A benchmarked binary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Build {
    pub(crate) version: String,
    /// The git commit it was built from, if known.
    pub(crate) commit: Option<String>,
}

/// The versions of the tools the run used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Tools {
    /// The Go version that built the reference, for example `go1.26.4`.
    pub(crate) go: String,
    pub(crate) hyperfine: String,
    /// `None` when shellcheck is not installed.
    pub(crate) shellcheck: Option<String>,
}

/// The machine the run used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Runner {
    /// `linux` or `macos`, from Rust's `std::env::consts::OS`.
    pub(crate) os: String,
    /// For example `x86_64` or `aarch64`.
    pub(crate) arch: String,
    /// The output of `uname -sr`, if it ran.
    pub(crate) kernel: Option<String>,
    /// The CPU model, if the command could read it.
    pub(crate) cpu: Option<String>,
    pub(crate) logical_cpus: Option<usize>,
    /// The GitHub-hosted runner image and its version, for example
    /// `ubuntu24 20261012.1`, when the run is in GitHub Actions.
    pub(crate) image: Option<String>,
}

/// One scenario of ADR 0019.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ScenarioResult {
    /// A stable name, which the history uses to match scenarios.
    pub(crate) id: String,
    pub(crate) title: String,
    /// The number of workflow files on the command line.
    pub(crate) files: usize,
    /// The flags both binaries ran with, before the files.
    pub(crate) flags: Vec<String>,
    pub(crate) warmup: u32,
    pub(crate) runs: u32,
    /// The time limit for one run, in seconds.
    pub(crate) timeout_seconds: f64,
    #[serde(flatten)]
    pub(crate) outcome: ScenarioOutcome,
}

/// Whether a scenario ran.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum ScenarioOutcome {
    Measured {
        workflint: Measurement,
        reference: Measurement,
        targets: Vec<TargetResult>,
    },
    Skipped {
        reason: String,
    },
}

impl ScenarioResult {
    /// The measurements of workflint and the reference, if the scenario ran.
    pub(crate) fn measurements(&self) -> Option<(&Measurement, &Measurement)> {
        match &self.outcome {
            ScenarioOutcome::Measured {
                workflint,
                reference,
                ..
            } => Some((workflint, reference)),
            ScenarioOutcome::Skipped { .. } => None,
        }
    }
}

/// What hyperfine measured for one binary in one scenario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum Measurement {
    Completed {
        /// Wall time of one run, in seconds.
        wall_time: Stats,
        /// The largest peak resident memory of any run, in bytes, if
        /// hyperfine could measure it.
        peak_memory_bytes: Option<u64>,
    },
    /// A run exceeded the scenario's time limit and was killed.
    TimedOut,
    /// The binary or hyperfine failed.
    Failed { reason: String },
}

impl Measurement {
    /// The mean wall time in seconds, if the runs completed.
    pub(crate) fn mean(&self) -> Option<f64> {
        match self {
            Self::Completed { wall_time, .. } => Some(wall_time.mean),
            Self::TimedOut | Self::Failed { .. } => None,
        }
    }

    /// The peak memory in bytes, if the runs completed and hyperfine
    /// measured it.
    pub(crate) fn peak_memory(&self) -> Option<u64> {
        match self {
            Self::Completed {
                peak_memory_bytes, ..
            } => *peak_memory_bytes,
            Self::TimedOut | Self::Failed { .. } => None,
        }
    }
}

/// Summary statistics of the wall time over all runs, in seconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Stats {
    pub(crate) runs: u64,
    pub(crate) mean: f64,
    /// `None` for a single run.
    pub(crate) stddev: Option<f64>,
    pub(crate) min: f64,
    pub(crate) max: f64,
}

/// How a scenario did against one of its ADR 0019 targets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct TargetResult {
    /// The target in words, for example `wall time at most 1/3 of the
    /// reference`.
    pub(crate) target: String,
    /// The unit of `limit` and `value`. `None` for a target without a
    /// number.
    pub(crate) unit: Option<Unit>,
    /// The limit. `None` for a target without a number.
    pub(crate) limit: Option<f64>,
    /// The measured value.
    pub(crate) value: Option<f64>,
    /// `value` divided by `limit`. At most 1 means the target is met.
    pub(crate) ratio: Option<f64>,
    /// `None` when a measurement is missing.
    pub(crate) met: Option<bool>,
}

impl TargetResult {
    /// The measured value as the results print it: seconds as time, a ratio
    /// to three places, and `n/a` without a value.
    pub(crate) fn formatted_value(&self) -> String {
        match (self.value, self.unit) {
            (Some(value), Some(Unit::Seconds)) => format_seconds(value),
            (Some(value), _) => format_number(value),
            (None, _) => "n/a".to_owned(),
        }
    }
}

/// The unit of a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Unit {
    /// workflint's value divided by the reference's.
    Ratio,
    Seconds,
}

/// Formats `time` as an ISO 8601 date and time in UTC, to the second.
pub(crate) fn format_utc(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let days = i64::try_from(seconds / 86_400).unwrap_or(0);
    let (year, month, day) = civil_from_days(days);
    let rest = seconds % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// The proleptic Gregorian date of the day `days` after 1970-01-01, from
/// Howard Hinnant's `civil_from_days`.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (
        year,
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

/// Renders the results as Markdown: one table of the measurements and one of
/// the targets.
pub(crate) fn render_markdown(results: &Results) -> String {
    let mut out = String::new();
    out.push_str("## Benchmark results\n\n");
    let workflint = describe_build(&results.workflint);
    let reference = describe_build(&results.reference);
    let shellcheck = results
        .tools
        .shellcheck
        .as_deref()
        .unwrap_or("not installed");
    let _ = writeln!(
        out,
        "{}: workflint {workflint}, reference {reference}, Go {}, hyperfine {}, shellcheck \
         {shellcheck}.\n",
        results.date, results.tools.go, results.tools.hyperfine
    );
    let _ = writeln!(out, "Runner: {}.\n", describe_runner(&results.runner));

    out.push_str("| Scenario | Binary | Mean | Std dev | Min | Max | Peak memory |\n");
    out.push_str("|---|---|---|---|---|---|---|\n");
    for scenario in &results.scenarios {
        match &scenario.outcome {
            ScenarioOutcome::Skipped { reason } => {
                let _ = writeln!(
                    out,
                    "| {} | both | skipped: {reason} | | | | |",
                    scenario.title
                );
            }
            ScenarioOutcome::Measured {
                workflint,
                reference,
                ..
            } => {
                for (binary, measurement) in [("workflint", workflint), ("reference", reference)] {
                    let cells = measurement_cells(measurement, scenario.timeout_seconds);
                    let _ = writeln!(out, "| {} | {binary} | {cells} |", scenario.title);
                }
            }
        }
    }

    out.push_str("\n| Scenario | Target | Value | Ratio | Met |\n");
    out.push_str("|---|---|---|---|---|\n");
    for scenario in &results.scenarios {
        let ScenarioOutcome::Measured { targets, .. } = &scenario.outcome else {
            continue;
        };
        for target in targets {
            let value = target.formatted_value();
            let ratio = target.ratio.map_or_else(|| "n/a".to_owned(), format_number);
            let met = match target.met {
                Some(true) => "yes",
                Some(false) => "no",
                None => "unknown",
            };
            let _ = writeln!(
                out,
                "| {} | {} | {value} | {ratio} | {met} |",
                scenario.title, target.target
            );
        }
    }
    out
}

fn describe_build(build: &Build) -> String {
    match &build.commit {
        Some(commit) => format!("{} ({})", build.version, short(commit)),
        None => build.version.clone(),
    }
}

fn short(commit: &str) -> &str {
    commit.get(..7).unwrap_or(commit)
}

fn describe_runner(runner: &Runner) -> String {
    let mut parts = vec![format!("{} {}", runner.os, runner.arch)];
    if let Some(kernel) = &runner.kernel {
        parts.push(kernel.clone());
    }
    if let Some(cpu) = &runner.cpu {
        parts.push(cpu.clone());
    }
    if let Some(cpus) = runner.logical_cpus {
        parts.push(format!("{cpus} logical CPUs"));
    }
    if let Some(image) = &runner.image {
        parts.push(format!("image {image}"));
    }
    parts.join(", ")
}

fn measurement_cells(measurement: &Measurement, timeout_seconds: f64) -> String {
    match measurement {
        Measurement::Completed {
            wall_time,
            peak_memory_bytes,
        } => {
            let stddev = wall_time
                .stddev
                .map_or_else(|| "n/a".to_owned(), format_seconds);
            let memory = peak_memory_bytes.map_or_else(|| "n/a".to_owned(), format_bytes);
            format!(
                "{} | {stddev} | {} | {} | {memory}",
                format_seconds(wall_time.mean),
                format_seconds(wall_time.min),
                format_seconds(wall_time.max)
            )
        }
        Measurement::TimedOut => format!("timed out after {timeout_seconds} s | | | |"),
        Measurement::Failed { reason } => format!("failed: {reason} | | | |"),
    }
}

/// Formats a time in seconds as milliseconds below one second and as seconds
/// above.
pub(crate) fn format_seconds(seconds: f64) -> String {
    if seconds < 1.0 {
        format!("{:.1} ms", seconds * 1000.0)
    } else {
        format!("{seconds:.2} s")
    }
}

/// Formats a byte count in mebibytes.
#[allow(clippy::cast_precision_loss)]
fn format_bytes(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / 1_048_576.0)
}

/// Formats a ratio with three decimal places.
fn format_number(value: f64) -> String {
    format!("{value:.3}")
}

#[cfg(test)]
pub(crate) mod tests {
    use std::time::Duration;

    use super::*;

    /// Results with one measured scenario, for tests here and in the history.
    pub(crate) fn sample(workflint_mean: f64, reference_mean: f64) -> Results {
        Results {
            schema_version: SCHEMA_VERSION,
            date: "2026-10-16T03:17:00Z".to_owned(),
            event: Some("schedule".to_owned()),
            workflint: Build {
                version: "0.0.1".to_owned(),
                commit: Some("0123456789abcdef0123456789abcdef01234567".to_owned()),
            },
            reference: Build {
                version: "v0.0.0-20260419144658-011a6d15e749".to_owned(),
                commit: Some("011a6d15e749bb3f2d771eed9c7aa0e7e3e10ee7".to_owned()),
            },
            tools: Tools {
                go: "go1.26.4".to_owned(),
                hyperfine: "2.0.0".to_owned(),
                shellcheck: Some("0.11.0".to_owned()),
            },
            runner: Runner {
                os: "linux".to_owned(),
                arch: "x86_64".to_owned(),
                kernel: Some("Linux 6.11.0".to_owned()),
                cpu: Some("AMD EPYC 7763 64-Core Processor".to_owned()),
                logical_cpus: Some(4),
                image: Some("ubuntu24 20261012.1".to_owned()),
            },
            scenarios: vec![ScenarioResult {
                id: "corpus".to_owned(),
                title: "Corpus, external tools off".to_owned(),
                files: 1503,
                flags: vec!["-shellcheck=".to_owned(), "-pyflakes=".to_owned()],
                warmup: 3,
                runs: 10,
                timeout_seconds: 120.0,
                outcome: ScenarioOutcome::Measured {
                    workflint: completed(workflint_mean),
                    reference: completed(reference_mean),
                    targets: vec![TargetResult {
                        target: "wall time at most 1/3 of the reference".to_owned(),
                        unit: Some(Unit::Ratio),
                        limit: Some(1.0 / 3.0),
                        value: Some(workflint_mean / reference_mean),
                        ratio: Some(workflint_mean / reference_mean * 3.0),
                        met: Some(workflint_mean / reference_mean <= 1.0 / 3.0),
                    }],
                },
            }],
        }
    }

    pub(crate) fn completed(mean: f64) -> Measurement {
        Measurement::Completed {
            wall_time: Stats {
                runs: 10,
                mean,
                stddev: Some(mean / 100.0),
                min: mean * 0.98,
                max: mean * 1.02,
            },
            peak_memory_bytes: Some(36 * 1_048_576),
        }
    }

    #[test]
    fn results_round_trip_through_json() {
        let mut results = sample(0.1, 0.3);
        results.scenarios.push(ScenarioResult {
            id: "corpus-shellcheck".to_owned(),
            title: "Corpus, shellcheck on".to_owned(),
            files: 1503,
            flags: Vec::new(),
            warmup: 0,
            runs: 3,
            timeout_seconds: 1800.0,
            outcome: ScenarioOutcome::Skipped {
                reason: "shellcheck is not installed".to_owned(),
            },
        });
        let json = serde_json::to_string(&results).unwrap();
        assert_eq!(serde_json::from_str::<Results>(&json).unwrap(), results);
    }

    #[test]
    fn results_without_an_event_read_as_a_run_outside_github_actions() {
        let mut json = serde_json::to_value(sample(0.1, 0.3)).unwrap();
        json.as_object_mut().unwrap().remove("event");
        let results: Results = serde_json::from_value(json).unwrap();
        assert_eq!(results.event, None);
    }

    #[test]
    fn statuses_appear_as_snake_case_tags() {
        let json = serde_json::to_value(Measurement::TimedOut).unwrap();
        assert_eq!(json, serde_json::json!({ "status": "timed_out" }));
        let json = serde_json::to_value(&sample(0.1, 0.3).scenarios[0]).unwrap();
        assert_eq!(json["status"], "measured");
        assert_eq!(json["workflint"]["status"], "completed");
    }

    #[test]
    fn dates_are_formatted_in_utc() {
        assert_eq!(format_utc(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        let leap_day = UNIX_EPOCH + Duration::from_secs(951_782_400 + 3_723);
        assert_eq!(format_utc(leap_day), "2000-02-29T01:02:03Z");
        let date = UNIX_EPOCH + Duration::from_mins(29_864_357);
        assert_eq!(format_utc(date), "2026-10-13T03:17:00Z");
    }

    #[test]
    fn markdown_has_a_row_per_binary_and_target() {
        let mut results = sample(0.1, 0.3);
        if let ScenarioOutcome::Measured { reference, .. } = &mut results.scenarios[0].outcome {
            *reference = Measurement::TimedOut;
        }
        let markdown = render_markdown(&results);
        assert!(markdown.contains(
            "| Corpus, external tools off | workflint | 100.0 ms | 1.0 ms | 98.0 ms | 102.0 ms | \
             36.0 MiB |"
        ));
        assert!(markdown.contains(
            "| Corpus, external tools off | reference | timed out after 120 s | | | | |"
        ));
        assert!(markdown.contains("| wall time at most 1/3 of the reference |"));
        assert!(markdown.contains("workflint 0.0.1 (0123456)"));
    }

    #[test]
    fn times_switch_to_seconds_at_one_second() {
        assert_eq!(format_seconds(0.004_24), "4.2 ms");
        assert_eq!(format_seconds(12.345), "12.35 s");
    }
}
