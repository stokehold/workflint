//! Finds the known name closest to an undefined one, for the help text of the
//! `undefined-variable`, `undefined-property`, and `undefined-function` errors.

use workflint_diagnostics::quote::quote;

/// Returns the help text that suggests `candidate`, such as `did you mean "github"?`.
pub(crate) fn did_you_mean(candidate: &str) -> String {
    format!("did you mean {}?", quote(candidate))
}

/// Returns the value of the candidate whose key is closest to `name`, or `None` if none is
/// close enough. Each candidate is a key, which is compared with `name`, and a value.
///
/// Names are compared without regard to ASCII case, as expressions compare them. The distance
/// counts the characters inserted, deleted, or replaced, and two adjacent characters swapped
/// count as one edit. A candidate is close enough when the distance is at most a third of the
/// length of `name`, and at least 1, so `gihub` finds `github` and `foo` finds nothing. Among
/// the closest keys the smallest in byte order wins, which keeps the result independent of the
/// order of `candidates`.
pub(crate) fn closest<'a, T>(
    name: &str,
    candidates: impl IntoIterator<Item = (&'a str, T)>,
) -> Option<T> {
    // Most undefined properties are read from objects with no properties at all, such as
    // `steps` before any step has an ID, so return before any other work.
    let mut candidates = candidates.into_iter().peekable();
    candidates.peek()?;
    let name_len = name.chars().count();
    let limit = name_len.max(3) / 3;
    // Built when the first candidate passes the length check, so a name with no candidate of
    // a close length allocates nothing.
    let mut name_chars = Vec::new();
    let mut key_chars = Vec::new();
    let mut rows = Rows::default();
    let mut best: Option<(usize, &str, T)> = None;
    for (key, value) in candidates {
        // The distance is at least the difference in length, so most candidates are rejected
        // without computing it.
        if key.chars().count().abs_diff(name_len) > limit {
            continue;
        }
        if name_chars.is_empty() {
            name_chars.extend(name.chars().map(|c| c.to_ascii_lowercase()));
        }
        key_chars.clear();
        key_chars.extend(key.chars().map(|c| c.to_ascii_lowercase()));
        let distance = rows.edit_distance(&name_chars, &key_chars);
        if distance > limit {
            continue;
        }
        let better = match &best {
            None => true,
            Some((best_distance, best_key, _)) => (distance, key) < (*best_distance, *best_key),
        };
        if better {
            best = Some((distance, key, value));
        }
    }
    best.map(|(_, _, value)| value)
}

/// Three rows of the dynamic programming table of [`Rows::edit_distance`]: two rows back, the
/// previous, and the current. [`closest`] reuses them for every candidate.
#[derive(Default)]
struct Rows {
    before: Vec<usize>,
    prev: Vec<usize>,
    cur: Vec<usize>,
}

impl Rows {
    /// The optimal string alignment distance between `a` and `b`: the Levenshtein distance,
    /// where swapping two adjacent characters also counts as one edit.
    fn edit_distance(&mut self, a: &[char], b: &[char]) -> usize {
        let Self { before, prev, cur } = self;
        before.clear();
        before.resize(b.len() + 1, 0);
        prev.clear();
        prev.extend(0..=b.len());
        cur.clear();
        cur.resize(b.len() + 1, 0);
        for (i, &ca) in a.iter().enumerate() {
            cur[0] = i + 1;
            for (j, &cb) in b.iter().enumerate() {
                let cost = usize::from(ca != cb);
                let mut d = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
                if i > 0 && j > 0 && a.get(i - 1) == Some(&cb) && b.get(j - 1) == Some(&ca) {
                    d = d.min(before[j - 1] + 1);
                }
                cur[j + 1] = d;
            }
            std::mem::swap(before, prev);
            std::mem::swap(prev, cur);
        }
        prev[b.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn distance(a: &str, b: &str) -> usize {
        let a: Vec<char> = a.chars().collect();
        let b: Vec<char> = b.chars().collect();
        Rows::default().edit_distance(&a, &b)
    }

    #[test]
    fn distance_counts_inserted_deleted_and_replaced_characters() {
        assert_eq!(distance("", ""), 0);
        assert_eq!(distance("", "abc"), 3);
        assert_eq!(distance("abc", ""), 3);
        assert_eq!(distance("gihub", "github"), 1);
        assert_eq!(distance("githubb", "github"), 1);
        assert_eq!(distance("gitlab", "github"), 2);
        assert_eq!(distance("kitten", "sitting"), 3);
    }

    #[test]
    fn distance_counts_a_swap_of_adjacent_characters_as_one_edit() {
        assert_eq!(distance("gihtub", "github"), 1);
        assert_eq!(distance("ab", "ba"), 1);
        assert_eq!(distance("abc", "ca"), 3);
    }

    fn closest_name<'a>(name: &str, candidates: &[&'a str]) -> Option<&'a str> {
        closest(name, candidates.iter().map(|&key| (key, key)))
    }

    #[test]
    fn closest_ignores_ascii_case() {
        assert_eq!(closest_name("GitHb", &["github", "env"]), Some("github"));
    }

    #[test]
    fn closest_rejects_candidates_beyond_a_third_of_the_name() {
        assert_eq!(closest_name("foo", &["job", "env"]), None);
        assert_eq!(closest_name("jo", &["job"]), Some("job"));
        assert_eq!(closest_name("strtwth", &["startsWith"]), None);
        assert_eq!(
            closest_name("strtswth", &["startsWith"]),
            Some("startsWith")
        );
    }

    #[test]
    fn closest_finds_nothing_without_candidates() {
        assert_eq!(closest_name("github", &[]), None);
    }

    #[test]
    fn closest_skips_candidates_whose_length_differs_too_much() {
        assert_eq!(
            closest_name("gihub", &["githubactions", "github"]),
            Some("github")
        );
    }

    #[test]
    fn closest_breaks_ties_by_byte_order() {
        assert_eq!(closest_name("jo", &["jobs", "job"]), Some("job"));
        assert_eq!(closest_name("ab", &["ac", "aa"]), Some("aa"));
        assert_eq!(closest_name("ab", &["aa", "ac"]), Some("aa"));
    }
}
