//! `workflint completion <shell>`: the completion scripts for bash, zsh, fish, and PowerShell.
//!
//! Each script lists the flags of [`FLAGS`] with a single dash, as Go's `flag` package writes
//! them, and completes their values as [`Values`] says. Like the parser, a script offers no
//! flag after the first argument that is not a flag, or after `--`.

use std::ffi::OsString;
use std::fmt::Write as _;

use crate::flags::{FLAGS, Flag, Values};

/// The commands that may come first.
const COMMANDS: [&str; 2] = ["check", "completion"];

/// A shell that `workflint completion` writes a script for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shell {
    Bash,
    Zsh,
    Fish,
    Powershell,
}

/// The shells by their names on the command line, in the order the error lists them.
const SHELLS: [(&str, Shell); 4] = [
    ("bash", Shell::Bash),
    ("zsh", Shell::Zsh),
    ("fish", Shell::Fish),
    ("powershell", Shell::Powershell),
];

/// What the arguments after `completion` ask for.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Request {
    /// `-h` or `-help`.
    Help,
    /// The script for a shell.
    Script(Shell),
}

/// Reads the arguments after `completion`: the name of one shell, or `-h`.
///
/// # Errors
///
/// Fails with the message to print before the usage.
pub(crate) fn parse(args: &[OsString]) -> Result<Request, String> {
    match args {
        [arg] if ["-h", "-help", "--h", "--help"].contains(&arg.to_string_lossy().as_ref()) => {
            Ok(Request::Help)
        }
        [arg] => SHELLS
            .iter()
            .find(|(name, _)| *arg == **name)
            .map(|&(_, shell)| Request::Script(shell))
            .ok_or_else(error),
        _ => Err(error()),
    }
}

fn error() -> String {
    "completion takes one argument, the name of a shell: bash, zsh, fish, or powershell".to_owned()
}

/// The completion script for `shell`.
pub(crate) fn script(shell: Shell) -> String {
    let mut flags: Vec<&Flag> = FLAGS.iter().collect();
    flags.sort_by_key(|flag| flag.name);
    match shell {
        Shell::Bash => bash(&flags),
        Shell::Zsh => zsh(&flags),
        Shell::Fish => fish(&flags),
        Shell::Powershell => powershell(&flags),
    }
}

/// The names of the output formats, separated by spaces.
fn format_names() -> String {
    let names: Vec<&str> = crate::FORMAT_NAMES.iter().map(|(name, _)| *name).collect();
    names.join(" ")
}

/// The names of the shells, sorted and separated by spaces.
fn shell_names() -> String {
    let mut names: Vec<&str> = SHELLS.iter().map(|(name, _)| *name).collect();
    names.sort_unstable();
    names.join(" ")
}

/// The names of the flags with `-`, and of the flags in `filter` without it, separated by
/// spaces.
fn names(flags: &[&Flag], dash: &str, filter: impl Fn(&Flag) -> bool) -> String {
    let names: Vec<String> = flags
        .iter()
        .filter(|flag| filter(flag))
        .map(|flag| format!("{dash}{}", flag.name))
        .collect();
    names.join(" ")
}

/// Quotes `text` for a POSIX shell, zsh, or fish with single quotes.
fn single_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// `template` with each `@NAME@` replaced by its value.
fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut text = template.to_owned();
    for (name, value) in values {
        text = text.replace(&format!("@{name}@"), value);
    }
    text
}

fn bash(flags: &[&Flag]) -> String {
    let value_flags = |values: Values| names(flags, "", |flag| flag.values == values);
    fill(
        include_str!("completion/workflint.bash"),
        &[
            ("ALL_FLAGS", &names(flags, "-", |_| true)),
            ("BOOL_FLAGS", &names(flags, "", Flag::is_bool)),
            ("VALUE_FLAGS", &names(flags, "", |flag| !flag.is_bool())),
            ("FILE_FLAGS", &value_flags(Values::File)),
            ("COMMAND_FLAGS", &value_flags(Values::Command)),
            ("FORMAT_FLAGS", &value_flags(Values::Format)),
            ("SHELLS", &shell_names()),
            ("COMMANDS", &COMMANDS.join(" ")),
            ("FORMATS", &format_names()),
        ],
    )
}

/// Escapes `text` for the description in brackets of an `_arguments` spec.
fn zsh_description(text: &str) -> String {
    text.replace('\\', r"\\")
        .replace('[', r"\[")
        .replace(']', r"\]")
        .replace(':', r"\:")
}

