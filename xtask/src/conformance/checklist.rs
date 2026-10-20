//! Reads the tables in `docs/parity-checklist.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use regex::Regex;

/// One item of the checklist: a table row with a checkbox.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Item {
    /// The line of the row in the file, from 1.
    pub line: usize,
    /// Whether the box is ticked.
    pub ticked: bool,
    /// The number of the latest milestone in the "Milestone" cell, such as
    /// 4 for `M2, M4`.
    pub milestone: Option<u32>,
    /// The fixture IDs in the "Upstream fixtures" cell, such as `err/name`.
    pub fixtures: Vec<String>,
    /// The fixtures that the notes say the item waits on, each with the
    /// number of the milestone given in brackets, as in
    /// ``the row waits on `err/name` (M4)``.
    pub waits_on: Vec<(String, u32)>,
    /// The anchor in the "Anchor" cell of the docs/checks.md sections, such
    /// as `check-unexpected-keys`.
    pub anchor: Option<String>,
}

static FIXTURE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"`((?:workflint/)?(?:err|examples|ok|projects)/[^`]+)`")
        .expect("the pattern is valid")
});

static WAITING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`([^`]+)` \(M(\d+)\)").expect("the pattern is valid"));

static MILESTONE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bM(\d+)\b").expect("the pattern is valid"));

/// The number of a milestone such as `M4`.
#[must_use]
pub fn milestone_number(text: &str) -> Option<u32> {
    text.strip_prefix('M')
        .filter(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|number| number.parse().ok())
}

/// Every item of every table in `markdown`.
///
/// # Errors
///
/// Returns one line for each row with another number of cells than its
/// header. GitHub drops the cells past the header's, so a stray `|` hides
/// the notes.
pub fn items(markdown: &str) -> Result<Vec<Item>, String> {
    let mut items = Vec::new();
    let mut problems = Vec::new();
    let mut header: Option<Vec<String>> = None;
    for (index, line) in markdown.lines().enumerate() {
        let line = line.trim();
        if !line.starts_with('|') {
            header = None;
            continue;
        }
        let cells = split_row(line);
        let Some(names) = &header else {
            header = Some(cells);
            continue;
        };
        if cells.iter().all(|cell| is_delimiter(cell)) {
            continue;
        }
        if cells.len() != names.len() {
            problems.push(format!(
                "docs/parity-checklist.md:{}: the row has {} cells, but its table has {} columns",
                index + 1,
                cells.len(),
                names.len()
            ));
            continue;
        }
        let row: BTreeMap<&str, &str> = names
            .iter()
            .map(String::as_str)
            .zip(cells.iter().map(String::as_str))
            .collect();
        let first = cells.first().map_or("", String::as_str);
        let ticked = match first.get(..5) {
            Some("- [x]") => true,
            Some("- [ ]") => false,
            _ => continue,
        };
        let cell = |name: &str| row.get(name).copied().unwrap_or_default();
        let notes = cell("Notes");
        let waits_on = notes
            .split_once("waits on")
            .map_or_else(Vec::new, |(_, rest)| {
                WAITING
                    .captures_iter(rest)
                    .filter_map(|captures| {
                        let milestone = captures[2].parse().ok()?;
                        Some((captures[1].to_owned(), milestone))
                    })
                    .collect()
            });
        items.push(Item {
            line: index + 1,
            ticked,
            milestone: MILESTONE
                .captures_iter(cell("Milestone"))
                .filter_map(|captures| captures[1].parse().ok())
                .max(),
            fixtures: FIXTURE
                .captures_iter(cell("Upstream fixtures"))
                .map(|captures| captures[1].to_owned())
                .collect(),
            waits_on,
            anchor: cell("Anchor")
                .trim_matches('`')
                .strip_prefix("checks.md#")
                .map(str::to_owned),
        });
    }
    if problems.is_empty() {
        Ok(items)
    } else {
        Err(problems.join("\n"))
    }
}

