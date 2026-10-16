//! `cargo xtask oracle`: runs workflint and the reference binary on the same
//! inputs and reports each difference that the allow-list does not explain
//! (ADR 0004).
//!
//! The inputs are the vendored `err`, `examples`, `ok`, and `projects`
//! fixtures and the corpus from the vendored archive. Each fixture runs with
//! the conformance runner's command line, external tools off. The corpus is
//! one run over all its files. [`compare`] compares the outputs of a run and
//! [`report`] writes the result. With `--kind`, [`kinds`] limits the
//! comparison to some actionlint kinds.

mod compare;
mod kinds;
mod report;
mod select;

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use xtask::conformance::allow_list::AllowList;
use xtask::conformance::binary;
use xtask::conformance::fixture::{self, Fixture, Origin, Tools};

use crate::bench;
use crate::reference::{self, CORPUS_FILES, Layout, corpus};
use crate::vendor_testdata;
use compare::{Allowed, Output};
use kinds::Kinds;
use report::Report;
use select::{Corpus, Places};

/// The flags of every run, before `-config-file`.
const FLAGS: [&str; 4] = ["-oneline", "-no-color", "-shellcheck=", "-pyflakes="];

/// The fixtures on which the reference's output varies between runs
/// (`docs/differences.md#cached-metadata-errors`). Every run skips them, and
/// the report lists them.
const VARYING: [&str; 3] = [
    "projects/broken_local_action",
    "projects/broken_reusable_workflow",
    "projects/workflow_call_not_found",
];

/// What the path arguments and flags ask for.
#[derive(Debug, Default)]
pub(crate) struct Options {
    /// Files or directories that limit the run. Empty means every input.
    pub(crate) paths: Vec<PathBuf>,
    /// Exit with success even if a difference is not explained.
    pub(crate) report_only: bool,
    /// Run the reference in place of workflint, without the kind mapping or
    /// the allow-list, to show that the runs are deterministic.
    pub(crate) self_check: bool,
    /// The actionlint kinds to compare. Empty means every kind.
    pub(crate) kinds: Vec<String>,
}

