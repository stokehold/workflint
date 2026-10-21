//! Functions that templates call: the caller's, registered in [`Functions`], and Go's
//! built-ins, a port of Go's `text/template/funcs.go`.

use std::collections::BTreeMap;
use std::fmt::{self, Debug, Formatter};
use std::sync::Arc;

use crate::Error;
use crate::fmt::{is_digit, is_letter, is_print, sprint, sprintf};
use crate::value::{List, Value};

/// The type of a function parameter. Execution checks each argument against it with Go's
/// rules and reports a mismatch in Go's words, such as `wrong type for value; expected string;
/// got int`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Param {
    /// Any value, as Go's `interface {}`.
    Any,
    /// A Go `string`.
    String,
    /// A Go `int`.
    Int,
    /// A Go `float64`.
    Float,
    /// A Go `bool`.
    Bool,
}

impl Param {
    pub(crate) fn go_name(self) -> &'static str {
        match self {
            Param::Any => "interface {}",
            Param::String => "string",
            Param::Int => "int",
            Param::Float => "float64",
            Param::Bool => "bool",
        }
    }
}

type Body = dyn Fn(&[Value]) -> Result<Value, String> + Send + Sync;

/// A function that templates can call by name, with the types of its parameters.
///
/// The body gets one value per argument, already checked against the parameter types. An
/// `Err` stops the execution with Go's `error calling <name>: <message>`.
#[derive(Clone)]
pub struct Function {
    pub(crate) params: Vec<Param>,
    pub(crate) variadic: Option<Param>,
    pub(crate) body: Arc<Body>,
}

impl Function {
    /// A function with the parameters `params`, then any number of parameters of type
    /// `variadic` if it is set, as a Go function `func(a A, b B, rest ...V)`.
    pub fn new(
        params: &[Param],
        variadic: Option<Param>,
        body: impl Fn(&[Value]) -> Result<Value, String> + Send + Sync + 'static,
    ) -> Function {
        Function {
            params: params.to_vec(),
            variadic,
            body: Arc::new(body),
        }
    }
}

impl Debug for Function {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Function")
            .field("params", &self.params)
            .field("variadic", &self.variadic)
            .finish_non_exhaustive()
    }
}

/// The caller's functions, by name. They hide built-ins with the same name, as in Go.
#[derive(Debug, Clone, Default)]
pub struct Functions {
    map: BTreeMap<String, Function>,
}

impl Functions {
    /// No functions beyond the built-ins.
    #[must_use]
    pub fn new() -> Functions {
        Functions::default()
    }

    /// Adds `function` under `name`, replacing any function of that name.
    ///
    /// # Errors
    ///
    /// Returns [`Error::FunctionName`] if `name` is not an identifier: letters, digits, and
    /// underscores, not starting with a digit.
    pub fn define(&mut self, name: &str, function: Function) -> Result<(), Error> {
        if !good_name(name) {
            return Err(Error::FunctionName(format!(
                "function name {} is not a valid identifier",
                crate::fmt::quote(name)
            )));
        }
        self.map.insert(name.to_owned(), function);
        Ok(())
    }

    pub(crate) fn get(&self, name: &str) -> Option<&Function> {
        self.map.get(name)
    }

    pub(crate) fn contains(&self, name: &str) -> bool {
        self.map.contains_key(name) || Builtin::from_name(name).is_some()
    }
}

/// Go's `goodName`.
fn good_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    name.chars()
        .enumerate()
        .all(|(index, c)| c == '_' || is_letter(c) || (index > 0 && is_digit(c)))
}

/// Go's built-in template functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Builtin {
    And,
    Call,
    Html,
    Index,
    Slice,
    Js,
    Len,
    Not,
    Or,
    Print,
    Printf,
    Println,
    Urlquery,
    Eq,
    Ge,
    Gt,
    Le,
    Lt,
    Ne,
}

impl Builtin {
    pub(crate) fn from_name(name: &str) -> Option<Builtin> {
        Some(match name {
            "and" => Builtin::And,
            "call" => Builtin::Call,
            "html" => Builtin::Html,
            "index" => Builtin::Index,
            "slice" => Builtin::Slice,
            "js" => Builtin::Js,
            "len" => Builtin::Len,
            "not" => Builtin::Not,
            "or" => Builtin::Or,
            "print" => Builtin::Print,
            "printf" => Builtin::Printf,
            "println" => Builtin::Println,
            "urlquery" => Builtin::Urlquery,
            "eq" => Builtin::Eq,
            "ge" => Builtin::Ge,
            "gt" => Builtin::Gt,
            "le" => Builtin::Le,
            "lt" => Builtin::Lt,
            "ne" => Builtin::Ne,
            _ => return None,
        })
    }

