//! Runs one fixture through a binary and judges the result.
//!
//! By default the binary is `workflint`, built from the workspace. With
//! [`REFERENCE_VARIABLE`] set, it is the reference binary at that path,
//! without the kind mapping, the expected failures or the allow-list, so that
//! a runner that passes fixtures it should fail shows up.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use super::allow_list::{self, AllowList};
use super::expected_failures::{self, ExpectedFailures};
use super::fixture::{self, Fixture, Tools};
use super::verdict::{Listing, Verdict, judge};
use super::{binary, conformance_dir, output, repository_root};
use super::{checklist, checks_md, rule_kinds};

/// The environment variable that names the reference binary to lint with in
/// place of workflint. A relative path is relative to the repository root.
pub const REFERENCE_VARIABLE: &str = "WORKFLINT_CONFORMANCE_REFERENCE";

/// The fixtures that the reference binary fails when the runner lints them,
/// each with the reason.
pub const REFERENCE_FAILURES: [(&str, &str); 1] = [(
    "projects/local_action_invalid",
    "its .out file expects the relative path \"testdata/projects/local_action_invalid/...\", \
     because the reference's Go test passes a relative project root, but the reference \
     binary finds the project root itself and prints an absolute path",
)];

/// How a fixture's test ends when it does not fail.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The fixture ran, and its output was compared.
    Judged(Verdict),
    /// The runner did not lint the fixture, for this reason.
    Skipped(String),
}

/// The binary that lints the fixtures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Program {
    /// `workflint`, built from the workspace before the first fixture.
    Workflint,
    /// The reference binary at this path.
    Reference(PathBuf),
}

impl Program {
    fn path(&self) -> &Path {
        match self {
            Self::Workflint => binary::workflint(),
            Self::Reference(path) => path,
        }
    }
}

/// What the runner lints with and how it judges the output.
#[derive(Debug)]
pub struct Setup {
    /// The binary that lints the fixtures.
    pub program: Program,
    /// Whether rule IDs in the output become actionlint kinds before the
    /// comparison. The reference prints kinds already.
    pub map_kinds: bool,
    /// The fixtures expected to fail, by ID.
    pub listings: BTreeMap<String, Listing>,
    /// The differences from the reference that the comparison accepts.
    pub allow_list: AllowList,
    /// The external tools on `PATH`.
    pub installed: Tools,
}

impl Setup {
    /// workflint, with the kind mapping, `expected-failures.toml`, and
    /// `allowed-differences.toml`.
    ///
    /// # Panics
    ///
    /// Panics if a data file is missing or invalid.
    #[must_use]
    pub fn workflint() -> Self {
        Self {
            program: Program::Workflint,
            map_kinds: true,
            listings: EXPECTED_FAILURES
                .fixtures
                .iter()
                .map(|(id, milestone)| (id.clone(), Listing::Until(milestone.clone())))
                .collect(),
            allow_list: AllowList {
                entries: ALLOW_LIST.entries.clone(),
            },
            installed: Tools::installed(),
        }
    }

    /// The reference binary at `binary`, without the kind mapping or the
    /// allow-list, and with [`REFERENCE_FAILURES`] as the only expected
    /// failures.
    #[must_use]
    pub fn reference(binary: PathBuf) -> Self {
        Self {
            program: Program::Reference(binary),
            map_kinds: false,
            listings: REFERENCE_FAILURES
                .iter()
                .map(|(id, reason)| ((*id).to_owned(), Listing::Reference((*reason).to_owned())))
                .collect(),
            allow_list: AllowList::default(),
            installed: Tools::installed(),
        }
    }

    /// [`Setup::reference`] if [`REFERENCE_VARIABLE`] is set and not empty,
    /// and [`Setup::workflint`] otherwise.
    #[must_use]
    pub fn from_environment() -> Self {
        match std::env::var_os(REFERENCE_VARIABLE) {
            Some(path) if !path.is_empty() => Self::reference(repository_root().join(path)),
            _ => Self::workflint(),
        }
    }

