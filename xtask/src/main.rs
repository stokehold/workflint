//! Developer commands for the workflint repository. Run `cargo xtask --help`
//! for the list.

// ADR 0018 lets xtask panic: it runs only on developer machines and in CI,
// never on user input.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod bench;
mod expr_parity;
mod fuzz_seeds;
mod oracle;
mod reference;
mod vendor_testdata;
mod yaml_parity;

use std::path::PathBuf;
use std::time::Duration;

use clap::{CommandFactory, Parser, Subcommand};

/// Developer commands for the workflint repository.
#[derive(Debug, Parser)]
#[command(name = "xtask")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

/// Each subcommand lives in its own module in `xtask/src/`.
#[derive(Debug, Subcommand)]
enum Command {
    /// Compare workflint with the reference on the benchmark scenarios of ADR
    /// 0019 and write `target/bench/results.json` and `results.md`. Needs
    /// hyperfine 2 and the reference's tools, plus shellcheck 0.11.0 for the
    /// shellcheck scenario.
    Bench {
        /// Time limit in seconds for one run, instead of each scenario's own.
        #[arg(long, value_name = "SECONDS", value_parser = parse_seconds)]
        timeout: Option<Duration>,
    },
    /// Append a benchmark result to a history file and compare it with the
    /// latest scheduled run in it. The nightly benchmark job runs it.
    BenchHistory {
        /// The history file, with one result per line. It is created if
        /// missing.
        #[arg(long, value_name = "FILE")]
        history: PathBuf,
        /// The result to append [default: target/bench/results.json].
        #[arg(long, value_name = "FILE")]
        results: Option<PathBuf>,
        /// Where to write the body of a `performance` issue when a
        /// measurement failed, the result regressed, or it missed a target.
        /// Removed otherwise.
        #[arg(long, value_name = "FILE")]
        issue_body: Option<PathBuf>,
    },
    /// Run expressions through the reference's expression checker and
    /// through workflint-expr, and compare the parse error or the type, the
    /// constant flag, and each error with its position. The expressions and
    /// the setups they run in are in tests/expr-parity/. Needs git and the Go
    /// version in `.go-version`.
    ///
    /// It exits with an error if a difference is not explained by
    /// tests/expr-parity/allowed-differences.toml.
    ExprParity,
    /// Write seed inputs for the fuzz targets into `fuzz/corpus/<target>/`:
    /// the YAML files of the vendored test data, the corpus archive, the YAML
    /// test suite, and the fuzz regression inputs, or, for the expression
    /// targets, the reference's real-world expressions, the expressions in
    /// the vendored test data, and the expression fuzz regression inputs.
    FuzzSeeds,
    /// Run workflint and the reference binary on the vendored fixtures and
    /// the corpus, and report each difference that
    /// tests/conformance/allowed-differences.toml does not explain. Writes
    /// `target/oracle/report.md` and `summary.md`. Needs git and the Go
    /// version in `.go-version` to build the reference.
    ///
    /// It exits with an error if a difference is not explained.
    Oracle {
        /// Fixture files, project directories, directories of fixtures, the
        /// corpus archive, or files in `.reference/realworld`, to compare
        /// instead of every input.
        paths: Vec<PathBuf>,
        /// Exit with success after writing the report, even if a difference
        /// is not explained.
        #[arg(long)]
        report_only: bool,
        /// Run the reference in place of workflint, without the kind mapping
        /// or the allow-list. Any difference means the runs are not
        /// deterministic.
        #[arg(long, hide = true)]
        self_check: bool,
        /// Compare only the diagnostics of this actionlint kind, such as
        /// `syntax-check`. Repeat it for more kinds. The kinds are the names
        /// in `workflint_rules::kind`. Exit codes 0 and 1 are not
        /// compared, because the lines of the other kinds decide them.
        #[arg(long = "kind", value_name = "KIND")]
        kinds: Vec<String>,
    },
    /// Recreate the actionlint checkout, reference binary, and corpus in
    /// `.reference/`. Needs git and the Go version in `.go-version`.
    Reference,
    /// Copy the reference's test data from `.reference/actionlint` into
    /// `tests/conformance/actionlint/`, and list the time zones of the Go
    /// version in `.go-version` in `tests/conformance/go/time-zones.txt`.
    /// Clones the checkout first if it is missing. Needs that Go version.
    VendorTestdata,
    /// Compare the YAML trees of workflint-yaml and go-yaml: node kinds,
    /// positions, decoded values, anchors, and aliases. Run it after
    /// changing crates/workflint-yaml, crates/workflint-diagnostics, or
    /// xtask/goyaml-dump. Needs the Go version in `.go-version`.
    ///
    /// Without paths, it compares the vendored test data, the corpus, and
    /// tests/yaml-parity/. It exits with an error if a file differs.
    YamlParity {
        /// Files, or directories to search for `.yaml` and `.yml` files, to
        /// compare instead of the default inputs.
        paths: Vec<PathBuf>,
    },
}

