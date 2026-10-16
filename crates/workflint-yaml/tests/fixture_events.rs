//! Snapshots of the full event stream of each YAML file in the reference's
//! fixtures, vendored in `tests/conformance/actionlint/testdata/`, with the
//! comment events in place and their text.
//!
//! Apart from the comments, which go-yaml attaches to nodes instead, the
//! events and their lines and columns are go-yaml's.

use std::fmt::Write as _;

use datatest_stable::Utf8Path;
use workflint_yaml::event::{EventData, EventParser, Mark};

const ROOT: &str = "../../tests/conformance/actionlint/testdata";

/// Renders a mark as `line:column@index`, with 0-based line and column.
fn mark(mark: Mark) -> String {
    format!("{}:{}@{}", mark.line, mark.column, mark.index)
}

/// Renders one event per line, up to the end of the stream or the first
/// error. A comment event shows its kind and its text.
fn dump(input: &str) -> String {
    let mut out = String::new();
    for event in EventParser::new(input) {
        let event = match event {
            Ok(event) => event,
            Err(err) => {
                let _ = writeln!(out, "error: {err}");
                break;
            }
        };
        let marks = format!("{}..{}", mark(event.start_mark), mark(event.end_mark));
        if let EventData::Comment { kind } = event.data {
            let text = usize::try_from(event.start_mark.index)
                .ok()
                .zip(usize::try_from(event.end_mark.index).ok())
                .and_then(|(start, end)| input.get(start..end))
                .unwrap_or("<not on character boundaries>");
            let _ = writeln!(out, "{marks} Comment {kind:?} {text:?}");
        } else {
            let _ = writeln!(out, "{marks} {:?}", event.data);
        }
    }
    out
}

// datatest-stable passes the file contents as an owned `String`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn fixture_events(path: &Utf8Path, input: String) -> datatest_stable::Result<()> {
    let fixture = path.strip_prefix(ROOT).unwrap_or(path);
    let name: String = fixture
        .as_str()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect();
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path("snapshots/fixture-events");
    settings.set_prepend_module_to_snapshot(false);
    settings.set_omit_expression(true);
    settings.set_description(fixture.as_str());
    settings.bind(|| {
        insta::assert_snapshot!(format!("events__{name}"), dump(&input));
    });
    Ok(())
}

datatest_stable::harness! {
    { test = fixture_events, root = "../../tests/conformance/actionlint/testdata", pattern = r"\.ya?ml$" },
}
