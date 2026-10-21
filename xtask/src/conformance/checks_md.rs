//! The `Example input` and `Output` blocks of upstream `docs/checks.md`, and
//! the comparison that a ticked row of the "docs/checks.md sections" table
//! in `docs/parity-checklist.md` needs.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;

use super::output::parse_line;

/// One section of `docs/checks.md`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Section {
    /// The workflow in the `Example input` block.
    pub input: Option<String>,
    /// The text of the `Output` block, which the reference printed for the
    /// input in its default format.
    pub output: Option<String>,
    /// Whether an `Output` block is marked `<!-- Skip update output -->`.
    /// Upstream writes such a block by hand and does not regenerate it.
    pub hand_written_output: bool,
}

/// The comment after `Output:` that marks a block that upstream writes by
/// hand.
const HAND_WRITTEN: &str = "Output:\n<!-- Skip update output -->\n";

/// The sections of `markdown`, by the anchor that starts each one, such as
/// `check-unexpected-keys`. A leading `#` in an anchor is left out: upstream
/// writes `<a id="#check-deprecated-workflow-commands">`, which its own
/// table-of-contents link misses.
#[must_use]
pub fn sections(markdown: &str) -> BTreeMap<String, Section> {
    markdown
        .split("<a id=\"")
        .skip(1)
        .filter_map(|part| {
            let (anchor, body) = part.split_once('"')?;
            let section = Section {
                input: block(body, "Example input:\n\n```yaml\n"),
                output: block(body, "Output:\n\n```\n"),
                hand_written_output: body.contains(HAND_WRITTEN),
            };
            Some((anchor.trim_start_matches('#').to_owned(), section))
        })
        .collect()
}

/// The text between `start` and the next line that starts a code fence.
fn block(body: &str, start: &str) -> Option<String> {
    let (_, rest) = body.split_once(start)?;
    let end = rest.find("\n```")?;
    Some(format!("{}\n", rest.get(..end)?))
}

/// A line with the `^~~~` indicator under a snippet. workflint underlines the
/// whole span where the reference underlines up to the next space
/// (`docs/differences.md#column-units`), so the comparison leaves these lines
/// out.
static INDICATOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^ *\|[ \t]*\^~*[ \t]*$").expect("the pattern is valid"));

/// The lines of `output` that the comparison uses: every line except the
/// indicators, with the code in brackets of each error line passed through
/// `kind`.
///
/// # Errors
///
/// Returns the message of `kind` for a code it rejects.
pub fn comparable_lines(
    output: &str,
    kind: impl Fn(&str) -> Result<String, String>,
) -> Result<Vec<String>, String> {
    output
        .lines()
        .filter(|line| !INDICATOR.is_match(line))
        .map(|line| match parse_line(line) {
            Ok(mut diagnostic) => {
                diagnostic.code = kind(&diagnostic.code)?;
                Ok(diagnostic.to_string())
            }
            Err(_) => Ok(line.to_owned()),
        })
        .collect()
}

/// Compares the `expected` Output block with the `actual` output of the
/// binary, without the indicator lines. `kind` maps the code in the
/// brackets of `actual`.
///
/// Each of `replacements`, an `upstream` line and the `workflint` line that
/// replaces it, takes the place of the first error line of `expected` that
/// equals its `upstream` line. The snippet lines under it stay, unless the
/// replacement moves the error to another line of `input`, the example's
/// workflow: then the snippet shows that line. A replacement whose line is
/// not in `expected` does nothing, because the allow-list records of a
/// fixture may cover lines that the example leaves out.
///
/// # Errors
///
/// Returns both sets of lines when they differ.
pub fn compare(
    expected: &str,
    actual: &str,
    replacements: &[(&str, &str)],
    input: &str,
    kind: impl Fn(&str) -> Result<String, String>,
) -> Result<(), String> {
    let mut want = comparable_lines(expected, |code| Ok(code.to_owned()))?;
    for (upstream, workflint) in replacements {
        let Some(index) = want.iter().position(|line| line == upstream) else {
            continue;
        };
        if let (Ok(old), Ok(new)) = (parse_line(upstream), parse_line(workflint))
            && old.line != new.line
        {
            move_snippet(&mut want, index + 1, new.line, input);
        }
        if let Some(line) = want.get_mut(index) {
            (*workflint).clone_into(line);
        }
    }
    let have = comparable_lines(actual, kind)?;
    if want == have {
        return Ok(());
    }
    Err(format!(
        "the Output block, without its ^~~~ lines, is:\n{}\nbut the binary printed:\n{}",
        want.join("\n"),
        have.join("\n")
    ))
}

