//! Measures one binary in one scenario: a first run under the time limit, then
//! hyperfine.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::results::{Measurement, Stats};
use xtask::process::{self, Finished};

/// The exit code with which both binaries report that they found problems.
/// Every scenario's input has some, so hyperfine accepts it.
const FOUND_PROBLEMS: i32 = 1;

/// One binary in one scenario.
#[derive(Debug)]
pub(crate) struct Job<'a> {
    pub(crate) hyperfine: &'a Path,
    pub(crate) program: &'a Path,
    pub(crate) args: &'a [String],
    pub(crate) cwd: &'a Path,
    pub(crate) warmup: u32,
    pub(crate) runs: u32,
    /// The time limit for one run.
    pub(crate) timeout: Duration,
    /// Where the output of the first run goes.
    pub(crate) log: PathBuf,
    /// Where hyperfine writes its JSON export.
    pub(crate) export: PathBuf,
}

/// Measures `job`.
///
/// hyperfine has no time limit for a run. So the binary first runs once on
/// its own under the limit, with its output in `job.log`. When that run times
/// out, hyperfine does not start. Otherwise the whole hyperfine series gets
/// [`series_limit`], and hyperfine is killed, with its child processes, when
/// it takes longer.
pub(crate) fn measure(job: &Job<'_>) -> Result<Measurement> {
    let log =
        File::create(&job.log).with_context(|| format!("cannot create {}", job.log.display()))?;
    let mut first = Command::new(job.program);
    first
        .args(job.args)
        .current_dir(job.cwd)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    match process::run(&mut first, job.timeout)? {
        Finished::TimedOut => return Ok(Measurement::TimedOut),
        Finished::Exited(status) => {
            if !matches!(status.code(), Some(0 | FOUND_PROBLEMS)) {
                return Ok(Measurement::Failed {
                    reason: format!(
                        "the first run exited with {status}, see {}",
                        job.log.display()
                    ),
                });
            }
        }
    }

    match fs::remove_file(&job.export) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(error).with_context(|| format!("cannot remove {}", job.export.display()));
        }
        _ => {}
    }
    let mut hyperfine = Command::new(job.hyperfine);
    hyperfine
        .args(["--shell=none", "--style=basic"])
        .arg(format!("--warmup={}", job.warmup))
        .arg(format!("--runs={}", job.runs))
        .arg(format!("--ignore-failure={FOUND_PROBLEMS}"))
        .arg("--metrics=time_wall_clock,memory_peak_resident")
        .arg("--export-json")
        .arg(&job.export)
        .arg(command_line(job.program, job.args))
        .current_dir(job.cwd)
        .stdin(Stdio::null());
    let limit = series_limit(job.timeout, job.warmup, job.runs);
    match process::run(&mut hyperfine, limit)? {
        Finished::TimedOut => Ok(Measurement::TimedOut),
        Finished::Exited(status) if !status.success() => Ok(Measurement::Failed {
            reason: format!("hyperfine exited with {status}"),
        }),
        Finished::Exited(_) => {
            let text = fs::read_to_string(&job.export)
                .with_context(|| format!("cannot read {}", job.export.display()))?;
            parse_export(&text).with_context(|| {
                format!("cannot read the hyperfine export {}", job.export.display())
            })
        }
    }
}

/// The time limit for the whole hyperfine series: `timeout` once for each
/// warmup and measured run. One slow run can use the time that the others
/// leave unused.
pub(crate) fn series_limit(timeout: Duration, warmup: u32, runs: u32) -> Duration {
    timeout * (warmup + runs).max(1)
}

/// Joins `program` and `args` into one command line that hyperfine splits
/// back with shell quoting rules. hyperfine runs it without a shell, so it
/// expands no globs: the corpus goes in as a list of files.
pub(crate) fn command_line(program: &Path, args: &[String]) -> String {
    let program = program.to_string_lossy();
    std::iter::once(program.as_ref())
        .chain(args.iter().map(String::as_str))
        .map(quote)
        .collect::<Vec<_>>()
        .join(" ")
}

