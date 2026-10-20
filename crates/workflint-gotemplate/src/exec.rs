//! The executor, a port of Go's `text/template/exec.go`.
//!
//! The state records the node it is at, as Go's `state.at` does, so that an error names the
//! same node and position as Go's.

use std::fmt::Display;
use std::io::Write;

use crate::Error;
use crate::fmt::sprint_one;
use crate::func::{Builtin, Function, Functions, Param};
use crate::parse::{Arg, Branch, Command, ListNode, Node, Number, Pipe, Tree};
use crate::value::Value;

/// A value with whether Go types it as `interface {}`: an item of a `[]interface {}` or a
/// value of a `map[string]interface {}` before a pipeline takes it out of the interface.
/// Error messages name that static type.
#[derive(Debug, Clone)]
struct Val {
    value: Value,
    iface: bool,
}

impl Val {
    fn concrete(value: Value) -> Val {
        Val {
            value,
            iface: false,
        }
    }

    fn type_name(&self) -> String {
        if self.iface {
            "interface {}".to_owned()
        } else {
            self.value.type_name()
        }
    }
}

/// The node the executor is at, for error messages.
#[derive(Clone, Copy)]
enum At<'t> {
    None,
    List(&'t ListNode),
    Node(&'t Node),
    Pipe(&'t Pipe),
    Command(&'t Command),
    Arg(&'t Arg),
}

impl At<'_> {
    fn pos(self) -> Option<usize> {
        Some(match self {
            At::None => return None,
            At::List(list) => list.pos,
            At::Node(node) => node.pos(),
            At::Pipe(pipe) => pipe.pos,
            At::Command(cmd) => cmd.pos,
            At::Arg(arg) => arg.pos(),
        })
    }

    fn context(self) -> String {
        match self {
            At::None => String::new(),
            At::List(list) => list.to_string(),
            At::Node(node) => node.to_string(),
            At::Pipe(pipe) => pipe.to_string(),
            At::Command(cmd) => cmd.to_string(),
            At::Arg(arg) => arg.to_string(),
        }
    }
}

/// How a walk ended: normally, or at `{{break}}` or `{{continue}}`.
enum Flow {
    Normal,
    Break,
    Continue,
}

struct Variable {
    name: String,
    value: Val,
}

/// A function found by name.
enum Callee<'f> {
    User(&'f Function),
    Builtin(Builtin),
}

impl Callee<'_> {
    fn signature(&self) -> (&[Param], Option<Param>) {
        match self {
            Callee::User(function) => (&function.params, function.variadic),
            Callee::Builtin(builtin) => builtin.signature(),
        }
    }
}

type Result<T> = std::result::Result<T, Error>;

pub(crate) struct State<'t, W> {
    name: &'t str,
    text: &'t str,
    functions: &'t Functions,
    out: &'t mut W,
    node: At<'t>,
    vars: Vec<Variable>,
}

/// Runs `tree` with `data` as dot and `$`, writing to `out`.
pub(crate) fn execute<W: Write>(
    name: &str,
    text: &str,
    tree: &Tree,
    functions: &Functions,
    out: &mut W,
    data: &Value,
) -> Result<()> {
    let data = Val::concrete(data.clone());
    let mut state = State {
        name,
        text,
        functions,
        out,
        node: At::None,
        vars: vec![Variable {
            name: "$".to_owned(),
            value: data.clone(),
        }],
    };
    state.walk_list(&data, &tree.root)?;
    Ok(())
}

impl<'t, W: Write> State<'t, W> {
    fn at(&mut self, node: At<'t>) {
        self.node = node;
    }

    /// Go's `state.errorf`.
    fn error<T>(&self, message: impl Display) -> Result<T> {
        let name = self.name;
        let Some(pos) = self.node.pos() else {
            return Err(Error::Exec(format!("template: {name}: {message}")));
        };
        let before = self.text.get(..pos).unwrap_or(self.text);
        let line = 1 + before.bytes().filter(|b| *b == b'\n').count();
        let column = before.rfind('\n').map_or(pos, |newline| pos - newline - 1);
        Err(Error::Exec(format!(
            "template: {name}:{line}:{column}: executing {} at <{}>: {message}",
            crate::fmt::quote(name),
            self.node.context()
        )))
    }

    fn write(&mut self, text: &str) -> Result<()> {
        self.out.write_all(text.as_bytes()).map_err(Error::Write)
    }

    fn mark(&self) -> usize {
        self.vars.len()
    }

    fn pop(&mut self, mark: usize) {
        self.vars.truncate(mark);
    }

    fn push(&mut self, name: &str, value: Val) {
        self.vars.push(Variable {
            name: name.to_owned(),
            value,
        });
    }

    fn set_var(&mut self, name: &str, value: Val) -> Result<()> {
        if let Some(variable) = self.vars.iter_mut().rev().find(|v| v.name == name) {
            variable.value = value;
            return Ok(());
        }
        self.error(format_args!("undefined variable: {name}"))
    }

    /// Go's `setTopVar`: sets the `n`th variable from the top of the stack.
    fn set_top_var(&mut self, n: usize, value: Val) {
        if let Some(index) = self.vars.len().checked_sub(n)
            && let Some(variable) = self.vars.get_mut(index)
        {
            variable.value = value;
        }
    }

    fn var_value(&self, name: &str) -> Result<Val> {
        if let Some(variable) = self.vars.iter().rev().find(|v| v.name == name) {
            return Ok(variable.value.clone());
        }
        self.error(format_args!("undefined variable: {name}"))
    }

    fn walk_list(&mut self, dot: &Val, list: &'t ListNode) -> Result<Flow> {
        self.at(At::List(list));
        for node in &list.nodes {
            match self.walk(dot, node)? {
                Flow::Normal => {}
                flow => return Ok(flow),
            }
        }
        Ok(Flow::Normal)
    }

    fn walk(&mut self, dot: &Val, node: &'t Node) -> Result<Flow> {
        self.at(At::Node(node));
        match node {
            Node::Action { pipe, .. } => {
                let value = self.eval_pipeline(dot, pipe)?;
                if pipe.decl.is_empty() {
                    self.at(At::Node(node));
                    let text = match value.value {
                        Value::Nil => "<no value>".to_owned(),
                        value => sprint_one(&value),
                    };
                    self.write(&text)?;
                }
                Ok(Flow::Normal)
            }
            Node::Break { .. } => Ok(Flow::Break),
            Node::Continue { .. } => Ok(Flow::Continue),
            Node::If(branch) => self.walk_if_or_with(false, dot, branch),
            Node::With(branch) => self.walk_if_or_with(true, dot, branch),
            Node::Range(branch) => self.walk_range(dot, node, branch),
            Node::Text { text, .. } => {
                self.write(text)?;
                Ok(Flow::Normal)
            }
        }
    }

    fn walk_if_or_with(&mut self, with: bool, dot: &Val, branch: &'t Branch) -> Result<Flow> {
        let mark = self.mark();
        let result = self.walk_if_or_with_inner(with, dot, branch);
        self.pop(mark);
        result
    }

    fn walk_if_or_with_inner(&mut self, with: bool, dot: &Val, branch: &'t Branch) -> Result<Flow> {
        let value = self.eval_pipeline(dot, &branch.pipe)?;
        if value.value.is_true() {
            if with {
                self.walk_list(&value, &branch.list)
            } else {
                self.walk_list(dot, &branch.list)
            }
        } else if let Some(else_list) = &branch.else_list {
            self.walk_list(dot, else_list)
        } else {
            Ok(Flow::Normal)
        }
    }

    fn walk_range(&mut self, dot: &Val, node: &'t Node, branch: &'t Branch) -> Result<Flow> {
        self.at(At::Node(node));
        let mark = self.mark();
        let result = self.walk_range_inner(dot, branch);
        self.pop(mark);
        result.map(|()| Flow::Normal)
    }

    fn walk_range_inner(&mut self, dot: &Val, branch: &'t Branch) -> Result<()> {
        let value = self.eval_pipeline(dot, &branch.pipe)?.value;
        let mark = self.mark();
        let mut ran = false;
        match &value {
            Value::Int(n) => {
                if branch.pipe.decl.len() > 1 {
                    return self.error(format_args!(
                        "can't use {} to iterate over more than one variable",
                        sprint_one(&value)
                    ));
                }
                for i in 0..*n {
                    ran = true;
                    let elem = Val::concrete(Value::Int(i));
                    if self.one_iteration(branch, mark, Val::concrete(Value::Nil), &elem)? {
                        break;
                    }
                }
            }
            Value::List(list) => {
                let iface = list.has_interface_items();
                for (i, item) in list.iter().enumerate() {
                    ran = true;
                    let index = Val::concrete(Value::Int(i64::try_from(i).unwrap_or(i64::MAX)));
                    let elem = Val {
                        value: item.clone(),
                        iface,
                    };
                    if self.one_iteration(branch, mark, index, &elem)? {
                        break;
                    }
                }
            }
            Value::Map(map) => {
                let iface = map.has_interface_values();
                for (key, item) in map.iter() {
                    ran = true;
                    let index = Val::concrete(Value::String(key.clone()));
                    let elem = Val {
                        value: item.clone(),
                        iface,
                    };
                    if self.one_iteration(branch, mark, index, &elem)? {
                        break;
                    }
                }
            }
            Value::Nil => {}
            other => {
                return self.error(format_args!(
                    "range can't iterate over {}",
                    sprint_one(other)
                ));
            }
        }
        if !ran && let Some(else_list) = &branch.else_list {
            self.walk_list(dot, else_list)?;
        }
        Ok(())
    }

    /// Go's `oneIteration`. Returns whether the body ran `{{break}}`.
    fn one_iteration(
        &mut self,
        branch: &'t Branch,
        mark: usize,
        index: Val,
        elem: &Val,
    ) -> Result<bool> {
        let decl = &branch.pipe.decl;
        if let Some(first) = decl.first() {
            if branch.pipe.is_assign {
                if decl.len() > 1 {
                    self.set_var(first, index.clone())?;
                } else {
                    self.set_var(first, elem.clone())?;
                }
            } else {
                self.set_top_var(1, elem.clone());
            }
        }
        if let Some(second) = decl.get(1) {
            if branch.pipe.is_assign {
                self.set_var(second, elem.clone())?;
            } else {
                self.set_top_var(2, index);
            }
        }
        let flow = self.walk_list(elem, &branch.list);
        self.pop(mark);
        Ok(matches!(flow?, Flow::Break))
    }

    /// Go's `evalPipeline`. Declared variables stay on the stack for the caller to pop.
    fn eval_pipeline(&mut self, dot: &Val, pipe: &'t Pipe) -> Result<Val> {
        self.at(At::Pipe(pipe));
        let mut value: Option<Val> = None;
        for cmd in &pipe.cmds {
            let mut result = self.eval_command(dot, cmd, value.take())?;
            // Go takes the value out of an `interface {}`.
            result.iface = false;
            value = Some(result);
        }
        let value = value.unwrap_or_else(|| Val::concrete(Value::Nil));
        for name in &pipe.decl {
            if pipe.is_assign {
                self.set_var(name, value.clone())?;
            } else {
                self.push(name, value.clone());
            }
        }
        Ok(value)
    }

    fn not_a_function(&self, args: &[Arg], final_value: Option<&Val>) -> Result<()> {
        if args.len() > 1 || final_value.is_some() {
            let first = args.first().map(ToString::to_string).unwrap_or_default();
            return self.error(format_args!("can't give argument to non-function {first}"));
        }
        Ok(())
    }

    fn eval_command(
        &mut self,
        dot: &Val,
        cmd: &'t Command,
        final_value: Option<Val>,
    ) -> Result<Val> {
        let Some(first) = cmd.args.first() else {
            return self.error("empty command");
        };
        match first {
            Arg::Field { ident, .. } => {
                self.at(At::Arg(first));
                return self.eval_field_chain(dot.clone(), ident, &cmd.args, final_value.as_ref());
            }
            Arg::Chain { .. } => {
                return self.eval_chain(dot, first, &cmd.args, final_value.as_ref());
            }
            Arg::Identifier { name, .. } => {
                return self.eval_function(
                    dot,
                    first,
                    name,
                    At::Command(cmd),
                    Some(&cmd.args),
                    final_value,
                );
            }
            Arg::Pipe(pipe) => {
                self.not_a_function(&cmd.args, final_value.as_ref())?;
                return self.eval_pipeline(dot, pipe);
            }
            Arg::Variable { ident, .. } => {
                return self.eval_variable(first, ident, &cmd.args, final_value.as_ref());
            }
            _ => {}
        }
        self.at(At::Arg(first));
        self.not_a_function(&cmd.args, final_value.as_ref())?;
        match first {
            Arg::Bool { value, .. } => Ok(Val::concrete(Value::Bool(*value))),
            Arg::Dot { .. } => Ok(dot.clone()),
            Arg::Nil { .. } => self.error("nil is not a command"),
            Arg::Number { number, .. } => self.ideal_constant(first, number).map(Val::concrete),
            Arg::String { text, .. } => Ok(Val::concrete(Value::String(text.clone()))),
            other => self.error(format_args!(
                "can't evaluate command {}",
                crate::fmt::quote(&other.to_string())
            )),
        }
    }

    /// Go's `idealConstant`: the value of a number whose type only its syntax tells.
    fn ideal_constant(&mut self, node: &'t Arg, number: &Number) -> Result<Value> {
        self.at(At::Arg(node));
        let text = &number.text;
        let is_hex_int = text.len() > 2
            && text.starts_with('0')
            && matches!(text.as_bytes()[1], b'x' | b'X')
            && !text.contains(['p', 'P']);
        let is_rune_int = text.starts_with('\'');
        if let Some(f) = number.float
            && !is_hex_int
            && !is_rune_int
            && text.contains(['.', 'e', 'E', 'p', 'P'])
        {
            return Ok(Value::Float(f));
        }
        if let Some(n) = number.int {
            return Ok(Value::Int(n));
        }
        if number.is_uint {
            return self.error(format_args!("{text} overflows int"));
        }
        Ok(Value::Nil)
    }

    fn eval_chain(
        &mut self,
        dot: &Val,
        chain: &'t Arg,
        args: &'t [Arg],
        final_value: Option<&Val>,
    ) -> Result<Val> {
        self.at(At::Arg(chain));
        let Arg::Chain { node, fields, .. } = chain else {
            return self.error("internal error: no fields in evalChainNode");
        };
        if let Arg::Nil { .. } = node.as_ref() {
            return self.error(format_args!("indirection through explicit nil in {chain}"));
        }
        let receiver = self.eval_arg(dot, None, node)?;
        self.eval_field_chain(Val::concrete(receiver), fields, args, final_value)
    }

    fn eval_variable(
        &mut self,
        node: &'t Arg,
        ident: &[String],
        args: &'t [Arg],
        final_value: Option<&Val>,
    ) -> Result<Val> {
        self.at(At::Arg(node));
        let name = ident.first().map_or("", String::as_str);
        let value = self.var_value(name)?;
        if ident.len() == 1 {
            self.not_a_function(args, final_value)?;
            return Ok(value);
        }
        self.eval_field_chain(value, &ident[1..], args, final_value)
    }

    /// Go's `evalFieldChain`: `.X.Y.Z` on `receiver`, where only the last field gets `args`.
    fn eval_field_chain(
        &mut self,
        mut receiver: Val,
        ident: &[String],
        args: &[Arg],
        final_value: Option<&Val>,
    ) -> Result<Val> {
        let Some((last, init)) = ident.split_last() else {
            return Ok(receiver);
        };
        for name in init {
            receiver = self.eval_field(name, false, &receiver)?;
        }
        let has_args = args.len() > 1 || final_value.is_some();
        self.eval_field(last, has_args, &receiver)
    }

    /// Go's `evalField`: a field of a struct or a key of a map.
    fn eval_field(&self, name: &str, has_args: bool, receiver: &Val) -> Result<Val> {
        match &receiver.value {
            Value::Nil if receiver.iface => {
                self.error(format_args!("nil pointer evaluating interface {{}}.{name}"))
            }
            Value::Nil => Ok(Val::concrete(Value::Nil)),
            Value::Struct(record) => match record.get(name) {
                Some(field) => {
                    if has_args {
                        return self.error(format_args!(
                            "{name} has arguments but cannot be invoked as function"
                        ));
                    }
                    Ok(Val::concrete(field.clone()))
                }
                None => self.error(format_args!(
                    "can't evaluate field {name} in type {}",
                    receiver.type_name()
                )),
            },
            Value::Map(map) => {
                if has_args {
                    return self.error(format_args!("{name} is not a method but has arguments"));
                }
                // A missing key gives Go's invalid value, not a nil interface.
                Ok(match map.get(name) {
                    Some(value) => Val {
                        value: value.clone(),
                        iface: map.has_interface_values(),
                    },
                    None => Val::concrete(Value::Nil),
                })
            }
            _ => self.error(format_args!(
                "can't evaluate field {name} in type {}",
                receiver.type_name()
            )),
        }
    }

    fn find_function(&self, name: &str) -> Option<Callee<'t>> {
        if let Some(function) = self.functions.get(name) {
            return Some(Callee::User(function));
        }
        Builtin::from_name(name).map(Callee::Builtin)
    }

    /// Go's `evalFunction` and `evalCall`. `args` includes the function's own node first;
    /// `None` means no arguments.
    fn eval_function(
        &mut self,
        dot: &Val,
        ident: &'t Arg,
        name: &str,
        node: At<'t>,
        args: Option<&'t [Arg]>,
        final_value: Option<Val>,
    ) -> Result<Val> {
        self.at(At::Arg(ident));
        let Some(callee) = self.find_function(name) else {
            return self.error(format_args!(
                "{} is not a defined function",
                crate::fmt::quote(name)
            ));
        };
        let args = args.map_or(&[][..], |args| args.get(1..).unwrap_or_default());
        let (params, variadic) = callee.signature();
        let num_in = args.len() + usize::from(final_value.is_some());
        let num_fixed = if variadic.is_some() {
            if num_in < params.len() {
                return self.error(format_args!(
                    "wrong number of args for {name}: want at least {} got {}",
                    params.len(),
                    args.len()
                ));
            }
            params.len()
        } else {
            if num_in != params.len() {
                return self.error(format_args!(
                    "wrong number of args for {name}: want {} got {num_in}",
                    params.len()
                ));
            }
            args.len()
        };

        if let Callee::Builtin(builtin @ (Builtin::And | Builtin::Or)) = callee {
            let want = builtin == Builtin::Or;
            let mut value = Value::Nil;
            for arg in args {
                value = self.eval_arg(dot, Some(Param::Any), arg)?;
                if value.is_true() == want {
                    return Ok(Val::concrete(value));
                }
            }
            if let Some(final_value) = final_value {
                value = final_value.value;
            }
            return Ok(Val::concrete(value));
        }

        let mut values = Vec::with_capacity(num_in);
        for (index, arg) in args.iter().enumerate() {
            let param = if index < num_fixed {
                params.get(index).copied()
            } else {
                variadic
            };
            values.push(self.eval_arg(dot, Some(param.unwrap_or(Param::Any)), arg)?);
        }
        if let Some(final_value) = final_value {
            let param = match variadic {
                Some(variadic) if num_in > num_fixed => variadic,
                _ => params.get(num_in - 1).copied().unwrap_or(Param::Any),
            };
            values.push(self.validate_type(final_value.value, Some(param))?);
        }

        let result = match &callee {
            Callee::User(function) => (function.body)(&values),
            Callee::Builtin(Builtin::Call) => Err(call_error(args, values.first())),
            Callee::Builtin(builtin) => builtin.call(&values),
        };
        match result {
            Ok(value) => Ok(Val::concrete(value)),
            Err(message) => {
                self.at(node);
                self.error(format_args!("error calling {name}: {message}"))
            }
        }
    }

    /// Go's `validateType`: checks that a computed value fits the parameter.
    fn validate_type(&self, value: Value, param: Option<Param>) -> Result<Value> {
        let Some(param) = param else {
            return Ok(value);
        };
        let fits = matches!(
            (param, &value),
            (Param::Any, _)
                | (Param::String, Value::String(_))
                | (Param::Int, Value::Int(_))
                | (Param::Float, Value::Float(_))
                | (Param::Bool, Value::Bool(_))
        );
        if fits {
            return Ok(value);
        }
        if let Value::Nil = value {
            return self.error(format_args!("invalid value; expected {}", param.go_name()));
        }
        self.error(format_args!(
            "wrong type for value; expected {}; got {}",
            param.go_name(),
            value.type_name()
        ))
    }

    /// Go's `evalArg`: an argument of type `param`, or of no type for a chain's receiver.
    fn eval_arg(&mut self, dot: &Val, param: Option<Param>, arg: &'t Arg) -> Result<Value> {
        self.at(At::Arg(arg));
        match arg {
            Arg::Dot { .. } => return self.validate_type(dot.value.clone(), param),
            Arg::Nil { .. } => {
                return match param {
                    None | Some(Param::Any) => Ok(Value::Nil),
                    Some(param) => {
                        self.error(format_args!("cannot assign nil to {}", param.go_name()))
                    }
                };
            }
            Arg::Field { ident, .. } => {
                let value =
                    self.eval_field_chain(dot.clone(), ident, std::slice::from_ref(arg), None)?;
                return self.validate_type(value.value, param);
            }
            Arg::Variable { ident, .. } => {
                let value = self.eval_variable(arg, ident, &[], None)?;
                return self.validate_type(value.value, param);
            }
            Arg::Pipe(pipe) => {
                let value = self.eval_pipeline(dot, pipe)?;
                return self.validate_type(value.value, param);
            }
            Arg::Identifier { name, .. } => {
                let value = self.eval_function(dot, arg, name, At::Arg(arg), None, None)?;
                return self.validate_type(value.value, param);
            }
            Arg::Chain { .. } => {
                let value = self.eval_chain(dot, arg, &[], None)?;
                return self.validate_type(value.value, param);
            }
            _ => {}
        }
        match (param, arg) {
            (None | Some(Param::Any), Arg::Bool { value, .. })
            | (Some(Param::Bool), Arg::Bool { value, .. }) => Ok(Value::Bool(*value)),
            (None | Some(Param::Any), Arg::Number { number, .. }) => {
                self.ideal_constant(arg, number)
            }
            (None | Some(Param::Any), Arg::String { text, .. })
            | (Some(Param::String), Arg::String { text, .. }) => Ok(Value::String(text.clone())),
            (Some(Param::Int), Arg::Number { number, .. }) if number.int.is_some() => {
                Ok(Value::Int(number.int.unwrap_or_default()))
            }
            (Some(Param::Float), Arg::Number { number, .. }) if number.float.is_some() => {
                Ok(Value::Float(number.float.unwrap_or_default()))
            }
            (Some(Param::Bool), _) => self.error(format_args!("expected bool; found {arg}")),
            (Some(Param::String), _) => self.error(format_args!("expected string; found {arg}")),
            (Some(Param::Int), _) => self.error(format_args!("expected integer; found {arg}")),
            (Some(Param::Float), _) => self.error(format_args!("expected float; found {arg}")),
            (None | Some(Param::Any), _) => self.error(format_args!(
                "can't handle assignment of {arg} to empty interface argument"
            )),
        }
    }
}

/// The error of Go's `call` built-in, given that no value is a function here.
fn call_error(args: &[Arg], function: Option<&Value>) -> String {
    match function {
        None | Some(Value::Nil) => "call of nil".to_owned(),
        Some(value) => {
            let name = match args.first() {
                Some(arg) => arg.to_string(),
                None => match value {
                    Value::String(s) => s.clone(),
                    other => format!("<{} Value>", other.type_name()),
                },
            };
            format!("non-function {name} of type {}", value.type_name())
        }
    }
}
