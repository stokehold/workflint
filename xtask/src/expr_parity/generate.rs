//! Random expressions from a fixed seed: the `generated` expression set.
//!
//! They mix context names, property and index access, object filters,
//! function calls, literals, and operators, in spellings that the
//! hand-written cases do not try. Every comparison and logical operation is
//! in parentheses, so the documented difference in comparison precedence
//! does not show up. The same seed gives the same expressions on every
//! machine.

/// The seed of the `generated` set.
pub(crate) const SEED: u64 = 129;

/// The number of expressions in the `generated` set.
pub(crate) const COUNT: usize = 2_000;

const CONTEXTS: [&str; 14] = [
    "github", "env", "vars", "secrets", "job", "runner", "strategy", "matrix", "steps", "needs",
    "inputs", "jobs", "foo", "GitHub",
];

const PROPERTIES: [&str; 35] = [
    "event",
    "ref",
    "issue",
    "title",
    "body",
    "commits",
    "pages",
    "page_name",
    "message",
    "author",
    "name",
    "head_ref",
    "pull_request",
    "head",
    "label",
    "repo",
    "default_branch",
    "services",
    "ports",
    "container",
    "id",
    "status",
    "os",
    "fail-fast",
    "job-index",
    "a",
    "b",
    "x",
    "inputs",
    "outputs",
    "result",
    "FOO",
    "github_x",
    "head_commit",
    "email",
];

const FUNCTIONS: [&str; 16] = [
    "contains",
    "startsWith",
    "endsWith",
    "format",
    "join",
    "toJSON",
    "fromJSON",
    "hashFiles",
    "success",
    "always",
    "cancelled",
    "failure",
    "case",
    "foo",
    "Contains",
    "FORMAT",
];

const LITERALS: [&str; 14] = [
    "'a'",
    "'{0}'",
    "'{0} {1}'",
    "'{{0}}'",
    "1",
    "0",
    "1.5",
    "true",
    "false",
    "null",
    "'[1]'",
    r#"'{"a":1}'"#,
    r#"'[{"a":[1]}]'"#,
    r#"'{"A":{"b":true}}'"#,
];

const SUFFIXES: [&str; 5] = [".a", ".*", "[0]", "['a']", ".b.c"];

const COMPARISONS: [&str; 6] = ["==", "!=", "<", "<=", ">", ">="];

/// The `SplitMix64` generator, which is small and good enough here.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number in `0..n`.
    fn below(&mut self, n: usize) -> usize {
        // `n` is small, so the modulo bias does not matter.
        usize::try_from(self.next() % n as u64).unwrap_or(0)
    }

    /// True with a chance of `percent` in 100.
    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

/// Generates [`COUNT`] expressions from [`SEED`].
pub(crate) fn expressions() -> Vec<String> {
    let mut random = Random(SEED);
    (0..COUNT).map(|_| expression(&mut random, 0)).collect()
}

fn expression(random: &mut Random, depth: usize) -> String {
    let roll = random.below(100);
    if depth > 4 || roll < 25 {
        return leaf(random, depth);
    }
    if roll < 50 {
        let mut text = random.pick(&FUNCTIONS).to_owned();
        let args: Vec<String> = (0..random.below(5))
            .map(|_| expression(random, depth + 1))
            .collect();
        text.push('(');
        text.push_str(&args.join(", "));
        text.push(')');
        if random.chance(30) {
            text.push_str(random.pick(&SUFFIXES));
        }
        return text;
    }
    if roll < 60 {
        return format!("!{}", expression(random, depth + 1));
    }
    let op = if roll < 80 {
        random.pick(&COMPARISONS)
    } else {
        random.pick(&["&&", "||"])
    };
    let left = expression(random, depth + 1);
    let right = expression(random, depth + 1);
    format!("({left} {op} {right})")
}

/// A literal, or a context with up to four property, filter, or index
/// accesses.
fn leaf(random: &mut Random, depth: usize) -> String {
    if random.chance(50) {
        return random.pick(&LITERALS).to_owned();
    }
    let mut text = random.pick(&CONTEXTS).to_owned();
    for _ in 0..random.below(5) {
        match random.below(100) {
            0..60 => {
                text.push('.');
                text.push_str(random.pick(&PROPERTIES));
            }
            60..75 => text.push_str(".*"),
            75..88 => {
                text.push_str("['");
                text.push_str(random.pick(&PROPERTIES));
                text.push_str("']");
            }
            _ => {
                let index = match random.below(6) {
                    0 => "0".to_owned(),
                    1 => "1".to_owned(),
                    2 => "'0'".to_owned(),
                    3 => "null".to_owned(),
                    4 => "true".to_owned(),
                    _ => expression(random, depth + 1),
                };
                text.push('[');
                text.push_str(&index);
                text.push(']');
            }
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_generated_set_is_the_same_on_every_run() {
        let first = expressions();
        assert_eq!(first.len(), COUNT);
        assert_eq!(first, expressions());
    }

    #[test]
    fn the_generated_set_parses_and_varies() {
        let expressions = expressions();
        let distinct: std::collections::BTreeSet<_> = expressions.iter().collect();
        assert!(distinct.len() > COUNT / 2, "{} distinct", distinct.len());
        let parsed = expressions
            .iter()
            .filter(|e| workflint_expr::parse_expression(&format!("{e}}}}}")).is_ok())
            .count();
        assert_eq!(parsed, COUNT);
    }
}
