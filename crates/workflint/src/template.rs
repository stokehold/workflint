//! `-format`: prints the diagnostics of a run through a Go template, with the fields and
//! functions of actionlint's `ErrorFormatter` (`error.go:182-318`).
//!
//! The template runs once, over the list of the diagnostics of every file. Each diagnostic is a
//! struct with the fields `Message`, `Filepath`, `Line`, `Column`, `Kind`, `Snippet`, and
//! `EndColumn` of the reference, and `Rule`, which holds workflint's rule ID
//! (`docs/differences.md#rule-id-in-brackets`). The functions are `json`, `replace`,
//! `toPascalCase`, `getVersion`, and `allKinds`. The values carry the reference's Go type names,
//! so that errors name the same types.

use std::fmt::Write as _;
use std::io::{self, Write};
use std::sync::{Arc, OnceLock};

use workflint_diagnostics::quote::quote;
use workflint_diagnostics::{Diagnostic, SourceFile, snippet};
use workflint_gotemplate::{Error, Function, Functions, List, Param, Struct, Template, Value};
use workflint_linter::Kind;

use crate::version::VERSION;

/// The name of the template, which its errors print.
const NAME: &str = "error formatter";
/// The Go type of a diagnostic.
const FIELDS_TYPE: &str = "*actionlint.ErrorTemplateFields";
/// The Go type of the list that the template runs over.
const LIST_TYPE: &str = "[]*actionlint.ErrorTemplateFields";
/// The Go type of a kind that `allKinds` returns.
const KIND_TYPE: &str = "*actionlint.ruleTemplateFields";
/// The Go type of the list that `allKinds` returns.
const KINDS_TYPE: &str = "[]*actionlint.ruleTemplateFields";

/// The JSON keys of the fields of a diagnostic, in the reference's order, and whether the key is
/// left out when the field is empty (`omitempty`). `Rule` has no key, so that `{{json .}}` prints
/// the reference's seven keys.
const JSON_KEYS: [(&str, &str, bool); 7] = [
    ("Message", "message", false),
    ("Filepath", "filepath", true),
    ("Line", "line", false),
    ("Column", "column", false),
    ("Kind", "kind", false),
    ("Snippet", "snippet", true),
    ("EndColumn", "end_column", false),
];

/// A parsed `-format` template with actionlint's functions.
#[derive(Debug)]
pub(crate) struct Formatter {
    template: Template,
    /// The kinds that `allKinds` returns, set once the rules of the run are known.
    kinds: Arc<OnceLock<Vec<&'static Kind>>>,
}

/// An error of [`Formatter::print`].
#[derive(Debug)]
pub(crate) enum PrintError {
    /// The template failed. The message is the one to print.
    Template(String),
    /// Writing the output failed.
    Write(io::Error),
}

impl Formatter {
    /// Parses `format`, the value of `-format`, after unescaping its backslash escapes, as
    /// `NewErrorFormatter` does (`error.go:252-290`). The error is the message to print.
    pub(crate) fn new(format: &str) -> Result<Self, String> {
        if !format.contains("{{") {
            return Err(format!(
                "template to format error messages must contain at least one {{{{ }}}} \
                 placeholder: {format}"
            ));
        }
        let kinds = Arc::new(OnceLock::new());
        let template = Template::parse(NAME, &unescape_backslash(format), functions(&kinds))
            .map_err(|err| {
                format!(
                    "template {} to format error messages could not be parsed: {err}",
                    quote(format)
                )
            })?;
        Ok(Self { template, kinds })
    }

    /// Sets the kinds that `allKinds` returns: those of the rules that the run uses.
    pub(crate) fn set_kinds(&self, kinds: Vec<&'static Kind>) {
        // A second call would change nothing that the template has seen.
        let _ = self.kinds.set(kinds);
    }

    /// Runs the template over `fields`, the diagnostics of every file as [`fields`] returns
    /// them, and writes the output to `out`. After an error, `out` has the output up to the
    /// failing action.
    pub(crate) fn print(&self, out: &mut impl Write, fields: Vec<Value>) -> Result<(), PrintError> {
        let data = Value::List(List::new(LIST_TYPE, fields));
        self.template.execute(out, &data).map_err(|err| match err {
            Error::Write(err) => PrintError::Write(err),
            err => PrintError::Template(format!("could not format error messages: {err}")),
        })
    }
}