fn zsh(flags: &[&Flag]) -> String {
    let mut specs = String::new();
    for flag in flags {
        let repeat = if flag.name == "ignore" { "*" } else { "" };
        let (equals, action) = if flag.is_bool() {
            ("", String::new())
        } else {
            let action = match flag.values {
                Values::Any => " ".to_owned(),
                Values::File => "_files".to_owned(),
                Values::Command => "_command_names -e".to_owned(),
                Values::Format => format!("({})", format_names()),
            };
            ("=", format!(":{}:{action}", flag.value_name))
        };
        let spec = format!(
            "{repeat}-{}{equals}[{}]{action}",
            flag.name,
            zsh_description(flag.summary())
        );
        let _ = writeln!(specs, "    {} \\", single_quote(&spec));
    }
    fill(
        include_str!("completion/workflint.zsh"),
        &[
            ("SPECS", &specs),
            ("SHELLS", &shell_names()),
            ("COMMANDS", &COMMANDS.join(" ")),
        ],
    )
}

fn fish(flags: &[&Flag]) -> String {
    let mut lines = String::new();
    for flag in flags {
        let values = match flag.values {
            _ if flag.is_bool() => String::new(),
            Values::Any => " -x".to_owned(),
            Values::File => " -r -F".to_owned(),
            Values::Command => " -x -a '(__fish_complete_command)'".to_owned(),
            Values::Format => format!(" -x -a {}", single_quote(&format_names())),
        };
        let _ = writeln!(
            lines,
            "complete -c workflint -n __workflint_flags_allowed -o {}{values} -d {}",
            flag.name,
            single_quote(flag.summary())
        );
    }
    fill(
        include_str!("completion/workflint.fish"),
        &[
            ("LINES", &lines),
            ("VALUE_FLAGS", &names(flags, "", |flag| !flag.is_bool())),
            ("SHELLS", &shell_names()),
        ],
    )
}

/// Quotes `text` for PowerShell with single quotes.
fn powershell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// The words of `names`, quoted for PowerShell and separated by commas.
fn powershell_list(names: &str) -> String {
    let quoted: Vec<String> = names.split(' ').map(powershell_quote).collect();
    quoted.join(", ")
}

fn powershell(flags: &[&Flag]) -> String {
    let mut table = String::new();
    for flag in flags {
        let values = match flag.values {
            _ if flag.is_bool() => "Bool",
            Values::Any => "Any",
            Values::File => "File",
            Values::Command => "Command",
            Values::Format => "Format",
        };
        let _ = writeln!(
            table,
            "        @{{ Name = {}; Values = '{values}'; Help = {} }}",
            powershell_quote(flag.name),
            powershell_quote(flag.summary())
        );
    }
    fill(
        include_str!("completion/workflint.ps1"),
        &[
            ("TABLE", &table),
            ("FORMATS", &powershell_list(&format_names())),
            ("SHELLS", &powershell_list(&shell_names())),
            ("COMMANDS", &powershell_list(&COMMANDS.join(" "))),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn completion_takes_one_shell_name_or_help() {
        assert_eq!(
            parse(&args(&["powershell"])),
            Ok(Request::Script(Shell::Powershell))
        );
        assert_eq!(parse(&args(&["--help"])), Ok(Request::Help));
        for bad in [&[][..], &["sh"], &["bash", "zsh"], &["Bash"]] {
            assert_eq!(parse(&args(bad)), Err(error()), "{bad:?}");
        }
    }

    #[test]
    fn every_script_lists_every_flag_with_a_single_dash() {
        for (name, shell) in SHELLS {
            let script = script(shell);
            for flag in &FLAGS {
                let listed = match shell {
                    Shell::Bash => script
                        .split([' ', '\''])
                        .any(|word| word == format!("-{}", flag.name)),
                    Shell::Zsh => {
                        script.contains(&format!("'-{}[", flag.name))
                            || script.contains(&format!("'-{}=[", flag.name))
                            || script.contains(&format!("'*-{}=[", flag.name))
                    }
                    Shell::Fish => script.contains(&format!(" -o {} ", flag.name)),
                    Shell::Powershell => script.contains(&format!("Name = '{}';", flag.name)),
                };
                assert!(listed, "{name} does not list -{}", flag.name);
            }
        }
    }

    #[test]
    fn single_quotes_are_escaped() {
        assert_eq!(single_quote("it's"), r"'it'\''s'");
        assert_eq!(powershell_quote("it's"), "'it''s'");
        assert_eq!(zsh_description("a [b]: c"), r"a \[b\]\: c");
    }
}