    /// Why the runner does not lint `fixture`, if it does not.
    #[must_use]
    pub fn skip_reason(&self, fixture: &Fixture) -> Option<String> {
        if let Some(tool) = fixture.missing_tool(&self.installed) {
            return Some(format!("{tool:?} is not installed"));
        }
        match (&self.program, fixture.id.starts_with("workflint/")) {
            (Program::Reference(_), true) => {
                Some("the reference does not lint workflint's own fixtures".to_owned())
            }
            _ => None,
        }
    }

    /// Lints `fixture` and judges the output.
    ///
    /// # Errors
    ///
    /// Returns the reason the fixture's test fails. See [`judge`].
    ///
    /// # Panics
    ///
    /// Panics if workflint does not build.
    pub fn run(&self, fixture: &Fixture) -> Result<Outcome, String> {
        if let Some(reason) = self.skip_reason(fixture) {
            return Ok(Outcome::Skipped(format!(
                "skipped {} because {reason}",
                fixture.id
            )));
        }
        let tools = fixture.tools(&self.installed)?;
        let invocation = fixture
            .prepare(&tools)
            .map_err(|error| format!("preparing {}: {error}", fixture.id))?;
        // A crash, a hang, another exit code or output on standard error
        // fails the fixture even when a list expects it to fail: the lists
        // excuse wrong diagnostics, not a broken binary.
        let stdout = binary::run(self.program.path(), &invocation).map_err(|reason| {
            match self.listings.get(&fixture.id) {
                Some(_) => format!(
                    "{reason}\nThe fixture is listed as an expected failure, but that covers only the diagnostics, not how the binary ran."
                ),
                None => reason,
            }
        })?;
        let actual: Result<Vec<_>, String> =
            output::parse_output(&stdout).and_then(|diagnostics| {
                if self.map_kinds {
                    rule_kinds::to_kinds(diagnostics)
                } else {
                    Ok(diagnostics)
                }
            });
        let entries = self.allow_list.entries_for(&fixture.id);
        judge(
            &fixture.id,
            &fixture.expected_lines(),
            actual.as_deref().map_err(String::as_str),
            &entries,
            self.listings.get(&fixture.id),
        )
        .map(Outcome::Judged)
    }

    /// Lints `input` as `test.yaml` in the default output format, as the
    /// reference did for the `Output` blocks of `docs/checks.md`, and
    /// compares the output with `output`, after `replacements`. See
    /// [`checks_md::compare`].
    ///
    /// # Errors
    ///
    /// Returns the reason the comparison fails.
    pub fn check_example(
        &self,
        input: &str,
        output: &str,
        replacements: &[(&str, &str)],
    ) -> Result<(), String> {
        let temp = tempfile::tempdir().map_err(|error| format!("creating a directory: {error}"))?;
        let suite = temp.path().join("err");
        let path = suite.join("example.yaml");
        fs::create_dir(&suite)
            .and_then(|()| fs::write(&path, input))
            .map_err(|error| format!("writing {}: {error}", path.display()))?;
        let mut invocation = Fixture::new(fixture::Origin::Workflint, &path)
            .prepare(&Tools::default())
            .map_err(|error| format!("preparing {}: {error}", path.display()))?;
        invocation.args.retain(|arg| arg != "-oneline");
        let stdout = binary::run(self.program.path(), &invocation)?;
        checks_md::compare(output, &stdout, replacements, |code| {
            if self.map_kinds {
                rule_kinds::actionlint_kind(code)
                    .map(str::to_owned)
                    .ok_or_else(|| {
                        format!("rule ID {code:?} has no actionlint kind in the rule registry or tests/conformance/rule-kinds.toml")
                    })
            } else {
                Ok(code.to_owned())
            }
        })
    }

