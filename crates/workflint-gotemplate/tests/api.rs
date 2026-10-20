//! Tests of the crate's API and of the behaviour that `golden.rs` cannot compare with Go:
//! write errors, the caller's functions, and what this engine leaves out.

#![allow(clippy::unwrap_used)]

use std::io::{self, Write};

use workflint_gotemplate::{Error, Function, Functions, Param, Struct, Template, Value};

fn render(text: &str, functions: Functions, data: &Value) -> Result<String, Error> {
    let template = Template::parse("t", text, functions)?;
    let mut out = Vec::new();
    template.execute(&mut out, data)?;
    Ok(String::from_utf8(out).unwrap())
}

struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("dummy write error"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_write_error_stops_execution_with_the_writer_error() {
    // Ported from `TestErrorFormatterPrintError` in actionlint's `error_test.go`.
    let template = Template::parse("t", "{{(index . 0).Message}}", Functions::new()).unwrap();
    let data = Value::list([Value::Struct(Struct::new(
        "*actionlint.ErrorTemplateFields",
        [("Message", Value::from("message 1"))],
    ))]);
    let error = template.execute(&mut FailingWriter, &data).unwrap_err();
    assert!(matches!(error, Error::Write(_)), "{error:?}");
    assert_eq!(error.to_string(), "dummy write error");
}

#[test]
fn a_function_error_names_the_function_and_the_command() {
    let mut functions = Functions::new();
    functions
        .define(
            "fail",
            Function::new(&[Param::Any], None, |_| Err("boom".to_owned())),
        )
        .unwrap();
    let error = render("{{fail 1}}", functions, &Value::Nil).unwrap_err();
    assert_eq!(
        error.to_string(),
        r#"template: t:1:2: executing "t" at <fail 1>: error calling fail: boom"#
    );
}

#[test]
fn a_function_gets_arguments_checked_against_its_parameters() {
    let mut functions = Functions::new();
    let echo = |args: &[Value]| Ok(Value::from(format!("{args:?}")));
    functions
        .define(
            "f",
            Function::new(&[Param::Int, Param::Float], Some(Param::Bool), echo),
        )
        .unwrap();
    assert_eq!(
        render("{{f 1 2.5 true false}}", functions.clone(), &Value::Nil).unwrap(),
        "[Int(1), Float(2.5), Bool(true), Bool(false)]"
    );
    assert_eq!(
        render("{{f 1 2}}", functions.clone(), &Value::Nil).unwrap(),
        "[Int(1), Float(2.0)]"
    );
    assert_eq!(
        render("{{f 1.5 2}}", functions.clone(), &Value::Nil)
            .unwrap_err()
            .to_string(),
        r#"template: t:1:4: executing "t" at <1.5>: expected integer; found 1.5"#
    );
    assert_eq!(
        render("{{f 1 2 3}}", functions.clone(), &Value::Nil)
            .unwrap_err()
            .to_string(),
        r#"template: t:1:8: executing "t" at <3>: expected bool; found 3"#
    );
    assert_eq!(
        render("{{true | f 1 2}}", functions.clone(), &Value::Nil).unwrap(),
        "[Int(1), Float(2.0), Bool(true)]"
    );
    assert_eq!(
        render("{{f}}", functions, &Value::Nil)
            .unwrap_err()
            .to_string(),
        r#"template: t:1:2: executing "t" at <f>: wrong number of args for f: want at least 2 got 0"#
    );
}

#[test]
fn a_function_hides_the_built_in_of_the_same_name() {
    let mut functions = Functions::new();
    functions
        .define(
            "len",
            Function::new(&[Param::Any], None, |_| Ok(Value::from("mine"))),
        )
        .unwrap();
    assert_eq!(render("{{len 1}}", functions, &Value::Nil).unwrap(), "mine");
}

#[test]
fn a_function_named_break_makes_break_a_call() {
    let mut functions = Functions::new();
    functions
        .define("break", Function::new(&[], None, |_| Ok(Value::from("b"))))
        .unwrap();
    assert_eq!(render("{{break}}", functions, &Value::Nil).unwrap(), "b");
}

#[test]
fn a_function_name_must_be_an_identifier() {
    let mut functions = Functions::new();
    let error = functions
        .define("to-json", Function::new(&[], None, |_| Ok(Value::Nil)))
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        r#"function name "to-json" is not a valid identifier"#
    );
}

#[test]
fn define_template_and_block_are_parse_errors() {
    for (text, keyword) in [
        (r#"{{define "x"}}{{end}}"#, "define"),
        (r#"{{template "x"}}"#, "template"),
        (r#"{{block "x" .}}{{end}}"#, "block"),
    ] {
        let error = Template::parse("t", text, Functions::new()).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("template: t:1: {{{{{keyword}}}}} is not supported")
        );
    }
}

#[test]
fn complex_numbers_are_parse_errors() {
    let error = Template::parse("t", "{{1+2i}}", Functions::new()).unwrap_err();
    assert_eq!(
        error.to_string(),
        r#"template: t:1: complex number "1+2i" is not supported"#
    );
}

#[test]
fn nesting_deeper_than_the_limit_is_a_parse_error() {
    let parens = format!("{{{{{}1{}}}}}", "(".repeat(100), ")".repeat(100));
    assert_eq!(render(&parens, Functions::new(), &Value::Nil).unwrap(), "1");
    let parens = format!("{{{{{}1{}}}}}", "(".repeat(101), ")".repeat(101));
    assert_eq!(
        Template::parse("t", &parens, Functions::new())
            .unwrap_err()
            .to_string(),
        "template: t:1: max expression depth exceeded"
    );
    let ifs = format!("{}x{}", "{{if 1}}".repeat(100), "{{end}}".repeat(100));
    assert_eq!(render(&ifs, Functions::new(), &Value::Nil).unwrap(), "x");
    let ifs = format!("{}x{}", "{{if 1}}".repeat(101), "{{end}}".repeat(101));
    assert_eq!(
        Template::parse("t", &ifs, Functions::new())
            .unwrap_err()
            .to_string(),
        "template: t:1: max expression depth exceeded"
    );
}

#[test]
fn a_missing_key_of_a_typed_map_indexes_to_the_zero_value() {
    let data = Value::Map(workflint_gotemplate::Map::new(
        "map[string]int",
        [("a", Value::Int(1))],
    ));
    assert_eq!(
        render(
            r#"{{index . "a"}} {{index . "b"}}"#,
            Functions::new(),
            &data
        )
        .unwrap(),
        "1 0"
    );
}

#[test]
fn a_pointer_to_a_struct_prints_with_an_ampersand_only_through_printf() {
    let data = Value::Struct(Struct::new("*pkg.T", [("A", Value::Int(1))]));
    assert_eq!(
        render(
            r#"{{.}} {{print .}} {{printf "%v" .}}"#,
            Functions::new(),
            &data
        )
        .unwrap(),
        "{1} {1} &{1}"
    );
}

#[test]
fn the_template_keeps_its_name() {
    let template = Template::parse("error formatter", "x", Functions::new()).unwrap();
    assert_eq!(template.name(), "error formatter");
}