    /// The fixed parameters and the variadic parameter of the Go function.
    pub(crate) fn signature(self) -> (&'static [Param], Option<Param>) {
        const ONE: &[Param] = &[Param::Any];
        const TWO: &[Param] = &[Param::Any, Param::Any];
        match self {
            Builtin::And
            | Builtin::Or
            | Builtin::Call
            | Builtin::Index
            | Builtin::Slice
            | Builtin::Eq => (ONE, Some(Param::Any)),
            Builtin::Html | Builtin::Js | Builtin::Print | Builtin::Println | Builtin::Urlquery => {
                (&[], Some(Param::Any))
            }
            Builtin::Len | Builtin::Not => (ONE, None),
            Builtin::Printf => (&[Param::String], Some(Param::Any)),
            Builtin::Ge | Builtin::Gt | Builtin::Le | Builtin::Lt | Builtin::Ne => (TWO, None),
        }
    }

    /// Calls the built-in with arguments of the right number and types. `and`, `or`, and
    /// `call` are handled by the executor.
    pub(crate) fn call(self, args: &[Value]) -> Result<Value, String> {
        let arg = |index: usize| args.get(index).cloned().unwrap_or(Value::Nil);
        match self {
            Builtin::And | Builtin::Or => Ok(arg(args.len().saturating_sub(1))),
            // The executor calls `call`, which needs the text of its first argument.
            Builtin::Call => Err("call of nil".to_owned()),
            Builtin::Html => Ok(Value::String(html_escape(&eval_args(args)))),
            Builtin::Js => Ok(Value::String(js_escape(&eval_args(args)))),
            Builtin::Urlquery => Ok(Value::String(query_escape(&eval_args(args)))),
            Builtin::Print => Ok(Value::String(sprint(args))),
            Builtin::Println => Ok(Value::String(crate::fmt::sprintln(args))),
            Builtin::Printf => {
                let format = match args.first() {
                    Some(Value::String(format)) => format.as_str(),
                    _ => "",
                };
                Ok(Value::String(sprintf(
                    format,
                    args.get(1..).unwrap_or_default(),
                )))
            }
            Builtin::Index => index(&arg(0), args.get(1..).unwrap_or_default()),
            Builtin::Slice => slice(&arg(0), args.get(1..).unwrap_or_default()),
            Builtin::Len => length(&arg(0)).map(Value::Int),
            Builtin::Not => Ok(Value::Bool(!arg(0).is_true())),
            Builtin::Eq => eq(&arg(0), args.get(1..).unwrap_or_default()).map(Value::Bool),
            Builtin::Ne => {
                eq(&arg(0), args.get(1..2).unwrap_or_default()).map(|equal| Value::Bool(!equal))
            }
            Builtin::Lt => lt(&arg(0), &arg(1)).map(Value::Bool),
            Builtin::Le => le(&arg(0), &arg(1)).map(Value::Bool),
            Builtin::Gt => le(&arg(0), &arg(1)).map(|le| Value::Bool(!le)),
            Builtin::Ge => lt(&arg(0), &arg(1)).map(|lt| Value::Bool(!lt)),
        }
    }
}

/// Go's `evalArgs`: one string as it is, or `fmt.Sprint` of the arguments.
fn eval_args(args: &[Value]) -> String {
    if let [Value::String(s)] = args {
        return s.clone();
    }
    sprint(args)
}

/// Go's `indexArg`: an index for a slice of length `cap`.
fn index_arg(index: &Value, cap: usize) -> Result<usize, String> {
    let x = match index {
        Value::Int(n) => *n,
        Value::Nil => return Err("cannot index slice/array with nil".to_owned()),
        other => {
            return Err(format!(
                "cannot index slice/array with type {}",
                other.type_name()
            ));
        }
    };
    match usize::try_from(x) {
        Ok(i) if i <= cap => Ok(i),
        _ => Err(format!("index out of range: {x}")),
    }
}

/// The zero value of the element type of a map type such as `map[string]int`, which Go's
/// `index` returns for a missing key.
fn zero_value(map_type: &str) -> Value {
    match map_type.split_once(']').map(|(_, elem)| elem) {
        Some("string") => Value::String(String::new()),
        Some("int") => Value::Int(0),
        Some("float64") => Value::Float(0.0),
        Some("bool") => Value::Bool(false),
        _ => Value::Nil,
    }
}

