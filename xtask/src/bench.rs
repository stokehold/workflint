//! `cargo xtask bench`: compares workflint with the reference on the
//! scenarios of ADR 0019 and writes `target/bench/results.json` and
//! `target/bench/results.md`.
//!
//! `cargo xtask bench-history`, in [`history`], keeps the nightly history.

pub(crate) mod history;
mod hyperfine;
mod process;
mod results;
mod scenario;
mod tools;

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::reference::{self, Layout};
use crate::vendor_testdata;
use results::{Build, Measurement, Results, Runner, ScenarioOutcome, ScenarioResult};
use scenario::{Input, SCENARIOS, Scenario};
use tools::{Hyperfine, Shellcheck};

/// Runs `cargo xtask bench`. `timeout` replaces every scenario's time limit
/// for one run.
pub(crate) fn run(layout: &Layout, timeout: Option<Duration>) -> Result<()> {
    let date = results::format_utc(SystemTime::now());
    reference::run(layout)?;
    let go = reference::pinned_go_version(layout)?;
    let reference_version = first_line_of(&layout.binary(), "-version")?;

    let target = target_directory(layout)?;
    let workflint = build_workflint(layout, &target)?;
    println!("bench: built {}", layout.display(&workflint));

    let hyperfine = tools::hyperfine()?;
    println!(
        "bench: hyperfine {} at {}",
        hyperfine.version,
        hyperfine.path.display()
    );
    let shellcheck = tools::shellcheck()?;
    match shellcheck.skip_reason() {
        None => println!(
            "bench: shellcheck {} at {}",
            tools::SHELLCHECK_VERSION,
            shellcheck.pinned().unwrap_or(Path::new("")).display()
        ),
        Some(reason) => println!("bench: skipping the shellcheck scenario: {reason}"),
    }

    let output = target.join("bench");
    let setup = Setup {
        workflint,
        reference: layout.binary(),
        hyperfine: &hyperfine,
        shellcheck: &shellcheck,
        corpus: layout.corpus(),
        bench_files: layout
            .root()
            .join(vendor_testdata::DESTINATION)
            .join("testdata")
            .join("bench"),
        output: output.clone(),
        timeout,
    };
    let scenarios = measure_scenarios(&setup)?;

    let results = Results {
        schema_version: results::SCHEMA_VERSION,
        date,
        workflint: Build {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            commit: git_commit(layout.root()),
        },
        reference: Build {
            version: reference_version,
            commit: Some(reference::COMMIT.to_owned()),
        },
        tools: results::Tools {
            go,
            hyperfine: hyperfine.version.clone(),
            shellcheck: shellcheck.version().map(str::to_owned),
        },
        runner: runner(),
        scenarios,
    };
    write_results(&results, &output)?;
    println!(
        "bench: wrote {} and {}",
        layout.display(&output.join("results.json")),
        layout.display(&output.join("results.md"))
    );

    let failed = results
        .scenarios
        .iter()
        .filter_map(ScenarioResult::measurements)
        .flat_map(|(workflint, reference)| [workflint, reference])
        .filter(|measurement| matches!(measurement, Measurement::Failed { .. }))
        .count();
    if failed > 0 {
        bail!("{failed} measurements failed, see the results");
    }
    Ok(())
}

/// What [`measure_scenarios`] needs.
#[derive(Debug)]
struct Setup<'a> {
    workflint: PathBuf,
    reference: PathBuf,
    hyperfine: &'a Hyperfine,
    shellcheck: &'a Shellcheck,
    /// The directory with the corpus files.
    corpus: PathBuf,
    /// The vendored `testdata/bench/` directory.
    bench_files: PathBuf,
    /// Where the logs and hyperfine exports go.
    output: PathBuf,
    /// Replaces every scenario's time limit, if set.
    timeout: Option<Duration>,
}

