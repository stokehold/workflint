//! `cargo xtask bench`: compares workflint with the reference on the
//! scenarios of ADR 0019 and writes `target/bench/results.json` and
//! `target/bench/results.md`.
//!
//! `cargo xtask bench-history`, in [`history`], keeps the nightly history.

pub(crate) mod history;
mod hyperfine;
mod results;
mod scenario;
mod tools;

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use xtask::conformance::binary;

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
    let workflint = build_workflint(layout)?;
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
    remove_results(&output)?;
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
        event: github_event(),
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
    check_results(&results)
}

/// Fails when a measurement in `results` failed or a target was missed. The
/// command calls it after writing the results, so that
/// `cargo xtask bench-history` can record them, and the nightly job fails
/// after that (ADR 0039).
fn check_results(results: &Results) -> Result<()> {
    let failed = results
        .scenarios
        .iter()
        .filter_map(ScenarioResult::measurements)
        .flat_map(|(workflint, reference)| [workflint, reference])
        .filter(|measurement| matches!(measurement, Measurement::Failed { .. }))
        .count();
    let missed = history::missed_targets(results);
    let mut problems = Vec::new();
    if failed > 0 {
        problems.push(format!("{failed} measurements failed, see the results"));
    }
    if !missed.is_empty() {
        let list: Vec<String> = missed
            .iter()
            .map(|(scenario, target)| {
                format!(
                    "{scenario}: {}, measured {}",
                    target.target,
                    target.formatted_value()
                )
            })
            .collect();
        let count = match missed.len() {
            1 => "1 target".to_owned(),
            n => format!("{n} targets"),
        };
        problems.push(format!("{count} missed: {}", list.join("; ")));
    }
    if !problems.is_empty() {
        bail!("{}", problems.join(". "));
    }
    Ok(())
}

/// Removes the results of an earlier run from `output`, so that a run that
/// stops before writing its own leaves none behind.
fn remove_results(output: &Path) -> Result<()> {
    for name in ["results.json", "results.md"] {
        let path = output.join(name);
        match fs::remove_file(&path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                return Err(error).with_context(|| format!("cannot remove {}", path.display()));
            }
            _ => {}
        }
    }
    Ok(())
}