/// Go's `index`.
fn index(item: &Value, indexes: &[Value]) -> Result<Value, String> {
    if matches!(item, Value::Nil) {
        return Err("index of untyped nil".to_owned());
    }
    let mut item = item.clone();
    for index in indexes {
        item = match &item {
            Value::Nil => return Err("index of nil pointer".to_owned()),
            Value::List(list) => {
                let x = index_arg(index, list.len())?;
                list.get(x)
                    .cloned()
                    .ok_or_else(|| "reflect: slice index out of range".to_owned())?
            }
            Value::String(s) => {
                let x = index_arg(index, s.len())?;
                let byte = s
                    .as_bytes()
                    .get(x)
                    .ok_or_else(|| "reflect: string index out of range".to_owned())?;
                Value::Int(i64::from(*byte))
            }
            Value::Map(map) => {
                let key = match index {
                    Value::String(key) => key,
                    Value::Nil => {
                        return Err("value is nil; should be of type string".to_owned());
                    }
                    other => {
                        return Err(format!(
                            "value has type {}; should be string",
                            other.type_name()
                        ));
                    }
                };
                map.get(key)
                    .cloned()
                    .unwrap_or_else(|| zero_value(map.type_name()))
            }
            other => {
                return Err(format!("can't index item of type {}", other.type_name()));
            }
        };
    }
    Ok(item)
}

/// Go's `slice`.
fn slice(item: &Value, indexes: &[Value]) -> Result<Value, String> {
    if matches!(item, Value::Nil) {
        return Err("slice of untyped nil".to_owned());
    }
    if indexes.len() > 3 {
        return Err(format!("too many slice indexes: {}", indexes.len()));
    }
    let (len, cap) = match item {
        Value::String(s) => {
            if indexes.len() == 3 {
                return Err("cannot 3-index slice a string".to_owned());
            }
            (s.len(), s.len())
        }
        Value::List(list) => (list.len(), list.len()),
        other => return Err(format!("can't slice item of type {}", other.type_name())),
    };
    let mut idx = [0, len, 0];
    for (i, index) in indexes.iter().enumerate() {
        idx[i] = index_arg(index, cap)?;
    }
    if idx[0] > idx[1] {
        return Err(format!("invalid slice index: {} > {}", idx[0], idx[1]));
    }
    if indexes.len() == 3 && idx[1] > idx[2] {
        return Err(format!("invalid slice index: {} > {}", idx[1], idx[2]));
    }
    Ok(match item {
        Value::String(s) => Value::String(
            String::from_utf8_lossy(s.as_bytes().get(idx[0]..idx[1]).unwrap_or_default())
                .into_owned(),
        ),
        Value::List(list) => Value::List(List::slice(list, idx[0], idx[1])),
        _ => Value::Nil,
    })
}

/// Go's `length`.
fn length(item: &Value) -> Result<i64, String> {
    let n = match item {
        Value::Nil => return Err("len of nil pointer".to_owned()),
        Value::String(s) => s.len(),
        Value::List(list) => list.len(),
        Value::Map(map) => map.len(),
        other => return Err(format!("len of type {}", other.type_name())),
    };
    Ok(i64::try_from(n).unwrap_or(i64::MAX))
}

#[derive(PartialEq, Eq)]
enum Kind {
    Invalid,
    Bool,
    Int,
    Float,
    String,
}

fn basic_kind(v: &Value) -> Kind {
    match v {
        Value::Bool(_) => Kind::Bool,
        Value::Int(_) => Kind::Int,
        Value::Float(_) => Kind::Float,
        Value::String(_) => Kind::String,
        _ => Kind::Invalid,
    }
}

const BAD_COMPARISON_TYPE: &str = "invalid type for comparison";