    /// How many of `fixtures` the runner skips and why, with one line per
    /// skipped fixture.
    #[must_use]
    pub fn skip_summary(&self, fixtures: &[Fixture]) -> String {
        let mut by_reason: BTreeMap<String, Vec<&str>> = BTreeMap::new();
        for fixture in fixtures {
            if let Some(reason) = self.skip_reason(fixture) {
                by_reason.entry(reason).or_default().push(&fixture.id);
            }
        }
        let skipped: usize = by_reason.values().map(Vec::len).sum();
        let mut text = format!("skipped {skipped} of {} fixtures", fixtures.len());
        if by_reason.is_empty() {
            text.push('.');
        } else {
            let reasons: Vec<String> = by_reason
                .iter()
                .map(|(reason, ids)| format!("{} because {reason}", ids.len()))
                .collect();
            let _ = write!(text, ": {}.", reasons.join(", "));
        }
        for (tool, path) in [
            ("shellcheck", &self.installed.shellcheck),
            ("pyflakes", &self.installed.pyflakes),
        ] {
            let found = path
                .as_ref()
                .map_or_else(|| "not found".to_owned(), |path| path.display().to_string());
            let _ = write!(text, "\n{tool}: {found}");
        }
        for (reason, ids) in &by_reason {
            for id in ids {
                let _ = write!(text, "\n  {id}: {reason}");
            }
        }
        text
    }
}

fn read(path: &str) -> String {
    let path = repository_root().join(path);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("reading {}: {error}", path.display()))
}

static EXPECTED_FAILURES: LazyLock<ExpectedFailures> = LazyLock::new(|| {
    ExpectedFailures::parse(&read("tests/conformance/expected-failures.toml"))
        .unwrap_or_else(|error| panic!("tests/conformance/expected-failures.toml: {error}"))
});

static ALLOW_LIST: LazyLock<AllowList> = LazyLock::new(|| {
    AllowList::parse(&read("tests/conformance/allowed-differences.toml"))
        .unwrap_or_else(|error| panic!("tests/conformance/allowed-differences.toml: {error}"))
});

static SETUP: LazyLock<Setup> = LazyLock::new(Setup::from_environment);

/// Lints `fixture` with the binary that [`Setup::from_environment`] picks and
/// judges the output.
///
/// # Errors
///
/// Returns the reason the fixture's test fails. See [`judge`].
///
/// # Panics
///
/// Panics if the binary does not build or a data file is missing or invalid.
pub fn run_fixture(fixture: &Fixture) -> Result<Outcome, String> {
    SETUP.run(fixture)
}

/// How many fixtures the runner skips and why. See [`Setup::skip_summary`].
///
/// # Errors
///
/// Returns a message if a fixture directory cannot be read.
pub fn skip_summary() -> Result<String, String> {
    Ok(SETUP.skip_summary(&fixture::all()?))
}

