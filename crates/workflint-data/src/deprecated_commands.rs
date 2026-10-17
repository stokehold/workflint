//! The workflow commands that GitHub deprecated, and the environment files that replace them.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `deprecatedCommandsPattern` in
//! `rule_deprecated_commands.go`, line 5, and the replacements in its `VisitStep`, lines 36-47.
//! Until M7 replaces them with a generated file (ADR 0011), keep them equal to that code. The
//! tests `commands_match_the_reference_pattern` and `replacements_match_the_reference_messages`
//! compare them. The pattern that finds the commands in a script belongs to the
//! `deprecated-commands` rule.
//!
//! - <https://github.blog/changelog/2020-10-01-github-actions-deprecating-set-env-and-add-path-commands/>
//! - <https://github.blog/changelog/2022-10-11-github-actions-deprecating-save-state-and-set-output-commands/>

/// A deprecated workflow command, such as `::set-output name=result::value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeprecatedCommand {
    /// The command after `::`, such as `set-output`.
    pub name: &'static str,
    /// Whether the command takes a `name=` parameter, as in `::set-env name=X::value`.
    /// `::add-path::dir` takes none.
    pub takes_name: bool,
    /// The shell command that the message suggests instead, such as
    /// `echo "{name}={value}" >> $GITHUB_OUTPUT`.
    pub replacement: &'static str,
}

const fn command(
    name: &'static str,
    takes_name: bool,
    replacement: &'static str,
) -> DeprecatedCommand {
    DeprecatedCommand {
        name,
        takes_name,
        replacement,
    }
}

/// Every deprecated workflow command, in the order of the reference's pattern.
pub static DEPRECATED_COMMANDS: &[DeprecatedCommand] = &[
    command(
        "save-state",
        true,
        r#"echo "{name}={value}" >> $GITHUB_STATE"#,
    ),
    command(
        "set-output",
        true,
        r#"echo "{name}={value}" >> $GITHUB_OUTPUT"#,
    ),
    command("set-env", true, r#"echo "{name}={value}" >> $GITHUB_ENV"#),
    command("add-path", false, r#"echo "{path}" >> $GITHUB_PATH"#),
];

/// Returns the deprecated command called `name`. Workflow command names are case-sensitive.
#[must_use]
pub fn deprecated_command(name: &str) -> Option<&'static DeprecatedCommand> {
    DEPRECATED_COMMANDS
        .iter()
        .find(|command| command.name == name)
}
