//! Checks the deprecated workflow commands against the reference's
//! `deprecatedCommandsPattern` and the replacements in `VisitStep` in the vendored
//! `rule_deprecated_commands.go`.

#![allow(clippy::panic)]

mod go_source;

use go_source::{deprecated_command_replacements, deprecated_commands_pattern};
use workflint_data::{DEPRECATED_COMMANDS, deprecated_command};

/// The reference's pattern rebuilt from the table. The name pattern belongs to the
/// `deprecated-commands` rule, so it is copied here.
fn pattern_from_the_table() -> String {
    let names = |takes_name: bool| {
        DEPRECATED_COMMANDS
            .iter()
            .filter(|command| command.takes_name == takes_name)
            .map(|command| command.name)
            .collect::<Vec<_>>()
            .join("|")
    };
    format!(
        r"(?:::({})\s+name=[a-zA-Z][a-zA-Z_-]*::\S+|::({})::\S+)",
        names(true),
        names(false)
    )
}

#[test]
fn commands_match_the_reference_pattern() {
    assert_eq!(pattern_from_the_table(), deprecated_commands_pattern());
}

#[test]
fn replacements_match_the_reference_messages() {
    let mut ours: Vec<(String, String)> = DEPRECATED_COMMANDS
        .iter()
        .map(|command| (command.name.to_owned(), command.replacement.to_owned()))
        .collect();
    let mut reference = deprecated_command_replacements();
    ours.sort();
    reference.sort();
    assert_eq!(ours, reference);
}

#[test]
fn lookup_finds_a_command_by_its_exact_name() {
    let Some(add_path) = deprecated_command("add-path") else {
        panic!("add-path is not a deprecated command");
    };
    assert!(!add_path.takes_name);
    assert_eq!(add_path.replacement, r#"echo "{path}" >> $GITHUB_PATH"#);
    assert_eq!(deprecated_command("Set-Output"), None);
}
