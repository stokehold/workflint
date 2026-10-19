//! The `deprecated-commands` rule: `set-output`, `save-state`, `set-env`, and `add-path`
//! workflow commands in `run:` scripts.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `rule_deprecated_commands.go`.

use workflint_data::{DEPRECATED_COMMANDS, DeprecatedCommand};
use workflint_diagnostics::{Category, RuleMeta, Severity};
use workflint_syntax::{Exec, Step};

use crate::{Context, Rule};

/// A `run:` script uses a workflow command that GitHub deprecated.
pub(crate) static DEPRECATED_WORKFLOW_COMMAND: RuleMeta = RuleMeta {
    id: "deprecated-workflow-command",
    description: "A `run:` script uses the deprecated `set-output`, `save-state`, `set-env`, or `add-path` workflow command",
    kind: "deprecated-commands",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/deprecated-workflow-command.md",
};

/// The reference's `RuleDeprecatedCommands`.
pub(crate) struct DeprecatedCommands;

impl<'a> Rule<'a> for DeprecatedCommands {
    fn step(&mut self, step: &'a Step<'a>, cx: &mut Context<'_>) {
        let Some(Exec::Run(run)) = &step.exec else {
            return;
        };
        let Some(script) = run.run.value() else {
            return;
        };
        // One error per command, all at the start of the script, as in the reference.
        for command in find_commands(script.value()) {
            cx.report(
                &DEPRECATED_WORKFLOW_COMMAND,
                script.span(),
                format!(
                    "workflow command \"{}\" was deprecated. use `{}` instead: https://docs.github.com/en/actions/using-workflows/workflow-commands-for-github-actions",
                    command.name, command.replacement
                ),
            );
        }
    }
}

/// Returns the deprecated commands in `script`, in order. It finds the matches of the reference's
/// pattern `::(save-state|set-output|set-env)\s+name=[a-zA-Z][a-zA-Z_-]*::\S+|::(add-path)::\S+`
/// as Go's `FindAllString` does: leftmost first and without overlap. Like Go's, `\s` is the ASCII
/// white space `[\t\n\f\r ]` and `\S` is any other character.
///
/// The name also allows digits after its first letter, which the reference misses (see
/// `docs/differences.md#deprecated-command-digits`).
fn find_commands(script: &str) -> Vec<&'static DeprecatedCommand> {
    let bytes = script.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match match_at(bytes, at) {
            Some((command, end)) => {
                found.push(command);
                at = end;
            }
            None => at += 1,
        }
    }
    found
}

/// Matches one command at byte `at` and returns it with the byte where the match ends. Every byte
/// the pattern names is ASCII and `\S+` stops only at ASCII, so the end is a character boundary.
fn match_at(bytes: &[u8], at: usize) -> Option<(&'static DeprecatedCommand, usize)> {
    let rest = bytes.get(at..)?.strip_prefix(b"::")?;
    DEPRECATED_COMMANDS.iter().find_map(|command| {
        let mut rest = rest.strip_prefix(command.name.as_bytes())?;
        if command.takes_name {
            rest = skip(rest, is_space, 1)?;
            rest = rest.strip_prefix(b"name=")?;
            rest = skip(rest, |byte| byte.is_ascii_alphabetic(), 1)?;
            rest = skip(rest, is_name_byte, 0)?;
        }
        rest = rest.strip_prefix(b"::")?;
        rest = skip(rest, |byte| !is_space(byte), 1)?;
        Some((command, bytes.len() - rest.len()))
    })
}

/// Skips the leading bytes of `bytes` that satisfy `matches`, or returns `None` when fewer than
/// `min` do.
fn skip(bytes: &[u8], matches: impl Fn(u8) -> bool, min: usize) -> Option<&[u8]> {
    let count = bytes.iter().take_while(|&&byte| matches(byte)).count();
    (count >= min).then(|| bytes.get(count..)).flatten()
}

/// Go's `\s`: tab, newline, form feed, carriage return, and space. Not vertical tab.
fn is_space(byte: u8) -> bool {
    matches!(byte, b'\t' | b'\n' | b'\x0c' | b'\r' | b' ')
}

