//! Finds the known name closest to an undefined one, for the help text of the
//! `undefined-variable`, `undefined-property`, and `undefined-function` errors.

use workflint_diagnostics::quote::quote;

/// The most work that one search does: the characters read to compare lengths plus the
/// cells of the edit distance table. A search that needs more suggests nothing. A name of 20
/// characters and 200 candidates of about its length need at most 57,000, while 2000
/// candidates of 100 characters would need millions for each undefined name.
const MAX_WORK: usize = 1 << 16;

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
///
/// The search gives up and returns `None` once it has done [`MAX_WORK`].
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
    let mut work = Work(MAX_WORK);
    // Built when the first candidate passes the length check, so a name with no candidate of
    // a close length allocates nothing.
    let mut name_chars = Vec::new();
    let mut key_chars = Vec::new();
    let mut rows = Rows::default();
    let mut best: Option<(usize, &str, T)> = None;
    for (key, value) in candidates {
        // Only a key as close as the best one so far can replace it.
        let bound = best.as_ref().map_or(limit, |(distance, _, _)| *distance);
        // The distance is at least the difference in length, so most candidates are rejected
        // without computing it. Counting stops once the key is too long.
        let key_len = key.chars().take(name_len + bound + 1).count();
        work.spend(key_len + 1).ok()?;
        if key_len.abs_diff(name_len) > bound {
            continue;
        }
        if name_chars.is_empty() {
            name_chars.extend(name.chars().map(|c| c.to_ascii_lowercase()));
        }
        key_chars.clear();
        key_chars.extend(key.chars().map(|c| c.to_ascii_lowercase()));
        let Some(distance) = rows
            .edit_distance(&name_chars, &key_chars, bound, &mut work)
            .ok()?
        else {
            continue;
        };
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

/// The work that a search may still do.
struct Work(usize);

/// The search ran out of [`Work`].
#[derive(Debug, PartialEq, Eq)]
struct OutOfWork;

impl Work {
    fn spend(&mut self, amount: usize) -> Result<(), OutOfWork> {
        self.0 = self.0.checked_sub(amount).ok_or(OutOfWork)?;
        Ok(())
    }
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
    /// The optimal string alignment distance between `a` and `b`, the Levenshtein distance
    /// where swapping two adjacent characters also counts as one edit, if it is at most
    /// `limit`, or `None` if it is greater. Returns [`OutOfWork`] if `work` runs out first.
    ///
    /// Only the cells within `limit` of the diagonal can lead to a distance of at most
    /// `limit`, so each row computes those alone. Cells outside hold `limit + 1`, which
    /// stands for every greater distance. The distance is at least the smallest cell of any
    /// row, so the computation stops at the first row whose cells all exceed `limit`.
    fn edit_distance(
        &mut self,
        a: &[char],
        b: &[char],
        limit: usize,
        work: &mut Work,
    ) -> Result<Option<usize>, OutOfWork> {
        if b.is_empty() {
            return Ok((a.len() <= limit).then_some(a.len()));
        }
        let over = limit + 1;
        let Self { before, prev, cur } = self;
        for row in [&mut *before, &mut *prev, &mut *cur] {
            row.clear();
            row.resize(b.len() + 1, over);
        }
        for (j, cell) in prev.iter_mut().enumerate().take(over) {
            *cell = j;
        }
        for (i, &ca) in a.iter().enumerate() {
            // Row `i + 1` of the table compares `a[..=i]`, and its band is the columns
            // `lo..=hi`.
            let row = i + 1;
            let lo = row.saturating_sub(limit).max(1);
            let hi = (row + limit).min(b.len());
            if lo > hi {
                // `b` is more than `limit` characters shorter than `a`.
                return Ok(None);
            }
            work.spend(hi - lo + 1)?;
            cur[lo - 1] = if lo == 1 { row } else { over };
            let mut row_min = cur[lo - 1];
            for j in lo..=hi {
                let cb = b[j - 1];
                let cost = usize::from(ca != cb);
                let mut d = (prev[j - 1] + cost).min(prev[j] + 1).min(cur[j - 1] + 1);
                if i > 0 && j > 1 && a[i - 1] == cb && b[j - 2] == ca {
                    d = d.min(before[j - 2] + 1);
                }
                let d = d.min(over);
                cur[j] = d;
                row_min = row_min.min(d);
            }
            if let Some(cell) = cur.get_mut(hi + 1) {
                *cell = over;
            }
            if row_min > limit {
                return Ok(None);
            }
            std::mem::swap(before, prev);
            std::mem::swap(prev, cur);
        }
        let distance = prev[b.len()];
        Ok((distance <= limit).then_some(distance))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    /// The distance between `a` and `b` if it is at most `limit`.
    fn distance_within(a: &str, b: &str, limit: usize) -> Option<usize> {
        let a: Vec<char> = a.chars().collect();
        let b: Vec<char> = b.chars().collect();
        Rows::default()
            .edit_distance(&a, &b, limit, &mut Work(usize::MAX))
            .unwrap()
    }

    fn distance(a: &str, b: &str) -> usize {
        distance_within(a, b, usize::MAX / 2).unwrap()
    }

    /// The optimal string alignment distance computed over the whole table, as the search
    /// did before it was bounded.
    fn full_distance(a: &[char], b: &[char]) -> usize {
        let mut table = vec![vec![0; b.len() + 1]; a.len() + 1];
        for (i, row) in table.iter_mut().enumerate() {
            row[0] = i;
        }
        for (j, cell) in table[0].iter_mut().enumerate() {
            *cell = j;
        }
        for i in 1..=a.len() {
            for j in 1..=b.len() {
                let cost = usize::from(a[i - 1] != b[j - 1]);
                let mut d = (table[i - 1][j - 1] + cost)
                    .min(table[i - 1][j] + 1)
                    .min(table[i][j - 1] + 1);
                if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                    d = d.min(table[i - 2][j - 2] + 1);
                }
                table[i][j] = d;
            }
        }
        table[a.len()][b.len()]
    }

    /// Every string of up to four characters from `abc`.
    fn short_strings() -> Vec<Vec<char>> {
        let mut strings = vec![Vec::new()];
        let mut last = vec![Vec::new()];
        for _ in 0..4 {
            last = last
                .iter()
                .flat_map(|s: &Vec<char>| {
                    ['a', 'b', 'c'].map(|c| {
                        let mut longer = s.clone();
                        longer.push(c);
                        longer
                    })
                })
                .collect();
            strings.extend(last.iter().cloned());
        }
        strings
    }

    #[test]
    fn bounded_distance_matches_the_full_table_within_the_limit() {
        let strings = short_strings();
        let mut rows = Rows::default();
        for a in &strings {
            for b in &strings {
                let full = full_distance(a, b);
                for limit in 0..=3 {
                    let bounded = rows
                        .edit_distance(a, b, limit, &mut Work(usize::MAX))
                        .unwrap();
                    let expected = (full <= limit).then_some(full);
                    assert_eq!(bounded, expected, "{a:?} {b:?} within {limit}");
                }
            }
        }
    }

    #[test]
    fn bounded_distance_stops_once_a_row_exceeds_the_limit() {
        let a: Vec<char> = "a".repeat(100).chars().collect();
        let b: Vec<char> = "z".repeat(100).chars().collect();
        let mut work = Work(usize::MAX);
        assert_eq!(
            Rows::default().edit_distance(&a, &b, 33, &mut work),
            Ok(None)
        );
        // The first 34 rows have at most 67 cells each.
        assert!(usize::MAX - work.0 <= 34 * 67);
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
    fn closest_gives_up_once_the_work_runs_out() {
        let long: Vec<String> = (0..1000)
            .map(|i| format!("{:a<100}", format!("k{i}")))
            .collect();
        let near = format!("{}y", "z".repeat(99));
        let name = "z".repeat(100);
        assert_eq!(closest_name(&name, &[near.as_str()]), Some(near.as_str()));
        let mut candidates: Vec<&str> = long.iter().map(String::as_str).collect();
        candidates.push(&near);
        assert_eq!(closest_name(&name, &candidates), None);
    }

    #[test]
    fn closest_breaks_ties_by_byte_order() {
        assert_eq!(closest_name("jo", &["jobs", "job"]), Some("job"));
        assert_eq!(closest_name("ab", &["ac", "aa"]), Some("aa"));
        assert_eq!(closest_name("ab", &["aa", "ac"]), Some("aa"));
    }
}