/// The GitHub Actions event that started the run, or `None` outside GitHub
/// Actions.
fn github_event() -> Option<String> {
    if env::var("GITHUB_ACTIONS").ok()? != "true" {
        return None;
    }
    env::var("GITHUB_EVENT_NAME")
        .ok()
        .filter(|event| !event.is_empty())
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
///
/// The binaries run in copies of the inputs, as the oracle runs them: each
/// in a project with `.git` and `.github/workflows` directories, with an
/// empty config file. So neither binary takes the workflint repository as
/// the project root, which would make it read the repository's config file
/// and resolve local actions there.
///
/// The corpus scenarios pass every corpus file on the command line, which
/// is about 78,000 characters long. Windows limits a command line to 32,767
/// characters, so they run only on Unix.
fn measure_scenarios(setup: &Setup<'_>) -> Result<Vec<ScenarioResult>> {
    let runs = setup.output.join("runs");
    fs::create_dir_all(&runs).with_context(|| format!("cannot create {}", runs.display()))?;
    let corpus = corpus_files(&setup.corpus)?;
    let projects = setup.output.join("projects");
    let corpus_project = copy_project(&projects.join("corpus"), &setup.corpus, &corpus)?;
    let bench_names: Vec<String> = SCENARIOS
        .iter()
        .filter_map(|scenario| match scenario.input {
            Input::BenchFile(name) => Some(name.to_owned()),
            Input::Corpus => None,
        })
        .collect();
    let bench_project = copy_project(&projects.join("bench"), &setup.bench_files, &bench_names)?;
    // `scenario::CONFIG_FILE` names this file from inside each project.
    let config = projects.join("config.yaml");
    fs::write(&config, "").with_context(|| format!("cannot write {}", config.display()))?;

    let mut measured = Vec::new();
    for scenario in &SCENARIOS {
        let (cwd, files) = match scenario.input {
            Input::Corpus => (corpus_project.clone(), corpus.clone()),
            Input::BenchFile(name) => (bench_project.clone(), vec![name.to_owned()]),
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

/// Makes `project` a fresh project with copies of `files` from `source`, and
/// `.git` and `.github/workflows` directories so that the binaries take it
/// as the project root. Returns `project`.
fn copy_project(project: &Path, source: &Path, files: &[String]) -> Result<PathBuf> {
    match fs::remove_dir_all(project) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(error).with_context(|| format!("cannot remove {}", project.display()));
        }
        _ => {}
    }
    for directory in [
        project.join(".git"),
        project.join(".github").join("workflows"),
    ] {
        fs::create_dir_all(&directory)
            .with_context(|| format!("cannot create {}", directory.display()))?;
    }
    for file in files {
        fs::copy(source.join(file), project.join(file))
            .with_context(|| format!("cannot copy {} into {}", file, project.display()))?;
    }
    Ok(project.to_path_buf())
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

/// Builds workflint in release mode and returns the path of the binary, as
/// cargo reports it. A `build.target` in the cargo config would put it
/// elsewhere than `target/release/`.
fn build_workflint(layout: &Layout) -> Result<PathBuf> {
    let output = Command::new(cargo())
        .args([
            "build",
            "--release",
            "--locked",
            "--package",
            "workflint",
            "--bin",
            "workflint",
            "--message-format=json-render-diagnostics",
        ])
        .current_dir(layout.root())
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .context("cannot run `cargo build`")?;
    if !output.status.success() {
        bail!(
            "`cargo build --release` of workflint failed with {}",
            output.status
        );
    }
    binary::executable(&String::from_utf8_lossy(&output.stdout))
        .context("`cargo build --release` reported no workflint executable")
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
                format!(
                    "{reference} -shellcheck= -pyflakes= -config-file ../config.yaml a.yml b.yaml"
                ),
                format!(
                    "{} -shellcheck= -pyflakes= -config-file ../config.yaml a.yml b.yaml",
                    fixture.path("workflint").display()
                ),
                format!(
                    "{reference} -shellcheck=/usr/bin/shellcheck -pyflakes= -config-file \
                     ../config.yaml a.yml b.yaml"
                ),
                format!(
                    "{} -shellcheck=/usr/bin/shellcheck -pyflakes= -config-file ../config.yaml \
                     a.yml b.yaml",
                    fixture.path("workflint").display()
                ),
                format!(
                    "{reference} -shellcheck= -pyflakes= -config-file ../config.yaml small.yaml"
                ),
                format!(
                    "{} -shellcheck= -pyflakes= -config-file ../config.yaml small.yaml",
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
    fn binaries_run_in_a_project_of_their_own_with_an_empty_config_file() {
        // Exits 1, as on the real inputs, only in a project with `.git`, the
        // input files, and an empty config file next to it.
        let isolated = "test -d .git && test -d .github/workflows && test -f ../config.yaml \
                        && test ! -s ../config.yaml && { test -f a.yml || test -f small.yaml; } \
                        || exit 3; exit 1";
        let fixture = Fixture::new(isolated, isolated);
        let scenarios = fixture.measure(&Shellcheck::Missing, Duration::from_secs(10));
        assert_eq!(
            statuses(&scenarios),
            [
                ("corpus", "completed", "completed"),
                ("corpus-shellcheck", "skipped", "skipped"),
                ("small", "completed", "completed"),
            ]
        );
        let corpus = fixture.path("out/projects/corpus");
        assert!(corpus.join("a.yml").is_file());
        assert!(!corpus.join("README.md").exists());
        assert!(fixture.path("out/projects/bench/small.yaml").is_file());
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
    fn failed_measurement_reaches_the_history_and_the_issue_before_the_command_fails() {
        let fixture = Fixture::new("exit 1", "exit 101");
        let mut results = results::tests::sample(0.1, 0.3);
        results.scenarios = fixture.measure(&Shellcheck::Missing, Duration::from_secs(10));
        let output = fixture.path("out");

        write_results(&results, &output).unwrap();
        let error = check_results(&results).unwrap_err();
        assert!(
            format!("{error:#}").contains("2 measurements failed"),
            "{error:#}"
        );

        let history = fixture.path("history.jsonl");
        let issue = fixture.path("issue.md");
        history::run(&history, &output.join("results.json"), Some(&issue)).unwrap();
        let entries = fs::read_to_string(&history).unwrap();
        assert_eq!(entries.lines().count(), 1);
        let entry: Results = serde_json::from_str(entries.trim()).unwrap();
        assert_eq!(entry, results);
        let body = fs::read_to_string(&issue).unwrap();
        assert!(body.contains("### Failed measurements"), "{body}");
        assert!(
            body.contains("- Corpus, external tools off, workflint: the first run exited with"),
            "{body}"
        );
    }

    #[test]
    fn missed_target_reaches_the_history_and_the_issue_before_the_command_fails() {
        let fixture = Fixture::new("exit 0", "exit 0");
        // Half of the reference's wall time misses the target of a third. The
        // sample is workflint 0.0.1, before 0.1.0.
        let results = results::tests::sample(0.5, 1.0);
        let output = fixture.path("out");

        write_results(&results, &output).unwrap();
        let error = check_results(&results).unwrap_err();
        assert_eq!(
            format!("{error:#}"),
            "1 target missed: Corpus, external tools off: wall time at most 1/3 of the \
             reference, measured 0.500"
        );

        let history = fixture.path("history.jsonl");
        let issue = fixture.path("issue.md");
        history::run(&history, &output.join("results.json"), Some(&issue)).unwrap();
        assert_eq!(fs::read_to_string(&history).unwrap().lines().count(), 1);
        let body = fs::read_to_string(&issue).unwrap();
        assert!(body.contains("### Missed targets"), "{body}");
        assert!(
            body.contains("- Corpus, external tools off: wall time at most 1/3 of the reference"),
            "{body}"
        );
    }

    #[test]
    fn results_that_meet_every_target_pass_the_check() {
        check_results(&results::tests::sample(0.1, 1.0)).unwrap();
    }

    #[test]
    fn results_of_an_earlier_run_are_removed_before_measuring() {
        let fixture = Fixture::new("exit 0", "exit 0");
        let output = fixture.path("out");
        fs::write(output.join("results.json"), "{}").unwrap();
        fs::write(output.join("results.md"), "old").unwrap();

        remove_results(&output).unwrap();

        assert!(!output.join("results.json").exists());
        assert!(!output.join("results.md").exists());
        remove_results(&output).unwrap();
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