/// Every body row of every Markdown table in `markdown`, as a map from column
/// header to cell text. `\|` inside a cell is not a column separator.
#[must_use]
pub fn markdown_rows(markdown: &str) -> Vec<BTreeMap<String, String>> {
    let mut rows = Vec::new();
    let mut header: Option<Vec<String>> = None;
    for line in markdown.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            header = None;
            continue;
        }
        let cells = split_row(line);
        match &header {
            None => header = Some(cells),
            Some(_) if cells.iter().all(|cell| is_delimiter(cell)) => {}
            Some(names) => rows.push(names.iter().cloned().zip(cells).collect()),
        }
    }
    rows
}

/// Checks what the boxes of `items` claim, given the IDs of every fixture in
/// `fixtures`, the fixtures listed in `expected-failures.toml` in `failing`,
/// and the `closed` milestones:
///
/// - Every fixture of a ticked item exists and passes, so is not listed.
/// - No item that is not ticked has a closed milestone. Its milestone has
///   passed, so the item would wait on a later milestone without saying so.
///
/// # Errors
///
/// Returns one line per problem.
pub fn check_items(
    items: &[Item],
    fixtures: &BTreeSet<String>,
    failing: &BTreeSet<&str>,
    closed: &BTreeSet<u32>,
) -> Result<(), String> {
    let mut problems = Vec::new();
    for item in items {
        let at = format!("docs/parity-checklist.md:{}", item.line);
        if !item.ticked {
            if let Some(milestone) = item.milestone.filter(|m| closed.contains(m)) {
                problems.push(format!(
                    "{at}: the row is open, but its milestone M{milestone} is closed. Tick it, or give it the milestone it waits on and say so in the notes."
                ));
            }
            continue;
        }
        for fixture in &item.fixtures {
            if !fixtures.contains(fixture) {
                problems.push(format!(
                    "{at}: the row is ticked, but its fixture {fixture} does not exist"
                ));
            } else if failing.contains(fixture.as_str()) {
                problems.push(format!(
                    "{at}: the row is ticked, but its fixture {fixture} is in tests/conformance/expected-failures.toml. Untick the row, or make the fixture pass."
                ));
            }
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

fn split_row(line: &str) -> Vec<String> {
    let inner = line.strip_prefix('|').unwrap_or(line);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                cell.push('\\');
                cell.push('|');
                chars.next();
            }
            '|' => cells.push(std::mem::take(&mut cell).trim().to_owned()),
            _ => cell.push(c),
        }
    }
    cells.push(cell.trim().to_owned());
    cells
}