fn parse_seconds(text: &str) -> Result<Duration, String> {
    let seconds = text
        .parse::<f64>()
        .ok()
        .filter(|seconds| *seconds > 0.0)
        .ok_or_else(|| format!("expected a positive number of seconds, got {text:?}"))?;
    Duration::try_from_secs_f64(seconds)
        .map_err(|_| format!("{text:?} seconds is longer than a time limit can be"))
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        None => Cli::command().print_help()?,
        Some(Command::Bench { timeout }) => {
            bench::run(&reference::Layout::repository(), timeout)?;
        }
        Some(Command::BenchHistory {
            history,
            results,
            issue_body,
        }) => {
            let results = match results {
                Some(results) => results,
                None => bench::target_directory(&reference::Layout::repository())?
                    .join("bench")
                    .join("results.json"),
            };
            bench::history::run(&history, &results, issue_body.as_deref())?;
        }
        Some(Command::ExprParity) => expr_parity::run(&reference::Layout::repository())?,
        Some(Command::FuzzSeeds) => fuzz_seeds::run(reference::Layout::repository().root())?,
        Some(Command::Oracle {
            paths,
            report_only,
            self_check,
            kinds,
        }) => {
            let options = oracle::Options {
                paths,
                report_only,
                self_check,
                kinds,
            };
            oracle::run(&reference::Layout::repository(), &options)?;
        }
        Some(Command::Reference) => reference::run(&reference::Layout::repository())?,
        Some(Command::VendorTestdata) => {
            vendor_testdata::run(&reference::Layout::repository())?;
        }
        Some(Command::YamlParity { paths }) => {
            yaml_parity::run(&reference::Layout::repository(), &paths)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn no_arguments_selects_no_subcommand() {
        let cli = Cli::try_parse_from(["xtask"]).unwrap();
        assert!(cli.command.is_none());
    }

    #[test]
    fn help_flag_requests_help() {
        let error = Cli::try_parse_from(["xtask", "--help"]).unwrap_err();
        assert_eq!(error.kind(), clap::error::ErrorKind::DisplayHelp);
    }

    #[test]
    fn expr_parity_selects_the_expr_parity_subcommand() {
        let cli = Cli::try_parse_from(["xtask", "expr-parity"]).unwrap();
        assert!(matches!(cli.command, Some(Command::ExprParity)));
    }

    #[test]
    fn fuzz_seeds_selects_the_fuzz_seeds_subcommand() {
        let cli = Cli::try_parse_from(["xtask", "fuzz-seeds"]).unwrap();
        assert!(matches!(cli.command, Some(Command::FuzzSeeds)));
    }

    #[test]
    fn oracle_takes_paths_and_report_only() {
        let cli = Cli::try_parse_from(["xtask", "oracle", "--report-only", "a.yaml"]).unwrap();
        let Some(Command::Oracle {
            paths,
            report_only,
            self_check,
            kinds,
        }) = cli.command
        else {
            panic!("{:?}", cli.command);
        };
        assert_eq!(paths, [PathBuf::from("a.yaml")]);
        assert!(report_only);
        assert!(!self_check);
        assert_eq!(kinds, Vec::<String>::new());
    }

    #[test]
    fn oracle_kind_can_be_repeated() {
        let cli = Cli::try_parse_from([
            "xtask",
            "oracle",
            "--kind",
            "syntax-check",
            "--kind",
            "expression",
        ])
        .unwrap();
        let Some(Command::Oracle { kinds, .. }) = cli.command else {
            panic!("{:?}", cli.command);
        };
        assert_eq!(kinds, ["syntax-check", "expression"]);
    }

    #[test]
    fn oracle_self_check_is_hidden_from_help() {
        let mut command = Cli::command();
        let help = command
            .find_subcommand_mut("oracle")
            .unwrap()
            .render_long_help()
            .to_string();
        assert!(help.contains("--report-only"), "{help}");
        assert!(!help.contains("--self-check"), "{help}");
        let cli = Cli::try_parse_from(["xtask", "oracle", "--self-check"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Oracle {
                self_check: true,
                ..
            })
        ));
    }

    #[test]
    fn reference_selects_the_reference_subcommand() {
        let cli = Cli::try_parse_from(["xtask", "reference"]).unwrap();
        assert!(matches!(cli.command, Some(Command::Reference)));
    }

    #[test]
    fn yaml_parity_without_paths_compares_every_input() {
        let cli = Cli::try_parse_from(["xtask", "yaml-parity"]).unwrap();
        assert!(matches!(cli.command, Some(Command::YamlParity { paths }) if paths.is_empty()));
    }

    #[test]
    fn yaml_parity_takes_paths_to_compare() {
        let cli = Cli::try_parse_from(["xtask", "yaml-parity", "a.yaml", "dir"]).unwrap();
        let Some(Command::YamlParity { paths }) = cli.command else {
            panic!("{:?}", cli.command);
        };
        assert_eq!(paths, [PathBuf::from("a.yaml"), PathBuf::from("dir")]);
    }

    #[test]
    fn vendor_testdata_selects_the_vendor_testdata_subcommand() {
        let cli = Cli::try_parse_from(["xtask", "vendor-testdata"]).unwrap();
        assert!(matches!(cli.command, Some(Command::VendorTestdata)));
    }

    #[test]
    fn bench_timeout_takes_seconds() {
        let cli = Cli::try_parse_from(["xtask", "bench", "--timeout", "1.5"]).unwrap();
        let Some(Command::Bench { timeout }) = cli.command else {
            panic!("expected the bench subcommand");
        };
        assert_eq!(timeout, Some(Duration::from_millis(1500)));
        assert!(Cli::try_parse_from(["xtask", "bench", "--timeout", "0"]).is_err());
    }

    #[test]
    fn bench_timeout_too_large_for_a_duration_is_an_error() {
        for huge in ["1e300", "inf", "NaN"] {
            let error = Cli::try_parse_from(["xtask", "bench", "--timeout", huge]).unwrap_err();
            assert_eq!(
                error.kind(),
                clap::error::ErrorKind::ValueValidation,
                "{huge}"
            );
        }
    }

    #[test]
    fn bench_history_needs_a_history_file() {
        assert!(Cli::try_parse_from(["xtask", "bench-history"]).is_err());
        let cli = Cli::try_parse_from(["xtask", "bench-history", "--history", "h.jsonl"]).unwrap();
        assert!(matches!(cli.command, Some(Command::BenchHistory { .. })));
    }
}
