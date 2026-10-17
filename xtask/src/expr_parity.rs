//! `cargo xtask expr-parity`: runs expressions through the reference's
//! expression checker and through `workflint-expr`, and reports each
//! difference that `tests/expr-parity/allowed-differences.toml` does not
//! explain.
//!
//! Each expression runs in each setup of `tests/expr-parity/setups.toml` that
//! names its expression set. Both sides parse it, and for an expression that
//! parses, check it and ask whether it is constant. [`compare`] compares the
//! parse error, or the type, the constant flag, and each error with its line,
//! column, and position in the list.

mod compare;
mod generate;
mod go_checker;
mod outcome;
mod setups;
mod types;

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use xtask::conformance::allow_list::anchors;

use crate::reference::{self, Layout, go};
use compare::{AllowList, Case};
use setups::Setup;

/// The inputs of the comparison, relative to the repository root.
const INPUTS: &str = "tests/expr-parity";

/// The reference's real-world expressions, relative to the repository root.
const REFERENCE_EXPRESSIONS: &str = "tests/conformance/actionlint/testdata/bench/expressions.txt";

/// Compares both checkers on every case, prints the report, and fails if a
/// difference is not explained or an allow-list record explains nothing.
pub(crate) fn run(layout: &Layout) -> Result<()> {
    let root = layout.root();
    let inputs = root.join(INPUTS);
    let setups = setups::parse(&read(&inputs.join("setups.toml"))?)?;
    let allow_list = AllowList::parse(&read(&inputs.join("allowed-differences.toml"))?)?;
    allow_list.validate(
        &anchors(&read(&root.join("docs").join("differences.md"))?),
        &setups,
    )?;
    let cases = cases(&setups, &|set| expression_set(root, set))?;

    reference::report_checkout(layout)?;
    let work = root.join("target").join("expr-parity");
    let binary = go_checker::binary_path(&work);
    go_checker::build(
        &OsString::from("go"),
        &layout.checkout(),
        &root.join("xtask").join("expr-check"),
        &work,
        &binary,
        &go::read_pinned_version(&layout.go_version_file())?,
    )?;
    let upstream = go_checker::run(&binary, &work, &setups, &cases)?;

    let envs: Vec<_> = setups.iter().map(Setup::type_env).collect();
    let report = compare::compare(
        &setups,
        &cases,
        &upstream,
        |setup, expression| outcome::check(&envs[setup], expression),
        &allow_list,
    )?;
    print!("{}", report.render());
    report.check()
}

fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))
}

/// The expressions of the set `name`, one of [`setups::EXPRESSION_SETS`].
fn expression_set(root: &Path, name: &str) -> Result<Vec<String>> {
    let inputs = root.join(INPUTS);
    let files = match name {
        "generated" => return Ok(generate::expressions()),
        "reference" => vec![root.join(REFERENCE_EXPRESSIONS)],
        "availability" => vec![inputs.join("availability.txt")],
        "cases" => case_files(&inputs.join("cases"))?,
        _ => bail!("unknown expression set {name:?}"),
    };
    let mut expressions = Vec::new();
    for file in files {
        let before = expressions.len();
        expressions.extend(lines(&read(&file)?));
        if expressions.len() == before {
            bail!("{} holds no expressions", file.display());
        }
    }
    Ok(expressions)
}

/// The `.txt` files in `dir`, sorted.
fn case_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("cannot read {}", dir.display()))? {
        let path = entry
            .with_context(|| format!("cannot read {}", dir.display()))?
            .path();
        if path.extension().is_some_and(|extension| extension == "txt") {
            files.push(path);
        }
    }
    if files.is_empty() {
        bail!("{} holds no .txt files", dir.display());
    }
    files.sort();
    Ok(files)
}

/// The expressions in a file: every line that is not empty and does not
/// start with `#`, as it is.
fn lines(text: &str) -> impl Iterator<Item = String> + '_ {
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
}

/// The cases of every setup, in setup order: the expressions of the sets it
/// names, in order, each once.
fn cases(
    setups: &[Setup],
    expression_set: &dyn Fn(&str) -> Result<Vec<String>>,
) -> Result<Vec<Case>> {
    let mut cases = Vec::new();
    for (index, setup) in setups.iter().enumerate() {
        let mut seen = BTreeSet::new();
        for set in &setup.expressions {
            for expression in expression_set(set)? {
                if seen.insert(expression.clone()) {
                    cases.push(Case {
                        setup: index,
                        expression,
                    });
                }
            }
        }
    }
    Ok(cases)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_and_blank_lines_are_not_expressions() {
        let found: Vec<_> = lines("# comment\n\nfoo\n  bar \n   \n#x\n").collect();
        assert_eq!(found, ["foo", "  bar "]);
    }

    #[test]
    fn each_setup_checks_each_expression_of_its_sets_once() {
        let setups = setups::parse(
            r#"
[[setup]]
name = "a"
key = "env"
expressions = ["cases", "availability"]

[[setup]]
name = "b"
key = "env"
expressions = ["availability"]
"#,
        )
        .unwrap();
        let cases = cases(&setups, &|set| {
            Ok(match set {
                "cases" => vec!["x".to_owned(), "y".to_owned()],
                _ => vec!["y".to_owned(), "z".to_owned()],
            })
        })
        .unwrap();
        let found: Vec<_> = cases
            .iter()
            .map(|case| (case.setup, case.expression.as_str()))
            .collect();
        assert_eq!(found, [(0, "x"), (0, "y"), (0, "z"), (1, "y"), (1, "z")]);
    }

    #[test]
    fn the_oracle_workflow_runs_the_comparison_when_its_inputs_change() {
        let path = Layout::repository()
            .root()
            .join(".github")
            .join("workflows")
            .join("oracle.yml");
        let workflow = read(&path).unwrap();
        let lines: Vec<&str> = workflow.lines().map(str::trim).collect();
        for expected in [
            "run: cargo xtask expr-parity",
            "- crates/**",
            "- xtask/**",
            "- tests/expr-parity/**",
            "- .go-version",
        ] {
            assert!(lines.contains(&expected), "oracle.yml lacks {expected:?}");
        }
    }

    #[test]
    fn the_committed_inputs_are_valid() {
        let layout = Layout::repository();
        let root = layout.root();
        let inputs = root.join(INPUTS);
        let setups = setups::parse(&read(&inputs.join("setups.toml")).unwrap()).unwrap();
        let allow_list =
            AllowList::parse(&read(&inputs.join("allowed-differences.toml")).unwrap()).unwrap();
        allow_list
            .validate(
                &anchors(&read(&root.join("docs").join("differences.md")).unwrap()),
                &setups,
            )
            .unwrap();
        for set in setups::EXPRESSION_SETS {
            assert!(!expression_set(root, set).unwrap().is_empty(), "{set}");
        }
        let cases = cases(&setups, &|set| expression_set(root, set)).unwrap();
        assert!(cases.len() > 10_000, "{} cases", cases.len());
    }
}