fn quote(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_./=:,+@%".contains(&byte));
    if plain {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

#[derive(Debug, Deserialize)]
struct SchemaVersion {
    schema_version: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct Export {
    results: Vec<ExportResult>,
}

#[derive(Debug, Deserialize)]
struct ExportResult {
    summary: Summary,
}

#[derive(Debug, Deserialize)]
struct Summary {
    time_wall_clock: Metric,
    memory_peak_resident: Option<Metric>,
}

#[derive(Debug, Deserialize)]
struct Metric {
    count: u64,
    mean: f64,
    stddev: Option<f64>,
    min: f64,
    max: f64,
}

/// Reads the measurement of the one command in a hyperfine JSON export with
/// schema version 2.
pub(crate) fn parse_export(text: &str) -> Result<Measurement> {
    let SchemaVersion { schema_version } = serde_json::from_str(text)?;
    match schema_version {
        Some(2) => {}
        Some(version) => {
            bail!("the export has schema version {version}, but the command reads version 2");
        }
        None => bail!("the export has no schema version, so hyperfine 1 wrote it"),
    }
    let export: Export = serde_json::from_str(text)?;
    let [result] = <[ExportResult; 1]>::try_from(export.results)
        .map_err(|results| anyhow::anyhow!("expected one result, found {}", results.len()))?;
    let wall = result.summary.time_wall_clock;
    let peak_memory_bytes = result
        .summary
        .memory_peak_resident
        .map(|memory| float_to_bytes(memory.max));
    Ok(Measurement::Completed {
        wall_time: Stats {
            runs: wall.count,
            mean: wall.mean,
            stddev: wall.stddev,
            min: wall.min,
            max: wall.max,
        },
        peak_memory_bytes,
    })
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn float_to_bytes(value: f64) -> u64 {
    value.max(0.0).round() as u64
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A hyperfine 2.0.0 export of two runs, cut down to the fields the
    /// command reads and a few it ignores.
    pub(crate) const EXPORT: &str = r#"{
      "schema_version": 2,
      "metadata": { "hyperfine_version": "2.0.0" },
      "primary_metric": "time_wall_clock",
      "results": [{
        "command": "actionlint small.yaml",
        "measurements": [],
        "summary": {
          "memory_peak_resident": {
            "unit": "byte", "count": 2, "mean": 1925120.0, "stddev": 11585.2,
            "median": 1925120.0, "min": 1916928.0, "max": 1933312.0
          },
          "time_wall_clock": {
            "unit": "second", "count": 2, "mean": 0.0111, "stddev": 0.0002,
            "median": 0.0111, "min": 0.0110, "max": 0.0112
          }
        }
      }]
    }"#;

    #[test]
    fn export_gives_wall_time_and_peak_memory() {
        let measurement = parse_export(EXPORT).unwrap();
        assert_eq!(
            measurement,
            Measurement::Completed {
                wall_time: Stats {
                    runs: 2,
                    mean: 0.0111,
                    stddev: Some(0.0002),
                    min: 0.0110,
                    max: 0.0112,
                },
                peak_memory_bytes: Some(1_933_312),
            }
        );
    }

    #[test]
    fn export_of_hyperfine_1_is_rejected() {
        let error = parse_export(r#"{"results": [{"mean": 1.0}]}"#).unwrap_err();
        assert!(error.to_string().contains("hyperfine 1"), "{error:#}");
        let error = parse_export(&EXPORT.replace("\"schema_version\": 2", "\"schema_version\": 3"))
            .unwrap_err();
        assert!(error.to_string().contains("schema version 3"), "{error:#}");
    }

    #[test]
    fn single_run_has_no_standard_deviation() {
        let export = EXPORT.replace("\"stddev\": 0.0002", "\"stddev\": null");
        let Measurement::Completed { wall_time, .. } = parse_export(&export).unwrap() else {
            panic!("expected a completed measurement");
        };
        assert_eq!(wall_time.stddev, None);
    }

    #[test]
    fn command_line_quotes_only_words_that_need_it() {
        let args = [
            "-shellcheck=".to_owned(),
            "a b.yml".to_owned(),
            "it's.yml".to_owned(),
            String::new(),
        ];
        assert_eq!(
            command_line(Path::new("/opt/bin/actionlint"), &args),
            r"/opt/bin/actionlint -shellcheck= 'a b.yml' 'it'\''s.yml' ''"
        );
    }
}
