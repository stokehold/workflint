//! Runs one fixture through the `workflint` binary and judges the result.

use std::fs;
use std::sync::LazyLock;

use super::allow_list::{self, AllowList};
use super::expected_failures::ExpectedFailures;
use super::fixture::{self, Fixture, Tools};
use super::rule_kinds;
use super::verdict::{Verdict, judge};
use super::{binary, conformance_dir, output, repository_root};

/// How a fixture's test ends when it does not fail.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The fixture ran, and its output was compared.
    Judged(Verdict),
    /// A tool that the fixture needs is not installed.
    Skipped(String),
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

static INSTALLED: LazyLock<Tools> = LazyLock::new(Tools::installed);

/// Lints `fixture` with workflint and judges the output.
///
/// # Errors
///
/// Returns the reason the fixture's test fails. See [`judge`].
///
/// # Panics
///
/// Panics if the binary does not build or a data file is missing or invalid.
pub fn run_fixture(fixture: &Fixture) -> Result<Outcome, String> {
    let tools = match fixture.tools(&INSTALLED) {
        Ok(tools) => tools,
        Err(skip) => return Ok(Outcome::Skipped(skip)),
    };
    let invocation = fixture
        .prepare(&tools)
        .map_err(|error| format!("preparing {}: {error}", fixture.id))?;
    let actual: Result<Vec<_>, String> = binary::run(binary::workflint(), &invocation)
        .and_then(|stdout| output::parse_output(&stdout))
        .and_then(rule_kinds::to_kinds);
    let entries = ALLOW_LIST.entries_for(&fixture.id);
    judge(
        &fixture.id,
        &fixture.expected_lines(),
        actual.as_deref().map_err(String::as_str),
        &entries,
        EXPECTED_FAILURES.milestone(&fixture.id),
    )
    .map(Outcome::Judged)
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
    ALLOW_LIST.validate(&anchors, &fixture::all_ids())?;
    let fixtures = fixture::discover(fixture::Origin::Actionlint)
        .into_iter()
        .chain(fixture::discover(fixture::Origin::Workflint));
    let problems: Vec<String> = fixtures
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

/// Checks that every fixture listed in `expected-failures.toml` exists.
///
/// # Errors
///
/// Returns one line per unknown fixture.
pub fn check_expected_failures() -> Result<(), String> {
    EXPECTED_FAILURES.validate(&fixture::all_ids())
}

/// Checks that `rule-kinds.toml` has exactly the rule IDs and kinds of the
/// "Rule ID (kind)" column of `docs/parity-checklist.md`.
///
/// # Errors
///
/// Returns one line per rule ID that differs.
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
    let mut problems = Vec::new();
    for (rule, kind) in &checklist {
        match table.get(rule) {
            Some(found) if found == kind => {}
            Some(found) => problems.push(format!(
                "{rule}: rule-kinds.toml says {found}, the parity checklist says {kind}"
            )),
            None => problems.push(format!(
                "{rule} ({kind}) is in the parity checklist but not in rule-kinds.toml"
            )),
        }
    }
    for rule in table.keys().filter(|rule| !checklist.contains_key(*rule)) {
        problems.push(format!(
            "{rule} is in rule-kinds.toml but not in the parity checklist"
        ));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}
