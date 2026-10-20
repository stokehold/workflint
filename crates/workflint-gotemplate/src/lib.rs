//! A subset of Go's `text/template` for workflint's `-format` flag.
//!
//! This is an internal crate with no stability promise. actionlint formats its errors with a
//! Go template, so workflint needs an engine that parses the same templates and prints the same
//! bytes. The parser, executor, and built-in functions are ports of Go's `text/template`, and
//! errors use Go's wording. `README.md` lists what differs from Go.
//!
//! [`Template::parse`] parses a template and checks that each function it calls exists.
//! [`Template::execute`] runs it with a [`Value`] as its data. The caller builds the value from
//! scalars, [`List`]s, [`Map`]s, and [`Struct`]s, and adds its own [`Function`]s by name in
//! [`Functions`].
//!
//! ```
//! use workflint_gotemplate::{Function, Functions, Param, Struct, Template, Value};
//!
//! let mut functions = Functions::new();
//! functions.define(
//!     "upper",
//!     Function::new(&[Param::String], None, |args| match args {
//!         [Value::String(s)] => Ok(Value::String(s.to_uppercase())),
//!         _ => Err("want one string".to_owned()),
//!     }),
//! )?;
//! let template = Template::parse(
//!     "example",
//!     "{{range $err := .}}{{upper $err.Kind}}: {{$err.Message | printf \"%q\"}}\n{{end}}",
//!     functions,
//! )?;
//! let data = Value::list([Value::Struct(Struct::new(
//!     "*actionlint.ErrorTemplateFields",
//!     [("Message", Value::from("tab\there")), ("Kind", Value::from("expression"))],
//! ))]);
//! let mut out = Vec::new();
//! template.execute(&mut out, &data)?;
//! assert_eq!(String::from_utf8(out)?, "EXPRESSION: \"tab\\there\"\n");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod exec;
mod fmt;
mod func;
mod lex;
mod parse;
mod strconv;
mod value;

use std::io::Write;

pub use func::{Function, Functions, Param};
pub use value::{List, Map, Struct, Value};

/// A parsed template, ready to run any number of times.
#[derive(Debug)]
pub struct Template {
    name: String,
    text: String,
    tree: parse::Tree,
    functions: Functions,
}

impl Template {
    /// Parses `text` as the template `name`, which error messages show. Templates can call
    /// Go's built-in functions and those in `functions`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Parse`] with Go's message, such as
    /// `template: name:1: function "xxx" not defined`.
    pub fn parse(name: &str, text: &str, functions: Functions) -> Result<Template, Error> {
        let tree = parse::parse(name, text, |function| functions.contains(function))
            .map_err(|error| Error::Parse(error.0))?;
        Ok(Template {
            name: name.to_owned(),
            text: text.to_owned(),
            tree,
            functions,
        })
    }

    /// The name given to [`Template::parse`].
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Runs the template with `data` as `.` and `$`, and writes the output to `out` as it goes.
    /// After an error, `out` has the output up to the failing action.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Exec`] with Go's message when an action fails, such as
    /// `template: name:1:2: executing "name" at <.Foo>: can't evaluate field Foo in type string`,
    /// and [`Error::Write`] when `out` fails.
    pub fn execute<W: Write>(&self, out: &mut W, data: &Value) -> Result<(), Error> {
        exec::execute(
            &self.name,
            &self.text,
            &self.tree,
            &self.functions,
            out,
            data,
        )
    }
}

/// An error from parsing or running a template. Each message is in Go's wording.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The template does not parse.
    Parse(String),
    /// An action failed while the template ran.
    Exec(String),
    /// Writing the output failed.
    Write(std::io::Error),
    /// A function name is not an identifier.
    FunctionName(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Parse(message) | Error::Exec(message) | Error::FunctionName(message) => {
                f.write_str(message)
            }
            Error::Write(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Write(error) => Some(error),
            _ => None,
        }
    }
}