/// Builds both binaries, compares them on the selected inputs, prints the
/// report, and writes it to `target/oracle/report.md` with the summary in
/// `target/oracle/summary.md`.
///
/// # Errors
///
/// Fails if a step cannot run, or, without `report_only`, if a difference is
/// not explained.
pub(crate) fn run(layout: &Layout, options: &Options) -> Result<()> {
    let kinds = Kinds::new(&options.kinds)?;
    let root = layout.root();
    let archive = root
        .join(vendor_testdata::DESTINATION)
        .join("testdata")
        .join("realworld")
        .join("dataset.zip");
    let paths = options
        .paths
        .iter()
        .map(|path| resolve(root, path))
        .collect::<Result<Vec<_>>>()?;
    let corpus_dir = layout.corpus();
    let selection = select::select(
        Places {
            fixtures: fixture::discover(Origin::Actionlint).map_err(anyhow::Error::msg)?,
            archive: &archive,
            corpus: &corpus_dir,
        },
        &paths,
    )?;
    let (fixtures, skipped) = without_varying(selection.fixtures);

    reference::report_checkout(layout)?;
    reference::report_binary(layout)?;
    let reference = layout.binary();
    let workflint = if options.self_check {
        reference.clone()
    } else {
        binary::workflint().to_path_buf()
    };
    let allow_list = if options.self_check {
        AllowList::default()
    } else {
        read_allow_list(layout)?
    };
    let sides = Sides {
        reference: &reference,
        workflint: &workflint,
        map_kinds: !options.self_check,
        kinds: &kinds,
    };

    let mut runs = Vec::new();
    for fixture in &fixtures {
        runs.push(fixture_run(layout, fixture, &sides, &allow_list)?);
    }
    let corpus_files = match &selection.corpus {
        Corpus::None => 0,
        selected => {
            let (run, files) = corpus_run(&archive, selected, &sides)?;
            runs.push(run);
            files
        }
    };

    let report = Report {
        description: description(options.self_check, fixtures.len(), corpus_files),
        self_check: options.self_check,
        kinds,
        skipped,
        runs,
    };
    let directory = bench::target_directory(layout)?.join("oracle");
    fs::create_dir_all(&directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;
    let rendered = report.render();
    for (name, text) in [("report.md", &rendered), ("summary.md", &report.summary())] {
        let path = directory.join(name);
        fs::write(&path, text).with_context(|| format!("cannot write {}", path.display()))?;
    }
    print!("{rendered}");

    if !report.totals().clean() && !options.report_only {
        bail!(
            "{}; see {}",
            failure(options.self_check),
            layout.display(&directory.join("report.md"))
        );
    }
    Ok(())
}

/// The fixtures that are not in [`VARYING`], and the IDs of those that are.
/// Every run skips the varying fixtures, with or without `--self-check`.
fn without_varying(fixtures: Vec<Fixture>) -> (Vec<Fixture>, Vec<String>) {
    let (varying, kept): (Vec<_>, Vec<_>) = fixtures
        .into_iter()
        .partition(|fixture| VARYING.contains(&fixture.id.as_str()));
    (
        kept,
        varying.into_iter().map(|fixture| fixture.id).collect(),
    )
}

fn read_allow_list(layout: &Layout) -> Result<AllowList> {
    let path = layout
        .root()
        .join("tests")
        .join("conformance")
        .join("allowed-differences.toml");
    let text = fs::read_to_string(&path)
        .with_context(|| format!("cannot read {}", layout.display(&path)))?;
    AllowList::parse(&text).map_err(|error| anyhow::anyhow!("{}: {error}", layout.display(&path)))
}

/// The error when the report is not clean.
fn failure(self_check: bool) -> &'static str {
    if self_check {
        "two runs of the reference printed different output, so the oracle cannot compare it with workflint"
    } else {
        "the allow-list does not explain every difference"
    }
}

/// `path` as the selection sees it: under the repository root as `root`
/// spells it.
fn resolve(root: &Path, path: &Path) -> Result<PathBuf> {
    let canonical =
        fs::canonicalize(path).with_context(|| format!("cannot find {}", path.display()))?;
    let canonical_root = fs::canonicalize(root)
        .with_context(|| format!("cannot find the repository at {}", root.display()))?;
    match canonical.strip_prefix(&canonical_root) {
        Ok(relative) => Ok(root.join(relative)),
        Err(_) => bail!("{} is outside the repository", path.display()),
    }
}

/// The files at the end of a command line that [`FLAGS`] and
/// `-config-file <path>` start.
fn input_files(args: &[OsString]) -> Vec<String> {
    let start = args
        .iter()
        .position(|arg| arg == "-config-file")
        .map_or(args.len(), |index| index + 2);
    args.get(start..)
        .unwrap_or_default()
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

/// Runs `program` with `args` in `cwd`. The project directory in its output
/// becomes `<project>`, since each run has another temporary directory.
fn execute(program: &Path, cwd: &Path, args: &[OsString]) -> Result<Output> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("cannot run {}", program.display()))?;
    let mut projects = vec![cwd.to_string_lossy().into_owned()];
    if let Ok(canonical) = fs::canonicalize(cwd) {
        projects.insert(0, canonical.to_string_lossy().into_owned());
    }
    let clean = |bytes: &[u8]| {
        projects
            .iter()
            .fold(String::from_utf8_lossy(bytes).into_owned(), |text, path| {
                text.replace(path.as_str(), "<project>")
            })
    };
    Ok(Output {
        stdout: clean(&output.stdout),
        stderr: clean(&output.stderr),
        code: output.status.code(),
    })
}

/// The binaries that a run compares, and how it compares them.
struct Sides<'a> {
    reference: &'a Path,
    workflint: &'a Path,
    map_kinds: bool,
    kinds: &'a Kinds,
}

