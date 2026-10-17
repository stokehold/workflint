//! Case-insensitive comparisons that give the reference's answers for the ASCII names in the
//! tables, without allocating.
//!
//! Go's `strings.ToLower` and `strings.EqualFold` use Unicode's simple case mappings, which
//! map three characters outside ASCII onto ASCII letters: `İ` (U+0130) lowers to `i`, the
//! Kelvin sign (U+212A) lowers and folds to `k`, and `ſ` (U+017F) folds to `s`. Rust's
//! `str::to_lowercase` lowers `İ` to two characters instead.

/// Returns whether Go's `strings.ToLower(input) == name`, for a `name` that is lower-case
/// ASCII.
pub(crate) fn eq_go_lower(input: &str, name: &str) -> bool {
    let mut chars = input.chars();
    for expected in name.chars() {
        let lowered = match chars.next() {
            Some('\u{130}') => 'i',
            Some('\u{212a}') => 'k',
            Some(c) => c.to_ascii_lowercase(),
            None => return false,
        };
        if lowered != expected {
            return false;
        }
    }
    chars.next().is_none()
}

/// Returns whether Go's `strings.EqualFold(input, name)`, for a `name` that is ASCII.
pub(crate) fn eq_go_fold(input: &str, name: &str) -> bool {
    let mut chars = input.chars();
    for expected in name.chars() {
        let folded = match chars.next() {
            Some('\u{17f}') => 's',
            Some('\u{212a}') => 'k',
            Some(c) => c.to_ascii_lowercase(),
            None => return false,
        };
        if folded != expected.to_ascii_lowercase() {
            return false;
        }
    }
    chars.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_matches_ascii_letters_of_either_case() {
        assert!(eq_go_lower("Ubuntu-LATEST", "ubuntu-latest"));
        assert!(!eq_go_lower("ubuntu-latest", "ubuntu-latest-arm"));
        assert!(!eq_go_lower("ubuntu-latest-arm", "ubuntu-latest"));
    }

    #[test]
    fn lower_maps_dotted_capital_i_and_kelvin_sign_but_not_long_s() {
        assert!(eq_go_lower("\u{130}", "i"));
        assert!(eq_go_lower("\u{212a}", "k"));
        assert!(!eq_go_lower("\u{17f}", "s"));
    }

    #[test]
    fn fold_maps_long_s_and_kelvin_sign_but_not_dotted_capital_i() {
        assert!(eq_go_fold("\u{17f}ELF", "self"));
        assert!(eq_go_fold("\u{212a}", "k"));
        assert!(!eq_go_fold("\u{130}", "i"));
    }

    #[test]
    fn fold_compares_the_whole_string() {
        assert!(eq_go_fold("X64", "x64"));
        assert!(!eq_go_fold("x64 ", "x64"));
        assert!(!eq_go_fold("x6", "x64"));
    }
}