/// Rewrites the snippet that starts at `want[start]`, the lines up to the
/// next error line, to show line `line` of `input`: the bar line with a
/// gutter as wide as the line number, and the numbered source line.
fn move_snippet(want: &mut [String], start: usize, line: u32, input: &str) {
    let source = usize::try_from(line)
        .ok()
        .and_then(|line| input.lines().nth(line.checked_sub(1)?))
        .unwrap_or_default();
    let gutter = " ".repeat(line.to_string().len());
    for text in want.iter_mut().skip(start) {
        if parse_line(text).is_ok() {
            break;
        }
        let trimmed = text.trim_start();
        if trimmed == "|" {
            *text = format!("{gutter} |");
        } else if trimmed
            .split_once(" | ")
            .is_some_and(|(number, _)| number.bytes().all(|byte| byte.is_ascii_digit()))
        {
            *text = format!("{line} | {source}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHECKS: &str = "\
<a id=\"check-a\"></a>
## A

Example input:

```yaml
on: push
```

Output:

```
test.yaml:1:1: bad [syntax-check]
  |
1 | on: push
  | ^~~
```

<a id=\"check-b\"></a>
## B

Only prose.

<a id=\"check-c\"></a>
## C

Example input:

```yaml
on: push
```

Output:
<!-- Skip update output -->

```
/home/me/test.yaml:1:1: bad [syntax-check]
```
";

    #[test]
    fn a_leading_hash_is_left_out_of_an_anchor() {
        let sections = sections("<a id=\"#check-d\"></a>\n## D\n");
        assert_eq!(sections.keys().collect::<Vec<_>>(), ["check-d"]);
    }

    #[test]
    fn sections_have_their_input_and_output() {
        let sections = sections(CHECKS);
        assert_eq!(
            sections["check-a"],
            Section {
                input: Some("on: push\n".to_owned()),
                output: Some(
                    "test.yaml:1:1: bad [syntax-check]\n  |\n1 | on: push\n  | ^~~\n".to_owned()
                ),
                hand_written_output: false,
            }
        );
        assert_eq!(sections["check-b"], Section::default());
        assert_eq!(
            sections["check-c"],
            Section {
                input: Some("on: push\n".to_owned()),
                output: None,
                hand_written_output: true,
            }
        );
    }

    fn kind(code: &str) -> Result<String, String> {
        match code {
            "some-rule" => Ok("syntax-check".to_owned()),
            other => Err(format!("no kind for {other}")),
        }
    }

    #[test]
    fn output_matches_with_rule_ids_mapped_and_indicators_left_out() {
        let expected = "test.yaml:1:1: bad [syntax-check]\n  |\n1 | on: push\n  | ^~~\n";
        let actual = "test.yaml:1:1: bad [some-rule]\n  |\n1 | on: push\n  | ^~~~~~~~\n";
        assert_eq!(compare(expected, actual, &[], "on: push\n", kind), Ok(()));
    }

    #[test]
    fn another_message_or_snippet_line_is_a_difference() {
        let expected = "test.yaml:1:1: bad [syntax-check]\n  |\n1 | on: push\n  | ^~~\n";
        let actual = "test.yaml:1:1: worse [some-rule]\n  |\n1 | on: push\n  | ^~~\n";
        let error = compare(expected, actual, &[], "on: push\n", kind).unwrap_err();
        assert!(
            error.contains("test.yaml:1:1: worse [syntax-check]"),
            "{error}"
        );
        let moved = "test.yaml:1:1: bad [some-rule]\n  |\n1 | on: pull\n  | ^~~\n";
        assert!(compare(expected, moved, &[], "on: push\n", kind).is_err());
    }

    #[test]
    fn a_replacement_changes_the_expected_line_and_keeps_its_snippet() {
        let expected = "test.yaml:1:1: bad [syntax-check]\n  |\n1 | on: push\n  | ^~~\n";
        let actual = "test.yaml:1:1: better [some-rule]\n  |\n1 | on: push\n  | ^~~\n";
        let replacements = [
            (
                "test.yaml:1:1: bad [syntax-check]",
                "test.yaml:1:1: better [syntax-check]",
            ),
            (
                "test.yaml:9:9: absent [syntax-check]",
                "test.yaml:9:9: x [syntax-check]",
            ),
        ];
        assert_eq!(
            compare(expected, actual, &replacements, "on: push\n", kind),
            Ok(())
        );
        assert!(compare(expected, actual, &[], "on: push\n", kind).is_err());
    }

    #[test]
    fn a_replacement_on_another_line_shows_that_line_in_the_snippet() {
        let input = "on: push\njobs:\n  a:\n    runs-on: x\n    steps:\n      - run: |\n          a\n          b\n          c\n          d\n";
        let expected = "test.yaml:6:9: bad [syntax-check]\n  |\n6 |       - run: |\n  | ^~~\n";
        let actual = "test.yaml:10:11: bad [some-rule]\n   |\n10 |           d\n   |           ^\n";
        let replacements = [(
            "test.yaml:6:9: bad [syntax-check]",
            "test.yaml:10:11: bad [syntax-check]",
        )];
        assert_eq!(
            compare(expected, actual, &replacements, input, kind),
            Ok(())
        );
    }

    #[test]
    fn an_unknown_rule_id_is_an_error() {
        let error = compare("", "test.yaml:1:1: bad [other]\n", &[], "", kind).unwrap_err();
        assert_eq!(error, "no kind for other");
    }
}
