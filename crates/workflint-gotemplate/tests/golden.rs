//! Runs each template in `golden/cases.rs.in` and compares the output or error with Go's.
//!
//! `xtask/go-template-golden` writes `cases.rs.in` with Go's `text/template` and
//! actionlint's template functions. The cases cover the recipes in actionlint's
//! `docs/usage.md` and `testdata/format/`, the templates in its `error_test.go`, and each part
//! of the language and each built-in function. The data sets here match the ones in the Go
//! program. The functions are test versions of actionlint's: `json` encodes only what the data
//! sets hold, and `getVersion` and `allKinds` return fixed values.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fmt::Write as _;

use workflint_gotemplate::{Error, Function, Functions, List, Param, Struct, Template, Value};

#[derive(Clone, Copy, Debug)]
enum Data {
    Errors,
    Report,
    NoErrors,
    Misc,
    Nil,
}

#[derive(Debug)]
enum Want {
    Output(&'static str),
    ParseError(&'static str),
    ExecError(&'static str, &'static str),
}

struct Case {
    name: &'static str,
    data: Data,
    template: &'static str,
    want: Want,
}

const CASES: &[Case] = &include!("golden/cases.rs.in");

#[allow(clippy::too_many_arguments)]
fn fields(
    message: &str,
    filepath: &str,
    line: i64,
    column: i64,
    kind: &str,
    snippet: &str,
    end_column: i64,
) -> Value {
    Value::Struct(Struct::new(
        "*actionlint.ErrorTemplateFields",
        [
            ("Message", Value::from(message)),
            ("Filepath", Value::from(filepath)),
            ("Line", Value::Int(line)),
            ("Column", Value::Int(column)),
            ("Kind", Value::from(kind)),
            ("Snippet", Value::from(snippet)),
            ("EndColumn", Value::Int(end_column)),
        ],
    ))
}

fn errors_list(items: Vec<Value>) -> Value {
    Value::List(List::new("[]*actionlint.ErrorTemplateFields", items))
}

fn strings(type_name: &str, items: &[&str]) -> Value {
    Value::List(List::new(
        type_name,
        items.iter().map(|item| Value::from(*item)),
    ))
}

#[allow(clippy::too_many_lines)]
fn data(set: Data) -> Value {
    match set {
        Data::Errors => errors_list(vec![
            fields("message 1", "file1", 1, 2, "kind1", "snippet 1", 3),
            fields("message 2", "file2", 3, 4, "kind2", "snippet 2", 5),
        ]),
        Data::Report => errors_list(vec![
            fields(
                r#"property "platform" is not defined in object type {os: string}"#,
                "test.yaml",
                21,
                20,
                "expression",
                "          key: ${{ matrix.platform }}-node\n                   ^~~~~~~~~~~~~~~",
                34,
            ),
            fields(
                "label \"linux-latest\" is unknown. <a> & 'b'\tcafé",
                ".github/workflows/ci.yaml",
                7,
                14,
                "runner-label",
                "",
                14,
            ),
        ]),
        Data::NoErrors => errors_list(Vec::new()),
        Data::Nil => Value::Nil,
        Data::Misc => Value::map([
            ("str", Value::from("héllo")),
            ("int", Value::Int(42)),
            ("neg", Value::Int(-7)),
            ("zero", Value::Int(0)),
            ("float", Value::Float(3.5)),
            ("whole", Value::Float(2.0)),
            ("big", Value::Float(1e21)),
            ("small", Value::Float(0.00001)),
            ("t", Value::Bool(true)),
            ("f", Value::Bool(false)),
            ("nil", Value::Nil),
            ("empty", Value::from("")),
            (
                "list",
                Value::list([
                    Value::from("a"),
                    Value::Int(1),
                    Value::Bool(true),
                    Value::Nil,
                ]),
            ),
            ("none", Value::list([])),
            ("strs", strings("[]string", &["x", "y", "z"])),
            (
                "ints",
                Value::List(List::new("[]int", [3, 1, 2].map(Value::Int))),
            ),
            (
                "map",
                Value::map([
                    ("b", Value::Int(2)),
                    ("a", Value::Int(1)),
                    ("c", Value::from("three")),
                ]),
            ),
            ("nomap", Value::map::<&str>([])),
            (
                "nested",
                Value::map([("inner", Value::map([("x", Value::from("deep"))]))]),
            ),
            ("ctrl", Value::from("tab\there\nnl\u{0}\u{7}\u{7f}\r")),
            (
                "uni",
                Value::from("日本語 é \u{200b} \u{ad} \u{1f600} \u{2028}"),
            ),
            ("quote", Value::from("say \"hi\" \\ back`")),
            ("html", Value::from("<a href=\"x\">&'</a>\u{0}")),
            ("js", Value::from("it's <b> & \"q\" = \\ \u{2028} \u{1} é")),
            ("url", Value::from("a b&c=d/é?~_-.")),
            (
                "words",
                strings(
                    "[]string",
                    &[
                        "", "foo", "Foo", "foo-bar", "fooBar", "FooBar", "foo_bar", "foo.bar",
                    ],
                ),
            ),
            ("errs", data(Data::Errors)),
            ("one", fields("m", "f", 1, 2, "k", "s", 3)),
            (
                "mixed",
                Value::list([Value::Int(1), Value::from("1"), Value::Float(1.0)]),
            ),
            (
                "maps",
                Value::List(List::new(
                    "[]map[string]interface {}",
                    [
                        Value::map([("k", Value::Int(1))]),
                        Value::map([("k", Value::Int(2))]),
                    ],
                )),
            ),
            ("Capital", Value::from("upper")),
            ("spaced", Value::from("  padded  ")),
        ]),
    }
}

/// The JSON key of each field of `ErrorTemplateFields`, and whether it is left out when empty.
fn json_key(field: &str) -> (&str, bool) {
    match field {
        "Message" => ("message", false),
        "Filepath" => ("filepath", true),
        "Line" => ("line", false),
        "Column" => ("column", false),
        "Kind" => ("kind", false),
        "Snippet" => ("snippet", true),
        "EndColumn" => ("end_column", false),
        other => (other, false),
    }
}

/// Go's `encoding/json` for the values in the data sets, with HTML escaping.
fn encode_json(out: &mut String, value: &Value) {
    match value {
        Value::Nil => out.push_str("null"),
        Value::Bool(b) => write!(out, "{b}").unwrap(),
        Value::Int(n) => write!(out, "{n}").unwrap(),
        Value::Float(x) => write!(out, "{x}").unwrap(),
        Value::String(s) => {
            out.push('"');
            for c in s.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    '<' | '>' | '&' | '\u{2028}' | '\u{2029}' => {
                        write!(out, "\\u{:04x}", u32::from(c)).unwrap();
                    }
                    c if c < ' ' => write!(out, "\\u{:04x}", u32::from(c)).unwrap(),
                    c => out.push(c),
                }
            }
            out.push('"');
        }
        Value::List(list) => {
            out.push('[');
            for (index, item) in list.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                encode_json(out, item);
            }
            out.push(']');
        }
        Value::Map(map) => {
            out.push('{');
            for (index, (key, item)) in map.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                encode_json(out, &Value::from(key.as_str()));
                out.push(':');
                encode_json(out, item);
            }
            out.push('}');
        }
        Value::Struct(record) => {
            out.push('{');
            let mut first = true;
            for (field, item) in record.fields() {
                let (key, omit_empty) = json_key(field);
                if omit_empty && *item == Value::from("") {
                    continue;
                }
                if !first {
                    out.push(',');
                }
                first = false;
                encode_json(out, &Value::from(key));
                out.push(':');
                encode_json(out, item);
            }
            out.push('}');
        }
    }
}

