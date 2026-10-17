//! The naming rules of configuration variables, which expressions read as `vars.NAME`.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `checkConfigVariables` in `expr_sema.go`,
//! which follows
//! <https://docs.github.com/en/actions/learn-github-actions/variables#naming-conventions-for-configuration-variables>.
//! A name may not start with a digit either, but the expression lexer already rejects such a
//! name.

/// The prefix that no configuration variable name may start with, in lower case. GitHub
/// compares it without regard to case.
pub const RESERVED_CONFIG_VARIABLE_PREFIX: &str = "github_";

/// Returns whether `c` may appear in a configuration variable name in lower case: an ASCII
/// lower-case letter, a digit, or `_`.
#[must_use]
pub const fn is_config_variable_name_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_digits_and_underscores_are_allowed() {
        assert!("abc_123".chars().all(is_config_variable_name_char));
    }

    #[test]
    fn dashes_dots_upper_case_and_non_ascii_letters_are_not_allowed() {
        for c in ['-', '.', 'A', 'é', ' '] {
            assert!(!is_config_variable_name_char(c), "{c:?}");
        }
    }
}
