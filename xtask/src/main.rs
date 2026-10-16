//! Developer commands for the workflint repository. Run `cargo xtask --help`
//! for the list.

// ADR 0018 lets xtask panic: it runs only on developer machines and in CI,
// never on user input.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod reference;
mod vendor_testdata;

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
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        None => Cli::command().print_help()?,
        Some(Command::Reference) => reference::run(&reference::Layout::repository())?,
        Some(Command::VendorTestdata) => {
            vendor_testdata::run(&reference::Layout::repository())?;
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
    fn vendor_testdata_selects_the_vendor_testdata_subcommand() {
        let cli = Cli::try_parse_from(["xtask", "vendor-testdata"]).unwrap();
        assert!(matches!(cli.command, Some(Command::VendorTestdata)));
    }
}