/// A byte of a name after its first letter.
fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use workflint_diagnostics::{Color, FileId, SourceFile, render_oneline};

    use super::{DeprecatedCommands, find_commands};
    use crate::{Context, Rule, visit};

    /// Returns the names of the deprecated commands in `script`.
    fn names(script: &str) -> Vec<&'static str> {
        find_commands(script)
            .iter()
            .map(|command| command.name)
            .collect()
    }

    /// Lints `text` with only the `deprecated-commands` rule and returns its diagnostics in the
    /// one-line format.
    fn lint(text: &str) -> Vec<String> {
        let file = SourceFile::new(FileId::new(0), "test.yaml", text).unwrap();
        let document = workflint_yaml::parse(&file).document.unwrap();
        let parsed = workflint_syntax::parse(&file, &document);
        let mut rules: Vec<Box<dyn Rule<'_>>> = vec![Box::new(DeprecatedCommands)];
        let mut cx = Context::new(&file);
        visit(&parsed.workflow, &mut rules, &mut cx);
        let mut out = Vec::new();
        render_oneline(&mut out, &file, &cx.into_diagnostics(), Color::Off).unwrap();
        String::from_utf8(out)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    // The cases of the reference's `TestRuleDeprecatedCommandsDetectTargetCommands`.

    #[test]
    fn each_deprecated_command_is_found() {
        assert_eq!(names("::save-state name=foo::42"), ["save-state"]);
        assert_eq!(names("::set-output name=foo::42"), ["set-output"]);
        assert_eq!(names("::set-env name=foo::42"), ["set-env"]);
        assert_eq!(names("::add-path::/path/to/foo"), ["add-path"]);
    }

    #[test]
    fn a_command_inside_other_text_is_found() {
        assert_eq!(
            names("hello::set-output name=foo::42 world"),
            ["set-output"]
        );
    }

    #[test]
    fn every_command_in_a_script_is_found_in_order() {
        assert_eq!(
            names("::set-output name=foo::42 ::set-output name=bar::xxx"),
            ["set-output", "set-output"]
        );
        assert_eq!(
            names("::set-output name=foo::42 ::add-path::/path/to/foo ::save-state name=foo::42"),
            ["set-output", "add-path", "save-state"]
        );
        assert_eq!(
            names(
                "hello::set-output name=foo::42 how ::add-path::/path/to/foo are ::save-state name=foo::42 you"
            ),
            ["set-output", "add-path", "save-state"]
        );
    }

    #[test]
    fn text_between_the_command_and_its_name_is_not_a_command() {
        assert_eq!(names("::set-output hello name=foo::42"), Vec::<&str>::new());
    }

    #[test]
    fn several_spaces_and_tabs_before_the_name_are_allowed() {
        assert_eq!(
            names("::set-output    \t\t    name=foo::42"),
            ["set-output"]
        );
    }

    #[test]
    fn a_script_without_a_command_has_none() {
        assert_eq!(names(""), Vec::<&str>::new());
        assert_eq!(names("echo 'do not use set-output!'"), Vec::<&str>::new());
    }

    #[test]
    fn a_name_may_have_hyphens_and_underscores_after_its_first_letter() {
        assert_eq!(names("::save-state name=foo_bar-woo::42"), ["save-state"]);
        assert_eq!(names("::save-state name=-foo::42"), Vec::<&str>::new());
    }

    #[test]
    fn a_parameter_other_than_name_is_not_a_command() {
        assert_eq!(names("::save-state myname=foo::42"), Vec::<&str>::new());
    }

    // Cases beyond the reference's tests.

    #[test]
    fn a_name_may_have_digits_after_its_first_letter() {
        assert_eq!(names("::set-output name=result1::x"), ["set-output"]);
        assert_eq!(names("::set-env name=V2_HOME::x"), ["set-env"]);
        assert_eq!(names("::set-output name=1result::x"), Vec::<&str>::new());
    }

    #[test]
    fn non_ascii_white_space_does_not_separate_the_command_from_its_name() {
        // U+00A0 and U+3000 are white space in Unicode but not in Go's `\s`.
        assert_eq!(names("::set-output\u{a0}name=foo::42"), Vec::<&str>::new());
        assert_eq!(
            names("::set-output\u{3000}name=foo::42"),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn non_ascii_white_space_and_vertical_tab_count_as_a_value() {
        assert_eq!(names("::add-path::\u{3000}"), ["add-path"]);
        assert_eq!(names("::add-path::\u{b}"), ["add-path"]);
        assert_eq!(names("::set-output\u{b}name=foo::42"), Vec::<&str>::new());
    }

    #[test]
    fn a_command_needs_a_value() {
        assert_eq!(names("::add-path:: dir"), Vec::<&str>::new());
        assert_eq!(names("::set-output name=foo::"), Vec::<&str>::new());
    }

    #[test]
    fn a_value_runs_to_the_next_white_space_and_hides_a_command_in_it() {
        assert_eq!(
            names("::set-output name=a::x::set-output name=b::y"),
            ["set-output"]
        );
        assert_eq!(names(":::add-path::/bin"), ["add-path"]);
    }

    #[test]
    fn command_names_are_case_sensitive() {
        assert_eq!(names("::SET-OUTPUT name=foo::42"), Vec::<&str>::new());
        assert_eq!(names("::set-output NAME=foo::42"), Vec::<&str>::new());
    }

    #[test]
    fn each_command_is_reported_at_the_start_of_the_script() {
        let text = "on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: |
          echo '::set-output name=foo::bar'
          echo '::add-path::/opt/bin'
      - uses: actions/checkout@v4
";
        assert_eq!(
            lint(text),
            [
                "test.yaml:6:14: workflow command \"set-output\" was deprecated. use `echo \"{name}={value}\" >> $GITHUB_OUTPUT` instead: https://docs.github.com/en/actions/using-workflows/workflow-commands-for-github-actions [deprecated-workflow-command]",
                "test.yaml:6:14: workflow command \"add-path\" was deprecated. use `echo \"{path}\" >> $GITHUB_PATH` instead: https://docs.github.com/en/actions/using-workflows/workflow-commands-for-github-actions [deprecated-workflow-command]",
            ]
        );
    }
}
