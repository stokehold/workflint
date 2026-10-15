//! Runs the event parser on the YAML test suite, as upstream libyaml-safer
//! does in `tests/test_parser.rs` and `tests/test_parser_error.rs`.
//!
//! A case without an `error` file must produce the events in its
//! `test.event` file. A case with one must fail to parse. The ignore lists are
//! upstream's: they name the cases where libyaml itself disagrees with the
//! suite. Those cases must still disagree, so that a fix shows up as a test
//! failure and the list stays accurate.

use std::fmt::Write as _;
use std::fs;

use datatest_stable::Utf8Path;
use workflint_yaml::{EventData, EventParser, ScalarStyle};

const PARSER_IGNORELIST: &str = include_str!("libyaml-parser");
const PARSER_ERROR_IGNORELIST: &str = include_str!("libyaml-parser-error");

fn is_listed(ignorelist: &str, id: &str) -> bool {
    ignorelist
        .lines()
        .any(|line| line.split(':').next() == Some(id))
}

/// Renders events in the yaml-test-suite `test.event` format, stopping at
/// the first error.
fn render_events(input: &str) -> Result<String, workflint_yaml::Error> {
    let mut out = String::new();
    for event in EventParser::new(input) {
        let event = event?;
        match event.data {
            EventData::StreamStart { .. } => out.push_str("+STR\n"),
            EventData::StreamEnd => out.push_str("-STR\n"),
            EventData::DocumentStart { implicit, .. } => {
                out.push_str(if implicit { "+DOC\n" } else { "+DOC ---\n" });
            }
            EventData::DocumentEnd { implicit } => {
                out.push_str(if implicit { "-DOC\n" } else { "-DOC ...\n" });
            }
            EventData::Alias { anchor } => {
                let _ = writeln!(out, "=ALI *{anchor}");
            }
            EventData::Scalar {
                anchor,
                tag,
                value,
                style,
                ..
            } => {
                out.push_str("=VAL");
                push_properties(&mut out, anchor.as_deref(), tag.as_deref());
                out.push_str(match style {
                    ScalarStyle::Plain => " :",
                    ScalarStyle::SingleQuoted => " '",
                    ScalarStyle::DoubleQuoted => " \"",
                    ScalarStyle::Literal => " |",
                    ScalarStyle::Folded => " >",
                    _ => " ?",
                });
                push_escaped(&mut out, &value);
                out.push('\n');
            }
            EventData::SequenceStart { anchor, tag, .. } => {
                out.push_str("+SEQ");
                push_properties(&mut out, anchor.as_deref(), tag.as_deref());
                out.push('\n');
            }
            EventData::SequenceEnd => out.push_str("-SEQ\n"),
            EventData::MappingStart { anchor, tag, .. } => {
                out.push_str("+MAP");
                push_properties(&mut out, anchor.as_deref(), tag.as_deref());
                out.push('\n');
            }
            EventData::MappingEnd => out.push_str("-MAP\n"),
        }
    }
    Ok(out)
}

fn push_properties(out: &mut String, anchor: Option<&str>, tag: Option<&str>) {
    if let Some(anchor) = anchor {
        let _ = write!(out, " &{anchor}");
    }
    if let Some(tag) = tag {
        let _ = write!(out, " <{tag}>");
    }
}

fn push_escaped(out: &mut String, value: &str) {
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\0' => out.push_str("\\0"),
            '\x08' => out.push_str("\\b"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch => out.push(ch),
        }
    }
}

// datatest-stable passes the file contents as an owned `String`.
#[allow(clippy::needless_pass_by_value)]
fn yaml_test_suite_case(path: &Utf8Path, input: String) -> datatest_stable::Result<()> {
    let dir = path.parent().ok_or("case file has no directory")?;
    let id = dir.file_name().ok_or("case directory has no name")?;

    if dir.join("error").exists() {
        let ignored = is_listed(PARSER_ERROR_IGNORELIST, id);
        match (render_events(&input), ignored) {
            (Err(_), false) | (Ok(_), true) => Ok(()),
            (Ok(events), false) => Err(format!("expected a parse error, got:\n{events}").into()),
            (Err(err), true) => Err(format!(
                "{id} now fails as expected ({err}); remove it from tests/libyaml-parser-error"
            )
            .into()),
        }
    } else {
        let expected = fs::read_to_string(dir.join("test.event"))?;
        let ignored = is_listed(PARSER_IGNORELIST, id);
        let actual = render_events(&input);
        let matches = actual.as_ref().is_ok_and(|events| *events == expected);
        match (matches, ignored) {
            (true, false) | (false, true) => Ok(()),
            (true, true) => {
                Err(format!("{id} now passes; remove it from tests/libyaml-parser").into())
            }
            (false, false) => Err(match actual {
                Ok(events) => format!("expected:\n{expected}\nactual:\n{events}"),
                Err(err) => format!("expected:\n{expected}\nactual: {err}"),
            }
            .into()),
        }
    }
}

datatest_stable::harness! {
    { test = yaml_test_suite_case, root = "tests/yaml-test-suite", pattern = r"/in\.yaml$" },
}
