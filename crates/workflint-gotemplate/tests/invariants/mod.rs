//! The check that the `gotemplate_parse` fuzz target in `fuzz/` runs on each input.
//!
//! The fuzz target includes this file with `#[path]`, and `fuzz_regressions.rs` runs it on
//! the inputs in `tests/fuzz-regressions/`, so a crash input found by the fuzzer fails the same
//! check as a test. The check panics when an invariant does not hold.

#![allow(clippy::panic, clippy::unwrap_used)]

use workflint_gotemplate::{Error, Function, Functions, Param, Template, Value};

/// The functions that actionlint adds, with bodies that return nothing, so that templates
/// written for actionlint parse.
fn functions() -> Functions {
    let mut functions = Functions::new();
    let any = |params: &[Param], variadic| Function::new(params, variadic, |_| Ok(Value::Nil));
    functions.define("json", any(&[Param::Any], None)).unwrap();
    functions
        .define("replace", any(&[Param::String], Some(Param::String)))
        .unwrap();
    functions
        .define("toPascalCase", any(&[Param::String], None))
        .unwrap();
    functions.define("getVersion", any(&[], None)).unwrap();
    functions.define("allKinds", any(&[], None)).unwrap();
    functions
}

/// Parses `text` as a template.
///
/// The parser does not panic. It fails exactly when it returns an [`Error::Parse`], whose
/// message starts with `template: fuzz:` and a line number no greater than the number of lines
/// in `text`. Text without `{{` always parses, and parsing the same text twice gives the same
/// result.
pub(crate) fn check_parse(text: &str) {
    let first = Template::parse("fuzz", text, functions());
    let second = Template::parse("fuzz", text, functions());
    match (&first, &second) {
        (Ok(_), Ok(_)) => {}
        (Err(Error::Parse(a)), Err(Error::Parse(b))) => {
            assert_eq!(a, b, "{text:?}");
            let rest = a
                .strip_prefix("template: fuzz:")
                .unwrap_or_else(|| panic!("{a:?} for {text:?}"));
            let line: usize = rest
                .split(':')
                .next()
                .and_then(|line| line.parse().ok())
                .unwrap_or_else(|| panic!("{a:?} for {text:?}"));
            let lines = 1 + text.bytes().filter(|b| *b == b'\n').count();
            assert!((1..=lines).contains(&line), "{a:?} for {text:?}");
        }
        _ => panic!("{first:?} and {second:?} for {text:?}"),
    }
    if !text.contains("{{") {
        assert!(first.is_ok(), "{first:?} for {text:?}");
    }
}