/// Runs every scenario for the reference and then for workflint. A scenario
/// that needs the pinned shellcheck is skipped without it.
fn measure_scenarios(setup: &Setup<'_>) -> Result<Vec<ScenarioResult>> {
    let runs = setup.output.join("runs");
    fs::create_dir_all(&runs).with_context(|| format!("cannot create {}", runs.display()))?;
    let corpus = corpus_files(&setup.corpus)?;

    let mut measured = Vec::new();
    for scenario in &SCENARIOS {
        let (cwd, files) = match scenario.input {
            Input::Corpus => (setup.corpus.clone(), corpus.clone()),
            Input::BenchFile(name) => (setup.bench_files.clone(), vec![name.to_owned()]),
        };
        let flags = scenario.flags(setup.shellcheck.pinned());
        let timeout = setup.timeout.unwrap_or(scenario.timeout);
        let mut result = ScenarioResult {
            id: scenario.id.to_owned(),
            title: scenario.title.to_owned(),
            files: files.len(),
            flags: flags.clone(),
            warmup: scenario.warmup,
            runs: scenario.runs,
            timeout_seconds: timeout.as_secs_f64(),
            outcome: ScenarioOutcome::Skipped {
                reason: String::new(),
            },
        };
        if let (true, Some(reason)) = (scenario.shellcheck, setup.shellcheck.skip_reason()) {
            println!("bench: {}: skipped: {reason}", scenario.id);
            result.outcome = ScenarioOutcome::Skipped { reason };
            measured.push(result);
            continue;
        }

        let args: Vec<String> = flags.into_iter().chain(files).collect();
        let measure = |binary: &str, program: &Path| -> Result<Measurement> {
            println!("bench: {}: {binary}", scenario.id);
            let measurement = hyperfine::measure(&hyperfine::Job {
                hyperfine: &setup.hyperfine.path,
                program,
                args: &args,
                cwd: &cwd,
                warmup: scenario.warmup,
                runs: scenario.runs,
                timeout,
                log: runs.join(format!("{}-{binary}.log", scenario.id)),
                export: runs.join(format!("{}-{binary}.json", scenario.id)),
            })?;
            report(scenario, binary, &measurement, timeout);
            Ok(measurement)
        };
        let reference = measure("reference", &setup.reference)?;
        let workflint = measure("workflint", &setup.workflint)?;
        let targets = scenario
            .targets
            .iter()
            .map(|target| target.evaluate(&workflint, &reference))
            .collect();
        result.outcome = ScenarioOutcome::Measured {
            workflint,
            reference,
            targets,
        };
        measured.push(result);
    }
    Ok(measured)
}

fn report(scenario: &Scenario, binary: &str, measurement: &Measurement, timeout: Duration) {
    match measurement {
        Measurement::Completed { wall_time, .. } => println!(
            "bench: {}: {binary}: mean {}",
            scenario.id,
            results::format_seconds(wall_time.mean)
        ),
        Measurement::TimedOut => println!(
            "bench: {}: {binary}: timed out after {} s, continuing",
            scenario.id,
            timeout.as_secs_f64()
        ),
        Measurement::Failed { reason } => {
            println!("bench: {}: {binary}: failed: {reason}", scenario.id);
        }
    }
}

/// The names of the workflow files in `corpus`, sorted.
fn corpus_files(corpus: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();
    for entry in
        fs::read_dir(corpus).with_context(|| format!("cannot read {}", corpus.display()))?
    {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let workflow = Path::new(&name)
            .extension()
            .is_some_and(|extension| extension == "yml" || extension == "yaml");
        if workflow && entry.file_type()?.is_file() {
            files.push(name);
        }
    }
    files.sort();
    if files.is_empty() {
        bail!("{} has no workflow files", corpus.display());
    }
    Ok(files)
}

/// Writes `results.json` and `results.md` into `output`.
fn write_results(results: &Results, output: &Path) -> Result<()> {
    fs::create_dir_all(output).with_context(|| format!("cannot create {}", output.display()))?;
    let json = output.join("results.json");
    let mut text = serde_json::to_string_pretty(results)?;
    text.push('\n');
    fs::write(&json, text).with_context(|| format!("cannot write {}", json.display()))?;
    let markdown = output.join("results.md");
    fs::write(&markdown, results::render_markdown(results))
        .with_context(|| format!("cannot write {}", markdown.display()))
}

