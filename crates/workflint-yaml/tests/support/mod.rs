//! Event dumps shared by the snapshot tests.

use std::fmt::Write as _;

use workflint_yaml::{Event, EventParser, Mark};

/// Renders a mark as `line:column@index`, with 0-based line and column.
fn mark(mark: Mark) -> String {
    format!("{}:{}@{}", mark.line, mark.column, mark.index)
}

/// Renders one event per line with its start and end marks and all of its
/// data.
fn event_line(event: &Event) -> String {
    format!(
        "{}..{} {:?}",
        mark(event.start_mark),
        mark(event.end_mark),
        event.data
    )
}

/// Parses `input` and renders every event, stopping at the first error,
/// which becomes the last line.
pub(crate) fn dump_events(input: &str) -> String {
    let mut out = String::new();
    for event in EventParser::new(input) {
        match event {
            Ok(event) => {
                let _ = writeln!(out, "{}", event_line(&event));
            }
            Err(err) => {
                let _ = writeln!(out, "error: {err}");
                break;
            }
        }
    }
    out
}
