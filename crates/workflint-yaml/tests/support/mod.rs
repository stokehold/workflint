//! Event dumps shared by the snapshot tests.

use std::fmt::Write as _;

use workflint_yaml::event::{CommentKind, Event, EventData, EventParser, Mark};

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

/// Parses `input` and renders every event except comments, stopping at the
/// first error, which becomes the last line.
pub(crate) fn dump_events(input: &str) -> String {
    let mut out = String::new();
    for event in EventParser::new(input) {
        match event {
            Ok(Event {
                data: EventData::Comment { .. },
                ..
            }) => {}
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

/// Parses `input` and renders each comment event with its kind and text,
/// stopping at the first error.
///
/// Fails the test if a comment's span does not lie on character boundaries
/// in `input`, does not cover exactly one `#` comment without its line break,
/// comes before the previous comment, or has the wrong kind.
pub(crate) fn dump_comments(input: &str) -> String {
    let mut out = String::new();
    let mut previous_end = 0;
    for event in EventParser::new(input) {
        let Ok(event) = event else { break };
        let EventData::Comment { kind } = event.data else {
            continue;
        };
        let start = usize::try_from(event.start_mark.index).unwrap_or(usize::MAX);
        let end = usize::try_from(event.end_mark.index).unwrap_or(usize::MAX);
        assert!(
            input.get(start..end).is_some(),
            "comment span {start}..{end} is not on character boundaries"
        );
        let text = &input[start..end];
        assert!(
            text.starts_with('#'),
            "comment {text:?} does not start with #"
        );
        assert!(
            !text.contains(is_line_break),
            "comment {text:?} contains a line break"
        );
        assert!(
            end == input.len() || input[end..].starts_with(is_line_break),
            "comment {text:?} does not end at a line break"
        );
        assert!(start >= previous_end, "comment {text:?} is out of order");
        previous_end = end;
        assert_eq!(kind, kind_from_source(input, start), "kind of {text:?}");
        let _ = writeln!(
            out,
            "{}..{} {kind:?} {text:?}",
            mark(event.start_mark),
            mark(event.end_mark)
        );
    }
    out
}

fn is_line_break(ch: char) -> bool {
    matches!(ch, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

/// Finds the kind of the comment at `start` from the text before it on its
/// line, as an independent check of the scanner.
fn kind_from_source(input: &str, start: usize) -> CommentKind {
    let line_start = input[..start]
        .char_indices()
        .rev()
        .find(|&(_, ch)| is_line_break(ch))
        .map_or(0, |(i, ch)| i + ch.len_utf8());
    let before = &input[line_start..start];
    if before
        .chars()
        .all(|ch| matches!(ch, ' ' | '\t' | '\u{feff}'))
    {
        CommentKind::OwnLine
    } else {
        CommentKind::Trailing
    }
}