/// Runs both binaries on `fixture` with the conformance runner's command
/// line and compares them, with the fixture's entries in `allow_list`.
fn fixture_run(
    layout: &Layout,
    fixture: &Fixture,
    sides: &Sides<'_>,
    allow_list: &AllowList,
) -> Result<compare::Run> {
    let invocation = fixture
        .prepare(&Tools::default())
        .with_context(|| format!("cannot prepare {}", fixture.id))?;
    let inputs = input_files(&invocation.args);
    let expected = execute(sides.reference, &invocation.cwd, &invocation.args)?;
    let actual = execute(sides.workflint, &invocation.cwd, &invocation.args)?;
    let out_file = fixture
        .expected
        .as_deref()
        .map(|path| layout.display(path).to_string())
        .unwrap_or_default();
    let out_lines = fixture.expected_lines();
    let entries = allow_list.entries_for(&fixture.id);
    let allowed = Allowed {
        out_file: &out_file,
        out_lines: &out_lines,
        entries: &entries,
    };
    Ok(compare::compare(
        &fixture.id,
        &inputs,
        &expected,
        &actual,
        &allowed,
        sides.map_kinds,
        sides.kinds,
    ))
}

/// Unpacks the corpus into a temporary project and runs both binaries once
/// on the `selected` files, in sorted order. Returns the number of files
/// too.
fn corpus_run(
    archive: &Path,
    selected: &Corpus,
    sides: &Sides<'_>,
) -> Result<(compare::Run, usize)> {
    let temp = tempfile::Builder::new()
        .prefix("workflint-oracle-")
        .tempdir()
        .context("cannot create a temporary directory")?;
    let project = temp.path().join("project");
    let (_, count) = corpus::ensure(archive, &project)?;
    let mut names = Vec::new();
    for entry in
        fs::read_dir(&project).with_context(|| format!("cannot read {}", project.display()))?
    {
        let entry = entry.with_context(|| format!("cannot read {}", project.display()))?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    match selected {
        Corpus::All if count != CORPUS_FILES || names.len() != CORPUS_FILES => bail!(
            "{} holds {count} files, but {CORPUS_FILES} were expected",
            archive.display()
        ),
        Corpus::Names(wanted) => {
            if let Some(missing) = wanted.iter().find(|name| !names.contains(name)) {
                bail!("the corpus has no file named {missing}");
            }
            names.retain(|name| wanted.contains(name));
        }
        _ => {}
    }

    fs::create_dir_all(project.join(".git"))?;
    fs::create_dir_all(project.join(".github").join("workflows"))?;
    let config = temp.path().join("config.yaml");
    fs::write(&config, "").with_context(|| format!("cannot write {}", config.display()))?;
    let mut args: Vec<OsString> = FLAGS.iter().map(OsString::from).collect();
    args.push(OsString::from("-config-file"));
    args.push(config.into_os_string());
    args.extend(names.iter().map(OsString::from));

    let expected = execute(sides.reference, &project, &args)?;
    let actual = execute(sides.workflint, &project, &args)?;
    let run = compare::compare(
        "realworld",
        &names,
        &expected,
        &actual,
        &Allowed::default(),
        sides.map_kinds,
        sides.kinds,
    );
    Ok((run, names.len()))
}

fn description(self_check: bool, fixtures: usize, corpus: usize) -> String {
    let short = &reference::COMMIT[..7];
    let sides = if self_check {
        format!(
            "The reference at `{short}` against itself, without the kind mapping or the allow-list"
        )
    } else {
        format!("workflint against the reference at `{short}`")
    };
    let flags = FLAGS.join(" ");
    let count = |n: usize, one: &str, many: &str| match n {
        0 => None,
        1 => Some(format!("1 {one}")),
        n => Some(format!("{n} {many}")),
    };
    let inputs: Vec<String> = [
        count(fixtures, "fixture", "fixtures"),
        count(corpus, "corpus file", "corpus files"),
    ]
    .into_iter()
    .flatten()
    .collect();
    let inputs = if inputs.is_empty() {
        "no input".to_owned()
    } else {
        inputs.join(" and ")
    };
    format!("{sides}, run with `{flags}` and an empty config file on {inputs}.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fixture_command_line_starts_with_the_oracle_flags() {
        let fixture = fixture::discover(Origin::Actionlint)
            .unwrap()
            .into_iter()
            .find(|fixture| fixture.id == "projects/paths_config")
            .unwrap();
        let invocation = fixture.prepare(&Tools::default()).unwrap();
        assert_eq!(invocation.args[..4], FLAGS.map(OsString::from));
        assert_eq!(
            input_files(&invocation.args),
            [
                "workflows/bar.yaml",
                "workflows/foo.yaml",
                "workflows/nested/piyo.yaml"
            ]
        );
    }

    #[test]
    fn input_files_follow_the_config_file() {
        let args: Vec<OsString> = ["-oneline", "-config-file", "c.yaml", "a.yml", "b.yml"]
            .iter()
            .map(OsString::from)
            .collect();
        assert_eq!(input_files(&args), ["a.yml", "b.yml"]);
        assert_eq!(input_files(&args[..1]), Vec::<String>::new());
    }

    #[cfg(unix)]
    #[test]
    fn the_project_directory_in_the_output_becomes_a_placeholder() {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("print-cwd");
        fs::write(&script, "#!/bin/sh\npwd\npwd >&2\nexit 3\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let output = execute(&script, dir.path(), &[]).unwrap();
        assert_eq!(output.stdout, "<project>\n");
        assert_eq!(output.stderr, "<project>\n");
        assert_eq!(output.code, Some(3));
    }

    #[test]
    fn the_description_counts_the_inputs() {
        let text = description(false, 260, 1503);
        assert!(
            text.starts_with("workflint against the reference at `011a6d1`"),
            "{text}"
        );
        assert!(
            text.ends_with("on 260 fixtures and 1503 corpus files."),
            "{text}"
        );
        assert!(description(true, 1, 0).ends_with("on 1 fixture."));
        assert!(description(true, 0, 1).contains("against itself"));
        assert!(description(false, 0, 0).ends_with("on no input."));
    }

    #[test]
    fn the_varying_fixtures_are_skipped_and_named() {
        let all = fixture::discover(Origin::Actionlint).unwrap();
        let (kept, skipped) = without_varying(all.clone());
        assert_eq!(skipped, VARYING);
        assert_eq!(kept.len(), all.len() - VARYING.len());
        assert!(!kept.iter().any(|f| VARYING.contains(&f.id.as_str())));
    }

    #[test]
    fn the_self_check_failure_does_not_blame_the_allow_list() {
        let message = failure(true);
        assert!(!message.contains("allow-list"), "{message}");
        assert!(message.contains("two runs of the reference"), "{message}");
        assert!(failure(false).contains("allow-list"));
    }

    #[test]
    fn the_varying_fixtures_are_the_ones_the_differences_page_names() {
        let markdown =
            fs::read_to_string(Layout::repository().root().join("docs/differences.md")).unwrap();
        let section = markdown
            .split("<a id=\"cached-metadata-errors\"></a>")
            .nth(1)
            .and_then(|rest| rest.split("<a id=").next())
            .unwrap();
        let fixtures = section
            .lines()
            .find_map(|line| line.strip_prefix("- Fixtures: "))
            .unwrap();
        let named: Vec<&str> = fixtures
            .trim_end_matches('.')
            .split(", ")
            .map(|name| name.trim_matches('`'))
            .collect();
        assert_eq!(named, VARYING);
    }

    #[test]
    fn a_path_outside_the_repository_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let root = Layout::repository();
        let error = resolve(root.root(), dir.path()).unwrap_err();
        assert!(
            format!("{error:#}").contains("outside the repository"),
            "{error:#}"
        );
    }

    #[test]
    fn a_path_in_the_repository_keeps_the_spelling_of_the_root() {
        let layout = Layout::repository();
        let path = resolve(layout.root(), &layout.root().join("tests/conformance")).unwrap();
        assert_eq!(path, layout.root().join("tests/conformance"));
    }
}
