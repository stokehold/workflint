//! Whether the default and one-line formats print color.

/// Whether to print color, given `-color`, `-no-color`, the environment, and whether stdout is
/// a terminal.
///
/// `-no-color` wins over `-color`, and `-color` wins over the environment
/// (`docs/differences.md#color-flag-precedence`). Without either flag, color is off when
/// `NO_COLOR` is set and not empty, when `TERM` is `dumb`, or when stdout is not a terminal, as
/// in the reference.
pub(crate) fn enabled(
    color: bool,
    no_color: bool,
    env: impl Fn(&str) -> Option<String>,
    terminal: bool,
) -> bool {
    if no_color {
        return false;
    }
    if color {
        return true;
    }
    let no_color_env = env("NO_COLOR").is_some_and(|value| !value.is_empty());
    let dumb = env("TERM").is_some_and(|value| value == "dumb");
    terminal && !no_color_env && !dumb
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(vars: &'a [(&str, &str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            vars.iter()
                .find(|(var, _)| *var == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    #[test]
    fn a_terminal_gets_color() {
        assert!(enabled(false, false, env(&[("TERM", "xterm")]), true));
    }

    #[test]
    fn a_pipe_gets_no_color() {
        assert!(!enabled(false, false, env(&[]), false));
    }

    #[test]
    fn no_color_turns_color_off_when_it_is_not_empty() {
        assert!(!enabled(false, false, env(&[("NO_COLOR", "1")]), true));
        assert!(enabled(false, false, env(&[("NO_COLOR", "")]), true));
    }

    #[test]
    fn a_dumb_terminal_gets_no_color() {
        assert!(!enabled(false, false, env(&[("TERM", "dumb")]), true));
    }

    #[test]
    fn the_color_flag_wins_over_the_environment_and_a_pipe() {
        let vars = [("NO_COLOR", "1"), ("TERM", "dumb")];
        assert!(enabled(true, false, env(&vars), false));
    }

    #[test]
    fn the_no_color_flag_wins_over_the_color_flag() {
        assert!(!enabled(true, true, env(&[]), true));
    }
}
