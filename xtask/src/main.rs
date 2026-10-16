//! Developer commands for the workflint repository. Run `cargo xtask --help`
//! for the list.

// ADR 0018 lets xtask panic: it runs only on developer machines and in CI,
// never on user input.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod reference;
mod vendor_testdata;
mod yaml_parity;

use std::path::PathBuf;

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
    /// Recreate the actionlint checkout, reference binary, and corpus in
    /// `.reference/`. Needs git and the Go version in `.go-version`.
    Reference,
    /// Copy the reference's test data from `.reference/actionlint` into
    /// `tests/conformance/actionlint/`. Clones the checkout first if it is
    /// missing.
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

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        None => Cli::command().print_help()?,
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
}