fn to_pascal_case(s: &str) -> String {
    s.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) if c.is_ascii_lowercase() => {
                    c.to_ascii_uppercase().to_string() + chars.as_str()
                }
                _ => part.to_owned(),
            }
        })
        .collect()
}

fn kind(name: &str, description: &str) -> Value {
    Value::Struct(Struct::new(
        "*actionlint.ruleTemplateFields",
        [
            ("Name", Value::from(name)),
            ("Description", Value::from(description)),
        ],
    ))
}

/// actionlint's template functions, as the Go program defines them.
fn functions() -> Functions {
    let mut functions = Functions::new();
    functions
        .define(
            "json",
            Function::new(&[Param::Any], None, |args| {
                let mut out = String::new();
                encode_json(&mut out, &args[0]);
                out.push('\n');
                Ok(Value::String(out))
            }),
        )
        .unwrap();
    functions
        .define(
            "replace",
            Function::new(&[Param::String], Some(Param::String), |args| {
                let [Value::String(s), Value::String(old), Value::String(new)] = args else {
                    panic!("replace takes three strings in these tests: {args:?}");
                };
                Ok(Value::String(s.replace(old.as_str(), new)))
            }),
        )
        .unwrap();
    functions
        .define(
            "toPascalCase",
            Function::new(&[Param::String], None, |args| match args {
                [Value::String(s)] => Ok(Value::String(to_pascal_case(s))),
                _ => panic!("toPascalCase takes a string: {args:?}"),
            }),
        )
        .unwrap();
    functions
        .define(
            "getVersion",
            Function::new(&[], None, |_| Ok(Value::from("1.2.3"))),
        )
        .unwrap();
    functions
        .define(
            "allKinds",
            Function::new(&[], None, |_| {
                Ok(Value::List(List::new(
                    "[]*actionlint.ruleTemplateFields",
                    [
                        kind(
                            "expression",
                            "Checks for syntax errors in expressions ${{ }} syntax",
                        ),
                        kind("syntax-check", "Checks for GitHub Actions workflow syntax"),
                    ],
                )))
            }),
        )
        .unwrap();
    functions
}

fn run(case: &Case) -> Result<String, String> {
    let template = match Template::parse("error formatter", case.template, functions()) {
        Ok(template) => template,
        Err(Error::Parse(message)) => return Err(format!("parse: {message}")),
        Err(other) => panic!("{other:?}"),
    };
    let mut out = Vec::new();
    let result = template.execute(&mut out, &data(case.data));
    let out = String::from_utf8(out).unwrap();
    match result {
        Ok(()) => Ok(out),
        Err(Error::Exec(message)) => Err(format!("exec: {out:?} {message}")),
        Err(other) => panic!("{other:?}"),
    }
}

#[test]
fn every_case_matches_go() {
    let mut failures = Vec::new();
    for case in CASES {
        let have = run(case);
        let want = match case.want {
            Want::Output(out) => Ok(out.to_owned()),
            Want::ParseError(message) => Err(format!("parse: {message}")),
            Want::ExecError(out, message) => Err(format!("exec: {out:?} {message}")),
        };
        if have != want {
            failures.push(format!(
                "{}: {:?}\n  want: {want:?}\n  have: {have:?}",
                case.name, case.template
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ from Go:\n{}",
        failures.len(),
        CASES.len(),
        failures.join("\n")
    );
}

#[test]
fn every_recipe_and_error_test_template_parses() {
    for case in CASES {
        if case.name.starts_with("recipe_")
            || case.name.starts_with("error_test_") && !matches!(case.want, Want::ParseError(_))
        {
            assert!(
                Template::parse("error formatter", case.template, functions()).is_ok(),
                "{}",
                case.name
            );
        }
    }
}
