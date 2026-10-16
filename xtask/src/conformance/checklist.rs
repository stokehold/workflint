//! Reads the tables in `docs/parity-checklist.md`.

use std::collections::BTreeMap;

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

    #[test]
    fn a_new_table_reads_its_own_header() {
        let rows = markdown_rows("| A |\n|---|\n| 1 |\n\n| B |\n|---|\n| 2 |\n");
        assert_eq!(rows[0].get("A").map(String::as_str), Some("1"));
        assert_eq!(rows[1].get("B").map(String::as_str), Some("2"));
        assert!(!rows[1].contains_key("A"));
    }
}