/// Go's `eq`: whether `arg1` equals any of `others`.
fn eq(arg1: &Value, others: &[Value]) -> Result<bool, String> {
    if others.is_empty() {
        return Err("missing argument for comparison".to_owned());
    }
    let k1 = basic_kind(arg1);
    for arg in others {
        let k2 = basic_kind(arg);
        #[allow(clippy::float_cmp)]
        let truth = if k1 == k2 {
            match (arg1, arg) {
                (Value::Bool(a), Value::Bool(b)) => a == b,
                (Value::Int(a), Value::Int(b)) => a == b,
                (Value::Float(a), Value::Float(b)) => a == b,
                (Value::String(a), Value::String(b)) => a == b,
                _ => compare_composite(arg1, arg)?,
            }
        } else {
            if !matches!(arg1, Value::Nil) && !matches!(arg, Value::Nil) {
                return Err(format!(
                    "incompatible types for comparison: {} and {}",
                    arg1.type_name(),
                    arg.type_name()
                ));
            }
            false
        };
        if truth {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The `default` arm of Go's `eq`, for values that are not booleans, numbers, or strings.
fn compare_composite(arg1: &Value, arg: &Value) -> Result<bool, String> {
    let same_kind = std::mem::discriminant(arg1) == std::mem::discriminant(arg);
    if !same_kind && !matches!(arg1, Value::Nil) && !matches!(arg, Value::Nil) {
        return Err(format!(
            "non-comparable types {}: {}, {}: {}",
            sprintf("%s", std::slice::from_ref(arg1)),
            arg1.type_name(),
            arg.type_name(),
            sprintf("%v", std::slice::from_ref(arg)),
        ));
    }
    if matches!(arg1, Value::Nil) || matches!(arg, Value::Nil) {
        return Ok(matches!(arg1, Value::Nil) == matches!(arg, Value::Nil));
    }
    match (arg1, arg) {
        (Value::Struct(a), Value::Struct(b)) => {
            if a.type_name().starts_with('*') {
                // Go compares pointers by address.
                Ok(a.type_name() == b.type_name() && a.same_fields(b))
            } else {
                Ok(a == b)
            }
        }
        _ => Err(format!(
            "non-comparable type {}: {}",
            sprintf("%s", std::slice::from_ref(arg)),
            arg.type_name()
        )),
    }
}

/// Go's `lt`.
fn lt(arg1: &Value, arg2: &Value) -> Result<bool, String> {
    let k1 = basic_kind(arg1);
    if k1 == Kind::Invalid {
        return Err(BAD_COMPARISON_TYPE.to_owned());
    }
    let k2 = basic_kind(arg2);
    if k2 == Kind::Invalid {
        return Err(BAD_COMPARISON_TYPE.to_owned());
    }
    if k1 != k2 {
        return Err(format!(
            "incompatible types for comparison: {} and {}",
            arg1.type_name(),
            arg2.type_name()
        ));
    }
    match (arg1, arg2) {
        (Value::Int(a), Value::Int(b)) => Ok(a < b),
        (Value::Float(a), Value::Float(b)) => Ok(a < b),
        (Value::String(a), Value::String(b)) => Ok(a < b),
        _ => Err(BAD_COMPARISON_TYPE.to_owned()),
    }
}

/// Go's `le`.
fn le(arg1: &Value, arg2: &Value) -> Result<bool, String> {
    if lt(arg1, arg2)? {
        return Ok(true);
    }
    eq(arg1, std::slice::from_ref(arg2))
}

/// Go's `HTMLEscapeString`.
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\0' => out.push('\u{fffd}'),
            '"' => out.push_str("&#34;"),
            '\'' => out.push_str("&#39;"),
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
    out
}

/// Go's `JSEscapeString`.
fn js_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '"' => out.push_str("\\\""),
            '<' => out.push_str("\\u003C"),
            '>' => out.push_str("\\u003E"),
            '&' => out.push_str("\\u0026"),
            '=' => out.push_str("\\u003D"),
            c if c < ' ' => {
                let _ =
                    std::fmt::Write::write_fmt(&mut out, format_args!("\\u00{:02X}", u32::from(c)));
            }
            c if !c.is_ascii() && !is_print(c) => {
                let _ =
                    std::fmt::Write::write_fmt(&mut out, format_args!("\\u{:04X}", u32::from(c)));
            }
            c => out.push(c),
        }
    }
    out
}

/// Go's `url.QueryEscape`.
fn query_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(byte));
            }
            b' ' => out.push('+'),
            _ => {
                let _ = std::fmt::Write::write_fmt(&mut out, format_args!("%{byte:02X}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_names_must_be_identifiers() {
        assert!(good_name("toPascalCase"));
        assert!(good_name("_x1"));
        assert!(good_name("é"));
        assert!(!good_name(""));
        assert!(!good_name("1x"));
        assert!(!good_name("a-b"));
    }

    #[test]
    fn query_escape_matches_go() {
        assert_eq!(
            query_escape("a b&c=d/é?~_-."),
            "a+b%26c%3Dd%2F%C3%A9%3F~_-."
        );
    }
}