fn cargo() -> OsString {
    env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

/// Cargo's target directory, which `CARGO_TARGET_DIR` or the configuration
/// may move.
pub(crate) fn target_directory(layout: &Layout) -> Result<PathBuf> {
    #[derive(Deserialize)]
    struct Metadata {
        target_directory: PathBuf,
    }
    let output = Command::new(cargo())
        .args(["metadata", "--format-version=1", "--no-deps", "--offline"])
        .current_dir(layout.root())
        .output()
        .context("cannot run `cargo metadata`")?;
    if !output.status.success() {
        bail!(
            "`cargo metadata` failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let metadata: Metadata =
        serde_json::from_slice(&output.stdout).context("cannot read `cargo metadata`")?;
    Ok(metadata.target_directory)
}

/// Builds workflint in release mode and returns the path of the binary.
fn build_workflint(layout: &Layout, target: &Path) -> Result<PathBuf> {
    let status = Command::new(cargo())
        .args(["build", "--release", "--locked", "--package", "workflint"])
        .current_dir(layout.root())
        .status()
        .context("cannot run `cargo build`")?;
    if !status.success() {
        bail!("`cargo build --release` of workflint failed with {status}");
    }
    Ok(target
        .join("release")
        .join(format!("workflint{}", env::consts::EXE_SUFFIX)))
}

/// The first line that `program argument` prints.
fn first_line_of(program: &Path, argument: &str) -> Result<String> {
    let output = Command::new(program)
        .arg(argument)
        .output()
        .with_context(|| format!("cannot run {}", program.display()))?;
    if !output.status.success() {
        bail!(
            "{} {argument} failed with {}",
            program.display(),
            output.status
        );
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text.lines().next().unwrap_or_default().trim().to_owned())
}

/// The output of `program args`, trimmed, or `None` if it fails.
fn try_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (output.status.success() && !text.is_empty()).then_some(text)
}

fn git_commit(root: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// The machine the benchmarks run on.
fn runner() -> Runner {
    let cpu = match env::consts::OS {
        "linux" => fs::read_to_string("/proc/cpuinfo").ok().and_then(|info| {
            info.lines()
                .find_map(|line| line.strip_prefix("model name"))
                .and_then(|rest| rest.split_once(':'))
                .map(|(_, model)| model.trim().to_owned())
        }),
        "macos" => try_output("sysctl", &["-n", "machdep.cpu.brand_string"]),
        _ => None,
    };
    let image = match (env::var("ImageOS"), env::var("ImageVersion")) {
        (Ok(os), Ok(version)) => Some(format!("{os} {version}")),
        _ => None,
    };
    Runner {
        os: env::consts::OS.to_owned(),
        arch: env::consts::ARCH.to_owned(),
        kernel: if cfg!(unix) {
            try_output("uname", &["-sr"])
        } else {
            None
        },
        cpu,
        logical_cpus: std::thread::available_parallelism().ok().map(usize::from),
        image,
    }
}

#[cfg(all(test, unix))]
pub(crate) mod test_support {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    /// Writes an executable `sh` script with `body` to `path`.
    pub(crate) fn write_script(path: &Path, body: &str) {
        fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use test_support::write_script;

    /// Long enough for a new script to start on a busy machine. macOS checks
    /// a new executable on its first run, which can take a moment.
    const SHORT_TIMEOUT: Duration = Duration::from_secs(2);

    /// A directory with fake binaries, a fake hyperfine, a corpus, and the
    /// bench files.
    struct Fixture {
        directory: tempfile::TempDir,
        hyperfine: Hyperfine,
    }

    impl Fixture {
        /// `reference` and `workflint` are the bodies of the fake binaries.
        fn new(reference: &str, workflint: &str) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path();
            write_script(&root.join("reference"), reference);
            write_script(&root.join("workflint"), workflint);
            fs::write(root.join("export.json"), hyperfine::tests::EXPORT).unwrap();
            // Records the command line, its last argument, and writes the
            // canned export to the path after `--export-json`.
            write_script(
                &root.join("hyperfine"),
                r#"dir=$(dirname "$0")
while [ $# -gt 0 ]; do
  if [ "$1" = --export-json ]; then shift; out=$1; fi
  last=$1
  shift
done
printf '%s\n' "$last" >> "$dir/commands.txt"
cat "$dir/export.json" > "$out""#,
            );
            for directory in ["corpus", "bench", "out"] {
                fs::create_dir(root.join(directory)).unwrap();
            }
            for file in ["b.yaml", "a.yml", "README.md"] {
                fs::write(root.join("corpus").join(file), "on: push\n").unwrap();
            }
            fs::write(root.join("bench").join("small.yaml"), "on: push\n").unwrap();
            let hyperfine = Hyperfine {
                path: root.join("hyperfine"),
                version: "2.0.0".to_owned(),
            };
            Self {
                directory,
                hyperfine,
            }
        }

        fn path(&self, name: &str) -> PathBuf {
            self.directory.path().join(name)
        }

        fn measure(&self, shellcheck: &Shellcheck, timeout: Duration) -> Vec<ScenarioResult> {
            measure_scenarios(&Setup {
                workflint: self.path("workflint"),
                reference: self.path("reference"),
                hyperfine: &self.hyperfine,
                shellcheck,
                corpus: self.path("corpus"),
                bench_files: self.path("bench"),
                output: self.path("out"),
                timeout: Some(timeout),
            })
            .unwrap()
        }
    }

    fn statuses(scenarios: &[ScenarioResult]) -> Vec<(&str, &str, &str)> {
        let name = |measurement: &Measurement| match measurement {
            Measurement::Completed { .. } => "completed",
            Measurement::TimedOut => "timed out",
            Measurement::Failed { .. } => "failed",
        };
        scenarios
            .iter()
            .map(|scenario| match &scenario.outcome {
                ScenarioOutcome::Measured {
                    workflint,
                    reference,
                    ..
                } => (scenario.id.as_str(), name(reference), name(workflint)),
                ScenarioOutcome::Skipped { .. } => (scenario.id.as_str(), "skipped", "skipped"),
            })
            .collect()
    }

    #[test]
    fn with_shellcheck_every_scenario_runs_for_both_binaries() {
        let fixture = Fixture::new("exit 1", "exit 0");
        let shellcheck = Shellcheck::Pinned(PathBuf::from("/usr/bin/shellcheck"));
        let scenarios = fixture.measure(&shellcheck, Duration::from_secs(10));
        assert_eq!(
            statuses(&scenarios),
            [
                ("corpus", "completed", "completed"),
                ("corpus-shellcheck", "completed", "completed"),
                ("small", "completed", "completed"),
            ]
        );
        let commands = fs::read_to_string(fixture.path("commands.txt")).unwrap();
        let reference = fixture.path("reference");
        let reference = reference.display();
        assert_eq!(
            commands.lines().collect::<Vec<_>>(),
            [
                format!("{reference} -shellcheck= -pyflakes= a.yml b.yaml"),
                format!(
                    "{} -shellcheck= -pyflakes= a.yml b.yaml",
                    fixture.path("workflint").display()
                ),
                format!("{reference} -shellcheck=/usr/bin/shellcheck -pyflakes= a.yml b.yaml"),
                format!(
                    "{} -shellcheck=/usr/bin/shellcheck -pyflakes= a.yml b.yaml",
                    fixture.path("workflint").display()
                ),
                format!("{reference} -shellcheck= -pyflakes= small.yaml"),
                format!(
                    "{} -shellcheck= -pyflakes= small.yaml",
                    fixture.path("workflint").display()
                ),
            ]
        );
    }

    #[test]
    fn without_shellcheck_the_shellcheck_scenario_is_skipped_and_the_others_run() {
        let fixture = Fixture::new("exit 1", "exit 0");
        let scenarios = fixture.measure(&Shellcheck::Missing, Duration::from_secs(10));
        assert_eq!(
            statuses(&scenarios),
            [
                ("corpus", "completed", "completed"),
                ("corpus-shellcheck", "skipped", "skipped"),
                ("small", "completed", "completed"),
            ]
        );
        let ScenarioOutcome::Skipped { reason } = &scenarios[1].outcome else {
            panic!("expected a skipped scenario");
        };
        assert!(reason.contains("not installed"), "{reason}");
    }

    #[test]
    fn reference_run_past_the_timeout_is_recorded_as_timed_out_and_the_rest_runs() {
        let fixture = Fixture::new("sleep 30", "exit 0");
        let scenarios = fixture.measure(&Shellcheck::Missing, SHORT_TIMEOUT);
        assert_eq!(
            statuses(&scenarios),
            [
                ("corpus", "timed out", "completed"),
                ("corpus-shellcheck", "skipped", "skipped"),
                ("small", "timed out", "completed"),
            ]
        );
        // hyperfine never started for the reference.
        let commands = fs::read_to_string(fixture.path("commands.txt")).unwrap();
        assert!(!commands.contains("reference"), "{commands}");
    }

    #[test]
    fn crash_of_a_binary_is_recorded_as_failed() {
        let fixture = Fixture::new("exit 1", "exit 101");
        let scenarios = fixture.measure(&Shellcheck::Missing, Duration::from_secs(10));
        assert_eq!(statuses(&scenarios)[0], ("corpus", "completed", "failed"));
    }

    #[test]
    fn results_are_written_as_json_and_markdown() {
        let fixture = Fixture::new("sleep 30", "exit 0");
        let scenarios = fixture.measure(&Shellcheck::Missing, SHORT_TIMEOUT);
        let mut results = results::tests::sample(0.1, 0.3);
        results.scenarios = scenarios;
        write_results(&results, &fixture.path("out")).unwrap();

        let json = fs::read_to_string(fixture.path("out/results.json")).unwrap();
        assert_eq!(serde_json::from_str::<Results>(&json).unwrap(), results);
        let markdown = fs::read_to_string(fixture.path("out/results.md")).unwrap();
        assert!(
            markdown.contains("| reference | timed out after 2 s"),
            "{markdown}"
        );
        assert!(
            markdown
                .contains("| Corpus, shellcheck on | both | skipped: shellcheck is not installed"),
            "{markdown}"
        );
    }

    #[test]
    fn corpus_files_are_the_sorted_workflow_files() {
        let fixture = Fixture::new("exit 0", "exit 0");
        assert_eq!(
            corpus_files(&fixture.path("corpus")).unwrap(),
            ["a.yml", "b.yaml"]
        );
    }
}