/// The template values of the diagnostics of `file` (`error.go:67-93`).
///
/// `Snippet` is the source line, a line break, and the indicator of the default format, or
/// empty when the line does not exist. `EndColumn` is the column of the last character that the
/// indicator covers, counted in code points as `Column` is, or `Column` when there is no snippet.
pub(crate) fn fields(file: &SourceFile, diagnostics: &[Diagnostic]) -> Vec<Value> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            let pos = file.line_col(diagnostic.span.start);
            let (snippet, end_column) = snippet(file, diagnostic).map_or_else(
                || (String::new(), pos.column),
                |snippet| {
                    (
                        format!("{}\n{}", snippet.line, snippet.indicator),
                        snippet.end_column,
                    )
                },
            );
            Value::Struct(Struct::new(
                FIELDS_TYPE,
                [
                    ("Message", Value::from(diagnostic.message.as_str())),
                    ("Filepath", Value::from(file.path())),
                    ("Line", int(pos.line)),
                    ("Column", int(pos.column)),
                    ("Kind", Value::from(diagnostic.rule.kind)),
                    ("Snippet", Value::from(snippet)),
                    ("EndColumn", int(end_column)),
                    ("Rule", Value::from(diagnostic.rule.id)),
                ],
            ))
        })
        .collect()
}

/// A Go `int` from a line or column, which never exceeds `i64::MAX`.
fn int(number: usize) -> Value {
    Value::Int(i64::try_from(number).unwrap_or(i64::MAX))
}

/// actionlint's template functions. `allKinds` reads `kinds` when it runs.
fn functions(kinds: &Arc<OnceLock<Vec<&'static Kind>>>) -> Functions {
    let mut functions = Functions::new();
    let kinds = Arc::clone(kinds);
    let defined = [
        (
            "json",
            Function::new(&[Param::Any], None, |args| match args {
                [value] => json(value)
                    .map(Value::String)
                    .map_err(|err| format!("could not encode template value into JSON: {err}")),
                _ => Err("json takes one argument".to_owned()),
            }),
        ),
        (
            "replace",
            Function::new(&[Param::String], Some(Param::String), |args| {
                let mut strings = args.iter().map(|arg| match arg {
                    Value::String(s) => s.as_str(),
                    _ => "",
                });
                let s = strings.next().unwrap_or_default();
                let old_new: Vec<&str> = strings.collect();
                replace(s, &old_new).map(Value::String)
            }),
        ),
        (
            "toPascalCase",
            Function::new(&[Param::String], None, |args| match args {
                [Value::String(s)] => Ok(Value::String(to_pascal_case(s))),
                _ => Err("toPascalCase takes one string".to_owned()),
            }),
        ),
        (
            "getVersion",
            Function::new(&[], None, |_| Ok(Value::from(VERSION))),
        ),
        (
            "allKinds",
            Function::new(&[], None, move |_| {
                let kinds = kinds.get().map_or(&[][..], Vec::as_slice);
                Ok(Value::List(List::new(
                    KINDS_TYPE,
                    kinds.iter().map(|kind| {
                        Value::Struct(Struct::new(
                            KIND_TYPE,
                            [
                                ("Name", Value::from(kind.name)),
                                ("Description", Value::from(kind.description)),
                            ],
                        ))
                    }),
                )))
            }),
        ),
    ];
    for (name, function) in defined {
        // The names are identifiers, which `define` accepts.
        let _ = functions.define(name, function);
    }
    functions
}

/// Unescapes `\a`, `\b`, `\f`, `\n`, `\r`, `\t`, `\v`, and `\\` in the value of `-format`, from
/// left to right, as the reference's `strings.Replacer` does (`error.go:202-215`). Any other
/// backslash stays.
fn unescape_backslash(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let rest = chars.clone().next();
        let unescaped = match rest {
            Some('a') => '\u{7}',
            Some('b') => '\u{8}',
            Some('f') => '\u{c}',
            Some('n') => '\n',
            Some('r') => '\r',
            Some('t') => '\t',
            Some('v') => '\u{b}',
            Some('\\') => '\\',
            _ => {
                out.push('\\');
                continue;
            }
        };
        chars.next();
        out.push(unescaped);
    }
    out
}