/// Checks `allowed-differences.toml`: every anchor exists in
/// `docs/differences.md`, every fixture exists, and every `upstream` line is
/// in its fixture's `.out` file.
///
/// # Errors
///
/// Returns one line per problem.
pub fn check_allow_list() -> Result<(), String> {
    let anchors = allow_list::anchors(&read("docs/differences.md"));
    ALLOW_LIST.validate(&anchors, &fixture::all_ids()?)?;
    let problems: Vec<String> = fixture::all()?
        .into_iter()
        .filter_map(|fixture| {
            let entries = ALLOW_LIST.entries_for(&fixture.id);
            allow_list::apply(&fixture.expected_lines(), &entries).err()
        })
        .collect();
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

/// The milestones in `tests/conformance/milestones.toml`.
fn closed() -> Result<BTreeSet<u32>, String> {
    expected_failures::closed_milestones(&read("tests/conformance/milestones.toml"))
        .map_err(|error| format!("tests/conformance/milestones.toml: {error}"))
}

/// Checks that every fixture listed in `expected-failures.toml` and
/// [`REFERENCE_FAILURES`] exists, and that each entry names the milestone
/// that the parity checklist gives and that is not closed. See
/// [`ExpectedFailures::check_milestones`].
///
/// # Errors
///
/// Returns one line per problem.
pub fn check_expected_failures() -> Result<(), String> {
    let ids = fixture::all_ids()?;
    EXPECTED_FAILURES.validate(&ids)?;
    let items = checklist::items(&read("docs/parity-checklist.md"))?;
    EXPECTED_FAILURES.check_milestones(&items, &closed()?)?;
    let unknown: Vec<String> = REFERENCE_FAILURES
        .iter()
        .filter(|(id, _)| !ids.contains(*id))
        .map(|(id, _)| format!("REFERENCE_FAILURES lists {id:?}, which does not exist"))
        .collect();
    if unknown.is_empty() {
        Ok(())
    } else {
        Err(unknown.join("\n"))
    }
}

/// Checks every ticked row of the "docs/checks.md sections" table in
/// `docs/parity-checklist.md`: the binary's output for the section's
/// `Example input` matches its `Output` block. See [`Setup::check_example`].
///
/// # Errors
///
/// Returns one block per section that differs.
pub fn check_checks_md() -> Result<(), String> {
    let items = checklist::items(&read("docs/parity-checklist.md"))?;
    let sections = checks_md::sections(&read("tests/conformance/actionlint/docs/checks.md"));
    let mut problems = Vec::new();
    for item in items.iter().filter(|item| item.ticked) {
        let Some(anchor) = &item.anchor else {
            continue;
        };
        // The example of a section is the input of the row's fixtures, so
        // their allow-list records that replace a line apply to it.
        let replacements: Vec<(&str, &str)> = item
            .fixtures
            .iter()
            .flat_map(|fixture| ALLOW_LIST.entries_for(fixture))
            .filter_map(|entry| Some((entry.upstream.as_deref()?, entry.workflint.as_deref()?)))
            .collect();
        let result = match sections.get(anchor) {
            Some(checks_md::Section {
                input: Some(input),
                output: Some(output),
            }) => SETUP.check_example(input, output, &replacements),
            Some(_) => Err("the section has no Example input or no Output block".to_owned()),
            None => Err("docs/checks.md has no such section".to_owned()),
        };
        if let Err(reason) = result {
            problems.push(format!(
                "docs/parity-checklist.md:{}: checks.md#{anchor}: {reason}",
                item.line
            ));
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("\n\n"))
    }
}

/// Checks the boxes of `docs/parity-checklist.md` against the fixtures and
/// `expected-failures.toml`. See [`checklist::check_items`].
///
/// # Errors
///
/// Returns one line per problem.
pub fn check_parity_checklist() -> Result<(), String> {
    let items = checklist::items(&read("docs/parity-checklist.md"))?;
    let failing: BTreeSet<&str> = EXPECTED_FAILURES
        .fixtures
        .keys()
        .map(String::as_str)
        .collect();
    checklist::check_items(&items, &fixture::all_ids()?, &failing, &closed()?)
}

/// Checks that every rule ID of the "Rule ID (kind)" column of
/// `docs/parity-checklist.md` is either in the rule registry or in
/// `rule-kinds.toml`, with the checklist's kind (see
/// [`rule_kinds::check_rules`]).
///
/// # Errors
///
/// Returns one line per problem.
///
/// # Panics
///
/// Panics if a file cannot be read.
pub fn check_rule_kinds() -> Result<(), String> {
    let table_path = conformance_dir().join("rule-kinds.toml");
    let table = rule_kinds::parse_table(
        &fs::read_to_string(&table_path)
            .unwrap_or_else(|error| panic!("reading {}: {error}", table_path.display())),
    )?;
    let checklist = rule_kinds::checklist_kinds(&read("docs/parity-checklist.md"))?;
    let problems = rule_kinds::check_rules(&table, &checklist, workflint_rules::registry::rules());
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance::fixture::Origin;

    fn fixture(origin: Origin, path: &str) -> Fixture {
        Fixture::new(origin, &origin.root().join(path))
    }

    fn setup(program: PathBuf) -> Setup {
        Setup {
            program: Program::Reference(program),
            map_kinds: false,
            listings: BTreeMap::new(),
            allow_list: AllowList::default(),
            installed: Tools::default(),
        }
    }

    /// A shell script that runs `body`, standing in for a binary.
    #[cfg(unix)]
    fn fake_binary(dir: &Path, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt as _;

        let script = dir.join("fake");
        fs::write(&script, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        script
    }

    #[cfg(unix)]
    #[test]
    fn a_binary_that_prints_to_stderr_and_exits_1_fails_an_ok_fixture() {
        let dir = tempfile::tempdir().unwrap();
        let fake = fake_binary(dir.path(), "echo 'cannot lint' >&2\nexit 1");
        let ok = fixture(Origin::Actionlint, "ok/minimal.yaml");
        let error = setup(fake).run(&ok).unwrap_err();
        assert!(error.contains("printed none"), "{error}");
        assert!(
            error.contains("printed to standard error:\ncannot lint"),
            "{error}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_binary_that_exits_0_with_output_fails_an_ok_fixture() {
        let dir = tempfile::tempdir().unwrap();
        let fake = fake_binary(dir.path(), "echo 'Hello, world!'\nexit 0");
        let ok = fixture(Origin::Actionlint, "ok/minimal.yaml");
        let error = setup(fake).run(&ok).unwrap_err();
        assert!(error.contains("exited with code 0"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn a_binary_that_exits_with_another_code_fails_an_ok_fixture() {
        let dir = tempfile::tempdir().unwrap();
        let fake = fake_binary(dir.path(), "exit 2");
        let ok = fixture(Origin::Actionlint, "ok/minimal.yaml");
        let error = setup(fake).run(&ok).unwrap_err();
        assert!(error.contains("exited with code 2"), "{error}");
    }

    /// `setup` with `fixture` listed as an expected failure until M4.
    #[cfg(unix)]
    fn listing(mut setup: Setup, fixture: &str) -> Setup {
        setup
            .listings
            .insert(fixture.to_owned(), Listing::Until("M4".to_owned()));
        setup
    }

    #[cfg(unix)]
    #[test]
    fn a_crashing_binary_fails_a_listed_fixture() {
        let dir = tempfile::tempdir().unwrap();
        let fake = fake_binary(
            dir.path(),
            "echo 'thread main panicked at src/main.rs:1:1' >&2\nexit 101",
        );
        let listed = fixture(Origin::Actionlint, "err/one_error.yaml");
        let error = listing(setup(fake), "err/one_error")
            .run(&listed)
            .unwrap_err();
        assert!(error.contains("exited with code 101"), "{error}");
        assert!(error.contains("panicked"), "{error}");
        assert!(error.contains("listed as an expected failure"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn a_binary_killed_by_a_signal_fails_a_listed_fixture() {
        let dir = tempfile::tempdir().unwrap();
        let fake = fake_binary(dir.path(), "kill -SEGV $$");
        let listed = fixture(Origin::Actionlint, "err/one_error.yaml");
        let error = listing(setup(fake), "err/one_error")
            .run(&listed)
            .unwrap_err();
        assert!(error.contains("stopped by a signal"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn a_binary_that_prints_to_stderr_fails_a_listed_fixture() {
        let dir = tempfile::tempdir().unwrap();
        let fake = fake_binary(
            dir.path(),
            "echo 'test.yaml:1:1: other [syntax-check]'\necho 'warning' >&2\nexit 1",
        );
        let listed = fixture(Origin::Actionlint, "err/one_error.yaml");
        let error = listing(setup(fake), "err/one_error")
            .run(&listed)
            .unwrap_err();
        assert!(error.contains("printed to standard error"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn wrong_diagnostics_from_a_binary_that_ran_normally_are_an_expected_failure() {
        let dir = tempfile::tempdir().unwrap();
        let fake = fake_binary(
            dir.path(),
            "echo 'test.yaml:1:1: other [syntax-check]'\nexit 1",
        );
        let listed = fixture(Origin::Actionlint, "err/one_error.yaml");
        let outcome = listing(setup(fake), "err/one_error").run(&listed);
        assert!(
            matches!(
                outcome,
                Ok(Outcome::Judged(Verdict::ExpectedFailure { .. }))
            ),
            "{outcome:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_silent_binary_that_exits_0_passes_an_ok_fixture() {
        let dir = tempfile::tempdir().unwrap();
        let fake = fake_binary(dir.path(), "exit 0");
        let ok = fixture(Origin::Actionlint, "ok/minimal.yaml");
        assert_eq!(setup(fake).run(&ok), Ok(Outcome::Judged(Verdict::Passed)));
    }

    #[test]
    fn a_fixture_that_names_a_missing_tool_is_skipped() {
        let setup = setup(PathBuf::from("unused"));
        let fixture = fixture(Origin::Actionlint, "ok/pyflakes_x.yaml");
        assert_eq!(
            setup.run(&fixture),
            Ok(Outcome::Skipped(
                "skipped ok/pyflakes_x.yaml because \"pyflakes\" is not installed".into()
            ))
        );
    }

    #[test]
    fn the_reference_skips_workflints_own_fixtures() {
        let setup = Setup::reference(PathBuf::from("unused"));
        let own = fixture(Origin::Workflint, "ok/minimal.yaml");
        assert_eq!(
            setup.skip_reason(&own).as_deref(),
            Some("the reference does not lint workflint's own fixtures")
        );
        assert_eq!(
            setup.skip_reason(&fixture(Origin::Actionlint, "ok/a.yaml")),
            None
        );
    }

    #[test]
    fn the_reference_runs_without_kinds_allow_list_or_expected_failures() {
        let setup = Setup::reference(PathBuf::from("actionlint"));
        assert!(!setup.map_kinds);
        assert_eq!(setup.allow_list, AllowList::default());
        let listed: Vec<&str> = setup.listings.keys().map(String::as_str).collect();
        assert_eq!(listed, ["projects/local_action_invalid"]);
    }

    #[test]
    fn the_skip_summary_counts_skips_by_reason() {
        let mut setup = setup(PathBuf::from("unused"));
        setup.installed.shellcheck = Some(PathBuf::from("/bin/shellcheck"));
        let fixtures = [
            fixture(Origin::Actionlint, "err/shellcheck_a.yaml"),
            fixture(Origin::Actionlint, "err/pyflakes_a.yaml"),
            fixture(Origin::Actionlint, "ok/pyflakes_b.yaml"),
            fixture(Origin::Actionlint, "ok/plain.yaml"),
        ];
        let summary = setup.skip_summary(&fixtures);
        assert_eq!(
            summary,
            "skipped 2 of 4 fixtures: 2 because \"pyflakes\" is not installed.\n\
             shellcheck: /bin/shellcheck\n\
             pyflakes: not found\n  \
             err/pyflakes_a: \"pyflakes\" is not installed\n  \
             ok/pyflakes_b.yaml: \"pyflakes\" is not installed"
        );
    }

    #[test]
    fn the_skip_summary_says_when_nothing_is_skipped() {
        let summary = setup(PathBuf::from("unused"))
            .skip_summary(&[fixture(Origin::Actionlint, "ok/plain.yaml")]);
        assert!(
            summary.starts_with("skipped 0 of 1 fixtures.\n"),
            "{summary}"
        );
    }
}