fn is_delimiter(cell: &str) -> bool {
    !cell.is_empty() && cell.chars().all(|c| matches!(c, '-' | ':'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_map_headers_to_cells() {
        let rows = markdown_rows("| A | B |\n|---|:-:|\n| 1 | 2 |\n| 3 | 4 |\n");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["A"], "1");
        assert_eq!(rows[1]["B"], "4");
    }

    #[test]
    fn an_escaped_pipe_stays_in_its_cell() {
        let rows = markdown_rows("| A | B |\n|---|---|\n| x \\|\\| y | 2 |\n");
        assert_eq!(rows[0]["A"], "x \\|\\| y");
        assert_eq!(rows[0]["B"], "2");
    }

    const TABLE: &str = "\
| Item | Milestone | Upstream fixtures | Notes |
|---|---|---|---|
| - [x] a | M2 | `err/a`, `ok/b.yaml`, `workflint/err/c` |  |
| - [ ] b | M2, M5 | all | Moved from M2: the row waits on `err/a` (M4), `examples/x` (M5). |
| not an item | M1 | `err/z` |  |
";

    #[test]
    fn items_read_the_box_milestone_fixtures_and_waits() {
        let items = items(TABLE).unwrap();
        assert_eq!(
            items,
            [
                Item {
                    line: 3,
                    ticked: true,
                    milestone: Some(2),
                    fixtures: vec!["err/a".into(), "ok/b.yaml".into(), "workflint/err/c".into()],
                    ..Item::default()
                },
                Item {
                    line: 4,
                    ticked: false,
                    milestone: Some(5),
                    waits_on: vec![("err/a".into(), 4), ("examples/x".into(), 5)],
                    ..Item::default()
                },
            ]
        );
    }

    #[test]
    fn a_row_with_an_extra_cell_is_an_error() {
        let markdown = "| Item | Notes |\n|---|---|\n| - [ ] a || note |\n";
        let error = items(markdown).unwrap_err();
        assert!(
            error.contains(":3: the row has 3 cells, but its table has 2 columns"),
            "{error}"
        );
    }

    #[test]
    fn a_checks_md_row_has_its_anchor() {
        let markdown = "| Section | Anchor | Milestone |\n|---|---|---|\n| - [x] A | `checks.md#check-a` | M2 |\n";
        assert_eq!(
            items(markdown).unwrap()[0].anchor.as_deref(),
            Some("check-a")
        );
    }

    fn check(items: &[Item], failing: &[&str]) -> Result<(), String> {
        let fixtures = ["err/a", "err/b"].map(str::to_owned).into();
        check_items(
            items,
            &fixtures,
            &failing.iter().copied().collect(),
            &BTreeSet::from([1, 2, 3]),
        )
    }

    fn item(ticked: bool, milestone: u32, fixtures: &[&str]) -> Item {
        Item {
            line: 7,
            ticked,
            milestone: Some(milestone),
            fixtures: fixtures.iter().map(|f| (*f).to_owned()).collect(),
            ..Item::default()
        }
    }

    #[test]
    fn a_ticked_row_whose_fixtures_pass_is_fine() {
        assert_eq!(check(&[item(true, 2, &["err/a"])], &["err/b"]), Ok(()));
    }

    #[test]
    fn a_ticked_row_with_a_failing_fixture_fails() {
        let error = check(&[item(true, 2, &["err/a", "err/b"])], &["err/b"]).unwrap_err();
        assert_eq!(
            error,
            "docs/parity-checklist.md:7: the row is ticked, but its fixture err/b is in tests/conformance/expected-failures.toml. Untick the row, or make the fixture pass."
        );
    }

    #[test]
    fn a_ticked_row_with_a_missing_fixture_fails() {
        let error = check(&[item(true, 2, &["err/gone"])], &[]).unwrap_err();
        assert!(error.contains("err/gone does not exist"), "{error}");
    }

    #[test]
    fn an_open_row_of_a_closed_milestone_fails() {
        for fixtures in [&["err/a"][..], &["err/b"], &[]] {
            let error = check(&[item(false, 3, fixtures)], &["err/b"]).unwrap_err();
            assert_eq!(
                error,
                "docs/parity-checklist.md:7: the row is open, but its milestone M3 is closed. Tick it, or give it the milestone it waits on and say so in the notes."
            );
        }
        assert_eq!(check(&[item(false, 4, &["err/b"])], &["err/b"]), Ok(()));
    }

    #[test]
    fn milestone_numbers_need_the_m_and_digits() {
        assert_eq!(milestone_number("M4"), Some(4));
        assert_eq!(milestone_number("M"), None);
        assert_eq!(milestone_number("4"), None);
        assert_eq!(milestone_number("M4.1"), None);
    }

    #[test]
    fn a_new_table_reads_its_own_header() {
        let rows = markdown_rows("| A |\n|---|\n| 1 |\n\n| B |\n|---|\n| 2 |\n");
        assert_eq!(rows[0].get("A").map(String::as_str), Some("1"));
        assert_eq!(rows[1].get("B").map(String::as_str), Some("2"));
        assert!(!rows[1].contains_key("A"));
    }
}