/// Go's `strings.NewReplacer(old_new...).Replace(s)`. At each byte of `s`, the first pair whose
/// old string starts there is replaced, and the scan goes on after it. An empty old string
/// matches between every two bytes and at both ends, but not twice at the same place.
///
/// Go works on bytes, so an empty old string can split a character. The result then gets
/// U+FFFD for the split bytes, as other strings of the engine do
/// (`docs/differences.md#format-template-subset`).
fn replace(s: &str, old_new: &[&str]) -> Result<String, String> {
    if old_new.len() % 2 == 1 {
        return Err("strings.NewReplacer: odd argument count".to_owned());
    }
    let pairs: Vec<(&[u8], &[u8])> = old_new
        .as_chunks::<2>()
        .0
        .iter()
        .map(|[old, new]| (old.as_bytes(), new.as_bytes()))
        .collect();
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    let mut matched_empty = false;
    while at <= bytes.len() {
        let rest = bytes.get(at..).unwrap_or_default();
        let found = pairs
            .iter()
            .find(|(old, _)| (!old.is_empty() || !matched_empty) && rest.starts_with(old));
        matched_empty = matches!(found, Some((old, _)) if old.is_empty());
        if let Some((old, new)) = found {
            out.extend_from_slice(new);
            at += old.len();
        } else {
            out.extend(bytes.get(at).copied());
            at += 1;
        }
    }
    Ok(String::from_utf8(out)
        .unwrap_or_else(|err| String::from_utf8_lossy(err.as_bytes()).into_owned()))
}

/// The reference's `toPascalCase` (`error.go:217-231`): splits `s` at every character that is
/// not an ASCII letter or digit, upper-cases the first letter of each part if it is an ASCII
/// lower-case letter, and joins the parts.
fn to_pascal_case(s: &str) -> String {
    s.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + chars.as_str()
            })
        })
        .collect()
}

/// `value` as Go's `json.Encoder` prints it: with HTML escaping and a final line break.
///
/// A diagnostic gets the JSON keys of its struct tags in the reference, and a kind of
/// `allKinds` gets its field names, as Go gives a struct without tags. Map keys are sorted.
fn json(value: &Value) -> Result<String, String> {
    let mut out = String::new();
    encode(value, &mut out)?;
    out.push('\n');
    Ok(out)
}

fn encode(value: &Value, out: &mut String) -> Result<(), String> {
    match value {
        Value::Nil => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(n) => {
            let _ = write!(out, "{n}");
        }
        Value::Float(x) => encode_float(*x, out)?,
        Value::String(s) => encode_string(s, out),
        Value::List(list) => {
            out.push('[');
            for (index, item) in list.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                encode(item, out)?;
            }
            out.push(']');
        }
        Value::Map(map) => {
            out.push('{');
            for (index, (key, item)) in map.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                encode_string(key, out);
                out.push(':');
                encode(item, out)?;
            }
            out.push('}');
        }
        Value::Struct(record) if record.type_name() == FIELDS_TYPE => {
            out.push('{');
            let mut first = true;
            for (field, key, omit_empty) in JSON_KEYS {
                let Some(item) = record.get(field) else {
                    continue;
                };
                if omit_empty && matches!(item, Value::String(s) if s.is_empty()) {
                    continue;
                }
                if !first {
                    out.push(',');
                }
                first = false;
                encode_string(key, out);
                out.push(':');
                encode(item, out)?;
            }
            out.push('}');
        }
        Value::Struct(record) => {
            out.push('{');
            for (index, (field, item)) in record.fields().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                encode_string(field, out);
                out.push(':');
                encode(item, out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

/// A float as Go's `encoding/json` prints a `float64`: the shortest form that reads back as
/// the same number, in exponent form below 1e-6 and from 1e21 on.
fn encode_float(x: f64, out: &mut String) -> Result<(), String> {
    if !x.is_finite() {
        let name = if x.is_nan() {
            "NaN"
        } else if x > 0.0 {
            "+Inf"
        } else {
            "-Inf"
        };
        return Err(format!("json: unsupported value: {name}"));
    }
    let abs = x.abs();
    if abs != 0.0 && !(1e-6..1e21).contains(&abs) {
        let text = format!("{x:e}");
        match text.split_once('e') {
            Some((mantissa, exponent)) if !exponent.starts_with('-') => {
                let _ = write!(out, "{mantissa}e+{exponent}");
            }
            _ => out.push_str(&text),
        }
    } else {
        let _ = write!(out, "{x}");
    }
    Ok(())
}

/// A JSON string as Go's `encoding/json` writes it with HTML escaping.
fn encode_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '<' | '>' | '&' | '\u{2028}' | '\u{2029}' | '\u{0}'..='\u{1f}' => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn unescape_backslash_replaces_go_escapes_from_left_to_right() {
        assert_eq!(
            unescape_backslash(r"\a\b\f\n\r\t\v\\"),
            "\u{7}\u{8}\u{c}\n\r\t\u{b}\\"
        );
        assert_eq!(unescape_backslash(r"\\n"), r"\n");
        assert_eq!(unescape_backslash(r"\x\"), r"\x\");
        assert_eq!(unescape_backslash("é\\té"), "é\té");
    }

    #[test]
    fn to_pascal_case_matches_the_reference_tests() {
        // error_test.go:625-659, and parts split at non-ASCII characters and digits kept.
        for (input, want) in [
            ("", ""),
            ("foo", "Foo"),
            ("Foo", "Foo"),
            ("foo-bar", "FooBar"),
            ("fooBar", "FooBar"),
            ("FooBar", "FooBar"),
            ("foo_bar", "FooBar"),
            ("foo.bar", "FooBar"),
            ("foo-bar_baz.qux 9x", "FooBarBazQux9x"),
            ("über-éa", "BerA"),
        ] {
            assert_eq!(to_pascal_case(input), want, "{input:?}");
        }
    }

    #[test]
    fn replace_matches_go_strings_replacer() {
        // The wanted values come from Go 1.26.4.
        for (s, old_new, want) in [
            ("ab", &["", "X"][..], "XaXbX"),
            ("", &["", "X"][..], "X"),
            ("abab", &["a", "1", "a", "2", "ab", "3"][..], "1b1b"),
            ("abac", &["ab", "1", "a", "2"][..], "12c"),
            ("aaa", &["aa", "X"][..], "Xa"),
            ("bab", &["a", "", "", "-"][..], "-b-b-"),
            ("bab", &[][..], "bab"),
            ("a\nb", &["\n", "%0A"][..], "a%0Ab"),
        ] {
            assert_eq!(replace(s, old_new).unwrap(), want, "{s:?} {old_new:?}");
        }
    }

    #[test]
    fn replace_with_an_odd_number_of_strings_fails_as_go_panics() {
        assert_eq!(
            replace("a", &["a"]),
            Err("strings.NewReplacer: odd argument count".to_owned())
        );
    }

    #[test]
    fn json_escapes_html_and_control_characters_as_go_does() {
        let s = "<a&b>\u{2028}\u{2029}\u{8}\u{c}\n\r\t\u{1}\u{1f}\u{7f}\"\\é";
        assert_eq!(
            json(&Value::from(s)).unwrap(),
            "\"\\u003ca\\u0026b\\u003e\\u2028\\u2029\\b\\f\\n\\r\\t\\u0001\\u001f\u{7f}\\\"\\\\é\"\n"
        );
    }

    #[test]
    fn json_prints_floats_as_go_does() {
        for (x, want) in [
            (1.5e-7, "1.5e-7"),
            (1e21, "1e+21"),
            (1e20, "100000000000000000000"),
            (0.0, "0"),
            (-0.0, "-0"),
            (0.000_001, "0.000001"),
            (123_456_789.125, "123456789.125"),
            (-2.5e-10, "-2.5e-10"),
        ] {
            assert_eq!(json(&Value::Float(x)).unwrap(), format!("{want}\n"), "{x}");
        }
    }

    #[test]
    fn json_of_a_float_that_is_not_finite_fails_as_go_does() {
        assert_eq!(
            json(&Value::Float(f64::NAN)),
            Err("json: unsupported value: NaN".to_owned())
        );
        assert_eq!(
            json(&Value::Float(f64::NEG_INFINITY)),
            Err("json: unsupported value: -Inf".to_owned())
        );
        assert_eq!(
            json(&Value::Float(f64::INFINITY)),
            Err("json: unsupported value: +Inf".to_owned())
        );
    }

    #[test]
    fn json_prints_lists_maps_and_nil_as_go_does() {
        let list = Value::list([1.into(), "x".into(), true.into(), Value::Nil]);
        assert_eq!(json(&list).unwrap(), "[1,\"x\",true,null]\n");
        let map = Value::map([("b", Value::from(1)), ("a", Value::from(2))]);
        assert_eq!(json(&map).unwrap(), "{\"a\":2,\"b\":1}\n");
    }

    #[test]
    fn json_of_a_kind_uses_the_go_field_names() {
        let kind = Value::Struct(Struct::new(
            KIND_TYPE,
            [("Name", "n".into()), ("Description", "d".into())],
        ));
        assert_eq!(
            json(&kind).unwrap(),
            "{\"Name\":\"n\",\"Description\":\"d\"}\n"
        );
    }

    fn file(text: &str) -> SourceFile {
        SourceFile::new(workflint_diagnostics::FileId::new(0), "a.yaml", text).unwrap()
    }

    fn diagnostic(start: u32, end: u32) -> Diagnostic {
        let rule = workflint_linter::registry::find("unexpected-key").unwrap();
        let span =
            workflint_diagnostics::Span::new(workflint_diagnostics::FileId::new(0), start, end);
        Diagnostic::new(rule, span, "m <1>")
    }

    #[test]
    fn json_of_a_diagnostic_has_the_seven_keys_of_the_reference() {
        let file = file("on: push\nx: 1\n");
        let fields = fields(&file, &[diagnostic(9, 10)]);
        assert_eq!(
            json(&fields[0]).unwrap(),
            "{\"message\":\"m \\u003c1\\u003e\",\"filepath\":\"a.yaml\",\"line\":2,\"column\":1,\
             \"kind\":\"syntax-check\",\"snippet\":\"x: 1\\n^\",\"end_column\":1}\n"
        );
    }

    #[test]
    fn json_of_a_diagnostic_leaves_out_an_empty_snippet() {
        let file = file("on: push\n");
        let fields = fields(&file, &[diagnostic(9, 9)]);
        assert_eq!(
            json(&fields[0]).unwrap(),
            "{\"message\":\"m \\u003c1\\u003e\",\"filepath\":\"a.yaml\",\"line\":2,\"column\":1,\
             \"kind\":\"syntax-check\",\"end_column\":1}\n"
        );
    }

    fn print(template: &str, fields: Vec<Value>) -> (String, Result<(), String>) {
        let formatter = Formatter::new(template).unwrap();
        let mut out = Vec::new();
        let result = formatter.print(&mut out, fields).map_err(|err| match err {
            PrintError::Template(message) => message,
            PrintError::Write(err) => err.to_string(),
        });
        (String::from_utf8(out).unwrap(), result)
    }

    #[test]
    fn json_runs_on_any_value_of_the_template() {
        let (out, result) = print("{{json 1.5}}{{json \"<\"}}{{json .}}", Vec::new());
        assert_eq!((out.as_str(), result), ("1.5\n\"\\u003c\"\n[]\n", Ok(())));
    }

    #[test]
    fn replace_with_an_odd_number_of_strings_is_an_execution_error() {
        let file = file("on: push\nx: 1\n");
        let (out, result) = print(
            "x{{range $ = .}}{{replace $.Message \"a\"}}{{end}}",
            fields(&file, &[diagnostic(9, 10)]),
        );
        assert_eq!(out, "x");
        assert_eq!(
            result,
            Err(
                "could not format error messages: template: error formatter:1:18: executing \
                 \"error formatter\" at <replace $.Message \"a\">: error calling replace: \
                 strings.NewReplacer: odd argument count"
                    .to_owned()
            )
        );
    }

    #[test]
    fn all_kinds_is_empty_until_the_kinds_are_set() {
        let formatter = Formatter::new("{{len allKinds}}").unwrap();
        let mut out = Vec::new();
        formatter.print(&mut out, Vec::new()).unwrap();
        assert_eq!(out, b"0");
    }
}
