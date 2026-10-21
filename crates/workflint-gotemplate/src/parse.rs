//! The syntax tree and the parser, a port of Go's `text/template/parse/node.go` and
//! `parse.go`.
//!
//! Each node keeps its byte offset, which execution errors turn into a line and column, and
//! displays as Go's `Node.String` does, which execution errors quote.

use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};

use crate::lex::{Item, ItemKind, Lexer};
use crate::strconv;

/// How deep parentheses and control structures may nest. Go allows 10,000 levels of
/// parentheses, or 1,000 on WebAssembly, and does not limit control structures. A limit for
/// both keeps the recursive parser and executor within the 2 MiB stack of a test thread in a
/// debug build.
pub(crate) const MAX_DEPTH: usize = 100;

/// A parsed template.
#[derive(Debug)]
pub(crate) struct Tree {
    /// The templates by name: the template itself under the name it was parsed with, and each
    /// template that `{{define}}` or `{{block}}` defines.
    pub(crate) templates: BTreeMap<String, ListNode>,
}

#[derive(Debug)]
pub(crate) struct ListNode {
    pub(crate) pos: usize,
    pub(crate) nodes: Vec<Node>,
}

#[derive(Debug)]
pub(crate) enum Node {
    Text {
        pos: usize,
        text: String,
    },
    Action {
        pos: usize,
        pipe: Pipe,
    },
    If(Branch),
    Range(Branch),
    With(Branch),
    Break {
        pos: usize,
    },
    Continue {
        pos: usize,
    },
    /// `{{template "name" pipeline}}`, which `{{block}}` also produces.
    Template {
        pos: usize,
        name: String,
        pipe: Option<Pipe>,
    },
}

/// The common part of `if`, `range`, and `with`.
#[derive(Debug)]
pub(crate) struct Branch {
    pub(crate) pos: usize,
    pub(crate) pipe: Pipe,
    pub(crate) list: ListNode,
    pub(crate) else_list: Option<ListNode>,
}

/// A pipeline with its optional variable declarations.
#[derive(Debug)]
pub(crate) struct Pipe {
    pub(crate) pos: usize,
    pub(crate) is_assign: bool,
    pub(crate) decl: Vec<String>,
    pub(crate) cmds: Vec<Command>,
}

#[derive(Debug)]
pub(crate) struct Command {
    pub(crate) pos: usize,
    pub(crate) args: Vec<Arg>,
}

/// An operand of a command.
#[derive(Debug)]
pub(crate) enum Arg {
    /// `.A.B`, with the names without dots.
    Field {
        pos: usize,
        ident: Vec<String>,
    },
    /// `(pipe).A.B` or `$x.A.B` on a non-variable term.
    Chain {
        pos: usize,
        node: Box<Arg>,
        fields: Vec<String>,
    },
    Identifier {
        pos: usize,
        name: String,
    },
    /// `$x.A.B`, with `$x` first.
    Variable {
        pos: usize,
        ident: Vec<String>,
    },
    Dot {
        pos: usize,
    },
    Nil {
        pos: usize,
    },
    Bool {
        pos: usize,
        value: bool,
    },
    Number {
        pos: usize,
        number: Number,
    },
    String {
        pos: usize,
        quoted: String,
        text: String,
    },
    Pipe(Box<Pipe>),
}

/// A number literal under every type that can hold it, as Go's `NumberNode`.
#[derive(Debug)]
pub(crate) struct Number {
    pub(crate) text: String,
    pub(crate) int: Option<i64>,
    pub(crate) is_uint: bool,
    pub(crate) float: Option<f64>,
}

impl Arg {
    pub(crate) fn pos(&self) -> usize {
        match self {
            Arg::Field { pos, .. }
            | Arg::Chain { pos, .. }
            | Arg::Identifier { pos, .. }
            | Arg::Variable { pos, .. }
            | Arg::Dot { pos }
            | Arg::Nil { pos }
            | Arg::Bool { pos, .. }
            | Arg::Number { pos, .. }
            | Arg::String { pos, .. } => *pos,
            Arg::Pipe(pipe) => pipe.pos,
        }
    }
}

impl Node {
    pub(crate) fn pos(&self) -> usize {
        match self {
            Node::Text { pos, .. }
            | Node::Action { pos, .. }
            | Node::Break { pos }
            | Node::Continue { pos }
            | Node::Template { pos, .. } => *pos,
            Node::If(branch) | Node::Range(branch) | Node::With(branch) => branch.pos,
        }
    }
}

impl Display for ListNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for node in &self.nodes {
            node.fmt(f)?;
        }
        Ok(())
    }
}

impl Display for Node {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Node::Text { text, .. } => f.write_str(text),
            Node::Action { pipe, .. } => write!(f, "{{{{{pipe}}}}}"),
            Node::If(branch) => branch.write(f, "if"),
            Node::Range(branch) => branch.write(f, "range"),
            Node::With(branch) => branch.write(f, "with"),
            Node::Break { .. } => f.write_str("{{break}}"),
            Node::Continue { .. } => f.write_str("{{continue}}"),
            Node::Template { name, pipe, .. } => {
                write!(f, "{{{{template {}", crate::fmt::quote(name))?;
                if let Some(pipe) = pipe {
                    write!(f, " {pipe}")?;
                }
                f.write_str("}}")
            }
        }
    }
}

impl Branch {
    fn write(&self, f: &mut Formatter<'_>, name: &str) -> fmt::Result {
        write!(f, "{{{{{name} {}}}}}{}", self.pipe, self.list)?;
        if let Some(else_list) = &self.else_list {
            write!(f, "{{{{else}}}}{else_list}")?;
        }
        f.write_str("{{end}}")
    }
}

impl Display for Pipe {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if !self.decl.is_empty() {
            f.write_str(&self.decl.join(", "))?;
            f.write_str(if self.is_assign { " = " } else { " := " })?;
        }
        for (index, cmd) in self.cmds.iter().enumerate() {
            if index > 0 {
                f.write_str(" | ")?;
            }
            cmd.fmt(f)?;
        }
        Ok(())
    }
}

impl Display for Command {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for (index, arg) in self.args.iter().enumerate() {
            if index > 0 {
                f.write_str(" ")?;
            }
            if let Arg::Pipe(pipe) = arg {
                write!(f, "({pipe})")?;
            } else {
                arg.fmt(f)?;
            }
        }
        Ok(())
    }
}

impl Display for Arg {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Arg::Field { ident, .. } => {
                for name in ident {
                    write!(f, ".{name}")?;
                }
                Ok(())
            }
            Arg::Chain { node, fields, .. } => {
                if let Arg::Pipe(pipe) = node.as_ref() {
                    write!(f, "({pipe})")?;
                } else {
                    node.fmt(f)?;
                }
                for name in fields {
                    write!(f, ".{name}")?;
                }
                Ok(())
            }
            Arg::Identifier { name, .. } => f.write_str(name),
            Arg::Variable { ident, .. } => f.write_str(&ident.join(".")),
            Arg::Dot { .. } => f.write_str("."),
            Arg::Nil { .. } => f.write_str("nil"),
            Arg::Bool { value, .. } => write!(f, "{value}"),
            Arg::Number { number, .. } => f.write_str(&number.text),
            Arg::String { quoted, .. } => f.write_str(quoted),
            Arg::Pipe(pipe) => pipe.fmt(f),
        }
    }
}

/// What `itemList` and `textOrAction` return: a node, or the `{{end}}` or `{{else}}` that ends
/// a list.
enum Parsed {
    Node(Node),
    End,
    Else { pos: usize },
}

impl Parsed {
    fn describe(&self) -> String {
        match self {
            Parsed::Node(node) => node.to_string(),
            Parsed::End => "{{end}}".to_owned(),
            Parsed::Else { .. } => "{{else}}".to_owned(),
        }
    }
}

/// A parse error, already in Go's wording.
pub(crate) struct ParseError(pub(crate) String);

type Result<T> = std::result::Result<T, ParseError>;

pub(crate) struct Parser<'a, F> {
    name: &'a str,
    lex: Lexer<'a>,
    token: [Item; 3],
    peek_count: usize,
    vars: Vec<String>,
    action_line: usize,
    range_depth: usize,
    depth: usize,
    has_function: F,
    templates: BTreeMap<String, ListNode>,
}

fn eof_item() -> Item {
    Item {
        kind: ItemKind::Eof,
        pos: 0,
        val: String::new(),
        line: 0,
    }
}

/// Parses `text` as the template `name`. `has_function` tells whether a function name is
/// defined.
pub(crate) fn parse(
    name: &str,
    text: &str,
    has_function: impl Fn(&str) -> bool,
) -> std::result::Result<Tree, ParseError> {
    let lex = Lexer::new(text, !has_function("break"), !has_function("continue"));
    let mut parser = Parser {
        name,
        lex,
        token: [eof_item(), eof_item(), eof_item()],
        peek_count: 0,
        vars: vec!["$".to_owned()],
        action_line: 0,
        range_depth: 0,
        depth: 0,
        has_function,
        templates: BTreeMap::new(),
    };
    let root = parser.parse_root()?;
    parser.add(name, root)?;
    Ok(Tree {
        templates: parser.templates,
    })
}

impl<F: Fn(&str) -> bool> Parser<'_, F> {
    fn next(&mut self) -> Item {
        if self.peek_count > 0 {
            self.peek_count -= 1;
        } else {
            self.token[0] = self.lex.next_item();
        }
        self.token[self.peek_count].clone()
    }

    fn backup(&mut self) {
        self.peek_count += 1;
    }

    fn backup2(&mut self, t1: Item) {
        self.token[1] = t1;
        self.peek_count = 2;
    }

    fn backup3(&mut self, t2: Item, t1: Item) {
        self.token[1] = t1;
        self.token[2] = t2;
        self.peek_count = 3;
    }

    fn peek(&mut self) -> Item {
        if self.peek_count > 0 {
            return self.token[self.peek_count - 1].clone();
        }
        self.peek_count = 1;
        self.token[0] = self.lex.next_item();
        self.token[0].clone()
    }

    fn next_non_space(&mut self) -> Item {
        loop {
            let token = self.next();
            if token.kind != ItemKind::Space {
                return token;
            }
        }
    }

    fn peek_non_space(&mut self) -> Item {
        let token = self.next_non_space();
        self.backup();
        token
    }

    /// Go's `errorf`: the message with the template name and the line of the last token read.
    fn error<T>(&self, message: impl Display) -> Result<T> {
        Err(ParseError(format!(
            "template: {}:{}: {message}",
            self.name, self.token[0].line
        )))
    }

    fn expect(&mut self, expected: ItemKind, context: &str) -> Result<Item> {
        let token = self.next_non_space();
        if token.kind != expected {
            return self.unexpected(&token, context);
        }
        Ok(token)
    }

    fn unexpected<T>(&self, token: &Item, context: &str) -> Result<T> {
        if token.kind == ItemKind::Error {
            let mut extra = String::new();
            if self.action_line != 0 && self.action_line != token.line {
                extra = format!(" in action started at {}:{}", self.name, self.action_line);
                if token.val.ends_with(" action") {
                    extra = extra[" in action".len()..].to_owned();
                }
            }
            return self.error(format_args!("{token}{extra}"));
        }
        self.error(format_args!("unexpected {token} in {context}"))
    }

    fn enter(&mut self) -> Result<()> {
        if self.depth >= MAX_DEPTH {
            return self.error("max expression depth exceeded");
        }
        self.depth += 1;
        Ok(())
    }

    fn parse_root(&mut self) -> Result<ListNode> {
        let mut root = ListNode {
            pos: self.peek().pos,
            nodes: Vec::new(),
        };
        while self.peek().kind != ItemKind::Eof {
            if self.peek().kind == ItemKind::LeftDelim {
                let delim = self.next();
                if self.next_non_space().kind == ItemKind::Define {
                    self.parse_definition()?;
                    continue;
                }
                self.backup2(delim);
            }
            match self.text_or_action()? {
                parsed @ (Parsed::End | Parsed::Else { .. }) => {
                    return self.error(format_args!("unexpected {}", parsed.describe()));
                }
                Parsed::Node(node) => root.nodes.push(node),
            }
        }
        Ok(root)
    }

    fn item_list(&mut self) -> Result<(ListNode, Parsed)> {
        let mut list = ListNode {
            pos: self.peek_non_space().pos,
            nodes: Vec::new(),
        };
        while self.peek_non_space().kind != ItemKind::Eof {
            match self.text_or_action()? {
                Parsed::Node(node) => list.nodes.push(node),
                end => return Ok((list, end)),
            }
        }
        self.error("unexpected EOF")
    }

    fn text_or_action(&mut self) -> Result<Parsed> {
        let token = self.next_non_space();
        match token.kind {
            ItemKind::Text => Ok(Parsed::Node(Node::Text {
                pos: token.pos,
                text: token.val,
            })),
            ItemKind::LeftDelim => {
                self.action_line = token.line;
                let result = self.action();
                self.action_line = 0;
                result
            }
            _ => self.unexpected(&token, "input"),
        }
    }

    fn action(&mut self) -> Result<Parsed> {
        let token = self.next_non_space();
        match token.kind {
            ItemKind::Break | ItemKind::Continue => {
                let is_break = token.kind == ItemKind::Break;
                let context = if is_break {
                    "{{break}}"
                } else {
                    "{{continue}}"
                };
                let next = self.next_non_space();
                if next.kind != ItemKind::RightDelim {
                    return self.unexpected(&next, context);
                }
                if self.range_depth == 0 {
                    return self.error(format_args!("{context} outside {{{{range}}}}"));
                }
                let pos = token.pos;
                return Ok(Parsed::Node(if is_break {
                    Node::Break { pos }
                } else {
                    Node::Continue { pos }
                }));
            }
            ItemKind::Else => return self.else_control(),
            ItemKind::End => {
                self.expect(ItemKind::RightDelim, "end")?;
                return Ok(Parsed::End);
            }
            ItemKind::If => return Ok(Parsed::Node(Node::If(self.parse_control("if")?))),
            ItemKind::Range => {
                return Ok(Parsed::Node(Node::Range(self.parse_control("range")?)));
            }
            ItemKind::With => return Ok(Parsed::Node(Node::With(self.parse_control("with")?))),
            ItemKind::Block => return self.block_control().map(Parsed::Node),
            ItemKind::Template => return self.template_control().map(Parsed::Node),
            _ => {}
        }
        self.backup();
        let pos = self.peek().pos;
        let pipe = self.pipeline("command", ItemKind::RightDelim)?;
        Ok(Parsed::Node(Node::Action { pos, pipe }))
    }

    /// Go's `add`: stores a template unless a non-empty one has its name.
    fn add(&mut self, name: &str, root: ListNode) -> Result<()> {
        match self.templates.get(name) {
            Some(existing) if !is_empty_tree(existing) => {
                if !is_empty_tree(&root) {
                    return self.error(format_args!(
                        "template: multiple definition of template {}",
                        crate::fmt::quote(name)
                    ));
                }
            }
            _ => {
                self.templates.insert(name.to_owned(), root);
            }
        }
        Ok(())
    }

    /// Parses the list of a `{{define}}` or `{{block}}` as Go does in a new tree: with only
    /// `$` defined and outside any `range`. A `{{define}}` is only at the top level, so only a
    /// `{{block}}` counts as a level of nesting.
    fn parse_subtree(&mut self, context: &str) -> Result<ListNode> {
        let nested = context == "block clause";
        if nested {
            self.enter()?;
        }
        let vars = std::mem::replace(&mut self.vars, vec!["$".to_owned()]);
        let range_depth = std::mem::take(&mut self.range_depth);
        let result = self.item_list();
        self.vars = vars;
        self.range_depth = range_depth;
        if nested {
            self.depth -= 1;
        }
        let (list, end) = result?;
        if !matches!(end, Parsed::End) {
            return self.error(format_args!("unexpected {} in {context}", end.describe()));
        }
        Ok(list)
    }

    /// Go's `parseDefinition`, after `{{define`.
    fn parse_definition(&mut self) -> Result<()> {
        const CONTEXT: &str = "define clause";
        let token = self.next_non_space();
        if !matches!(token.kind, ItemKind::String | ItemKind::RawString) {
            return self.unexpected(&token, CONTEXT);
        }
        let Some(name) = strconv::unquote(&token.val) else {
            return self.error("invalid syntax");
        };
        self.expect(ItemKind::RightDelim, CONTEXT)?;
        let list = self.parse_subtree(CONTEXT)?;
        self.add(&name, list)
    }

    /// Go's `parseTemplateName`.
    fn template_name(&self, token: &Item, context: &str) -> Result<String> {
        match token.kind {
            ItemKind::String | ItemKind::RawString => match strconv::unquote(&token.val) {
                Some(name) => Ok(name),
                None => self.error("invalid syntax"),
            },
            _ => self.unexpected(token, context),
        }
    }

    /// Go's `blockControl`: defines the template and calls it.
    fn block_control(&mut self) -> Result<Node> {
        const CONTEXT: &str = "block clause";
        let token = self.next_non_space();
        let name = self.template_name(&token, CONTEXT)?;
        let pipe = self.pipeline(CONTEXT, ItemKind::RightDelim)?;
        let list = self.parse_subtree(CONTEXT)?;
        self.add(&name, list)?;
        Ok(Node::Template {
            pos: token.pos,
            name,
            pipe: Some(pipe),
        })
    }

    /// Go's `templateControl`.
    fn template_control(&mut self) -> Result<Node> {
        const CONTEXT: &str = "template clause";
        let token = self.next_non_space();
        let name = self.template_name(&token, CONTEXT)?;
        let mut pipe = None;
        if self.next_non_space().kind != ItemKind::RightDelim {
            self.backup();
            pipe = Some(self.pipeline(CONTEXT, ItemKind::RightDelim)?);
        }
        Ok(Node::Template {
            pos: token.pos,
            name,
            pipe,
        })
    }

    fn else_control(&mut self) -> Result<Parsed> {
        let peek = self.peek_non_space();
        if matches!(peek.kind, ItemKind::If | ItemKind::With) {
            return Ok(Parsed::Else { pos: peek.pos });
        }
        let token = self.expect(ItemKind::RightDelim, "else")?;
        Ok(Parsed::Else { pos: token.pos })
    }

    fn pipeline(&mut self, context: &str, end: ItemKind) -> Result<Pipe> {
        let token = self.peek_non_space();
        let mut pipe = Pipe {
            pos: token.pos,
            is_assign: false,
            decl: Vec::new(),
            cmds: Vec::new(),
        };
        loop {
            let v = self.peek_non_space();
            if v.kind != ItemKind::Variable {
                break;
            }
            self.next();
            let token_after_variable = self.peek();
            let next = self.peek_non_space();
            if matches!(next.kind, ItemKind::Assign | ItemKind::Declare) {
                pipe.is_assign = next.kind == ItemKind::Assign;
                self.next_non_space();
                pipe.decl.push(v.val.clone());
                self.vars.push(v.val);
            } else if next.kind == ItemKind::Char && next.val == "," {
                self.next_non_space();
                pipe.decl.push(v.val.clone());
                self.vars.push(v.val);
                if context == "range" && pipe.decl.len() < 2 {
                    match self.peek_non_space().kind {
                        ItemKind::Variable | ItemKind::RightDelim | ItemKind::RightParen => {
                            continue;
                        }
                        _ => return self.error("range can only initialize variables"),
                    }
                }
                return self.error(format_args!("too many declarations in {context}"));
            } else if token_after_variable.kind == ItemKind::Space {
                self.backup3(v, token_after_variable);
            } else {
                self.backup2(v);
            }
            break;
        }
        loop {
            let token = self.next_non_space();
            match token.kind {
                kind if kind == end => {
                    self.check_pipeline(&pipe, context)?;
                    return Ok(pipe);
                }
                ItemKind::Bool
                | ItemKind::CharConstant
                | ItemKind::Complex
                | ItemKind::Dot
                | ItemKind::Field
                | ItemKind::Identifier
                | ItemKind::Number
                | ItemKind::Nil
                | ItemKind::RawString
                | ItemKind::String
                | ItemKind::Variable
                | ItemKind::LeftParen => {
                    self.backup();
                    let cmd = self.command()?;
                    pipe.cmds.push(cmd);
                }
                _ => return self.unexpected(&token, context),
            }
        }
    }

    fn check_pipeline(&self, pipe: &Pipe, context: &str) -> Result<()> {
        if pipe.cmds.is_empty() {
            return self.error(format_args!("missing value for {context}"));
        }
        for (index, cmd) in pipe.cmds.iter().enumerate().skip(1) {
            if let Some(
                Arg::Bool { .. }
                | Arg::Dot { .. }
                | Arg::Nil { .. }
                | Arg::Number { .. }
                | Arg::String { .. },
            ) = cmd.args.first()
            {
                return self.error(format_args!(
                    "non executable command in pipeline stage {}",
                    index + 1
                ));
            }
        }
        Ok(())
    }

    fn parse_control(&mut self, context: &str) -> Result<Branch> {
        self.enter()?;
        let mark = self.vars.len();
        let result = self.parse_control_inner(context);
        self.vars.truncate(mark);
        self.depth -= 1;
        result
    }

    fn parse_control_inner(&mut self, context: &str) -> Result<Branch> {
        let pipe = self.pipeline(context, ItemKind::RightDelim)?;
        if context == "range" {
            self.range_depth += 1;
        }
        let list = self.item_list();
        if context == "range" {
            self.range_depth -= 1;
        }
        let (list, next) = list?;
        let mut else_list = None;
        match next {
            Parsed::End | Parsed::Node(_) => {}
            Parsed::Else { pos } => {
                if context == "if" && self.peek().kind == ItemKind::If {
                    self.next();
                    let nested = self.parse_control("if")?;
                    else_list = Some(ListNode {
                        pos,
                        nodes: vec![Node::If(nested)],
                    });
                } else if context == "with" && self.peek().kind == ItemKind::With {
                    self.next();
                    let nested = self.parse_control("with")?;
                    else_list = Some(ListNode {
                        pos,
                        nodes: vec![Node::With(nested)],
                    });
                } else {
                    let (list, next) = self.item_list()?;
                    if !matches!(next, Parsed::End) {
                        return self.error(format_args!("expected end; found {}", next.describe()));
                    }
                    else_list = Some(list);
                }
            }
        }
        Ok(Branch {
            pos: pipe.pos,
            pipe,
            list,
            else_list,
        })
    }

    fn command(&mut self) -> Result<Command> {
        let mut cmd = Command {
            pos: self.peek_non_space().pos,
            args: Vec::new(),
        };
        loop {
            self.peek_non_space();
            if let Some(operand) = self.operand()? {
                cmd.args.push(operand);
            }
            let token = self.next();
            match token.kind {
                ItemKind::Space => continue,
                ItemKind::RightDelim | ItemKind::RightParen => self.backup(),
                ItemKind::Pipe => {}
                _ => return self.unexpected(&token, "operand"),
            }
            break;
        }
        if cmd.args.is_empty() {
            return self.error("empty command");
        }
        Ok(cmd)
    }

    fn operand(&mut self) -> Result<Option<Arg>> {
        let Some(node) = self.term()? else {
            return Ok(None);
        };
        if self.peek().kind != ItemKind::Field {
            return Ok(Some(node));
        }
        let pos = self.peek().pos;
        let mut fields = Vec::new();
        while self.peek().kind == ItemKind::Field {
            let field = self.next().val;
            fields.push(field.trim_start_matches('.').to_owned());
        }
        Ok(Some(match node {
            Arg::Field { mut ident, .. } => {
                ident.extend(fields);
                Arg::Field { pos, ident }
            }
            Arg::Variable { mut ident, .. } => {
                ident.extend(fields);
                Arg::Variable { pos, ident }
            }
            Arg::Bool { .. }
            | Arg::String { .. }
            | Arg::Number { .. }
            | Arg::Nil { .. }
            | Arg::Dot { .. } => {
                return self.error(format_args!(
                    "unexpected . after term {}",
                    crate::fmt::quote(&node.to_string())
                ));
            }
            node => Arg::Chain {
                pos,
                node: Box::new(node),
                fields,
            },
        }))
    }

    fn term(&mut self) -> Result<Option<Arg>> {
        let token = self.next_non_space();
        let pos = token.pos;
        let arg = match token.kind {
            ItemKind::Identifier => {
                if !(self.has_function)(&token.val) {
                    return self.error(format_args!(
                        "function {} not defined",
                        crate::fmt::quote(&token.val)
                    ));
                }
                Arg::Identifier {
                    pos,
                    name: token.val,
                }
            }
            ItemKind::Dot => Arg::Dot { pos },
            ItemKind::Nil => Arg::Nil { pos },
            ItemKind::Variable => {
                let ident: Vec<String> = token.val.split('.').map(str::to_owned).collect();
                let name = ident.first().cloned().unwrap_or_default();
                if !self.vars.contains(&name) {
                    return self.error(format_args!(
                        "undefined variable {}",
                        crate::fmt::quote(&name)
                    ));
                }
                Arg::Variable { pos, ident }
            }
            ItemKind::Field => Arg::Field {
                pos,
                ident: token.val.split('.').skip(1).map(str::to_owned).collect(),
            },
            ItemKind::Bool => Arg::Bool {
                pos,
                value: token.val == "true",
            },
            ItemKind::CharConstant | ItemKind::Complex | ItemKind::Number => {
                let number = self.number(&token)?;
                Arg::Number { pos, number }
            }
            ItemKind::LeftParen => {
                self.enter()?;
                let pipe = self.pipeline("parenthesized pipeline", ItemKind::RightParen);
                self.depth -= 1;
                Arg::Pipe(Box::new(pipe?))
            }
            ItemKind::String | ItemKind::RawString => {
                let Some(text) = strconv::unquote(&token.val) else {
                    return self.error("invalid syntax");
                };
                Arg::String {
                    pos,
                    quoted: token.val,
                    text,
                }
            }
            _ => {
                self.backup();
                return Ok(None);
            }
        };
        Ok(Some(arg))
    }

    /// Go's `newNumber`.
    fn number(&self, token: &Item) -> Result<Number> {
        let text = token.val.clone();
        if token.kind == ItemKind::CharConstant {
            let Some((c, tail)) = text
                .get(1..)
                .and_then(|rest| strconv::unquote_char(rest, '\''))
            else {
                return self.error("invalid syntax");
            };
            if tail != "'" {
                return self.error(format_args!("malformed character constant: {text}"));
            }
            let code = i64::from(u32::from(c));
            return Ok(Number {
                text,
                int: Some(code),
                is_uint: true,
                float: Some(f64::from(u32::from(c))),
            });
        }
        if token.kind == ItemKind::Complex || text.ends_with('i') {
            return self.error(format_args!(
                "complex number {} is not supported",
                crate::fmt::quote(&text)
            ));
        }
        let uint = strconv::parse_uint(&text).ok();
        let mut int = strconv::parse_int(&text).ok();
        let mut is_uint = uint.is_some() || int == Some(0);
        let mut float = None;
        if let Some(n) = int {
            #[allow(clippy::cast_precision_loss)]
            {
                float = Some(n as f64);
            }
        } else if let Some(u) = uint {
            #[allow(clippy::cast_precision_loss)]
            {
                float = Some(u as f64);
            }
        } else if let Some(f) = strconv::parse_float(&text) {
            if !text.contains(['.', 'e', 'E', 'p', 'P']) {
                return self.error(format_args!(
                    "integer overflow: {}",
                    crate::fmt::quote(&text)
                ));
            }
            float = Some(f);
            int = float_to_int(f);
            if f >= 0.0 && int.is_some() {
                is_uint = true;
            }
        }
        if int.is_none() && !is_uint && float.is_none() {
            return self.error(format_args!(
                "illegal number syntax: {}",
                crate::fmt::quote(&text)
            ));
        }
        Ok(Number {
            text,
            int,
            is_uint,
            float,
        })
    }
}

/// The integer equal to `f`, if there is one in range: Go's `float64(int64(f)) == f`.
fn float_to_int(f: f64) -> Option<i64> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    let n = f as i64;
    #[allow(clippy::cast_precision_loss, clippy::float_cmp)]
    (n as f64 == f).then_some(n)
}

/// Go's `IsEmptyTree`: whether a template has nothing but space.
fn is_empty_tree(list: &ListNode) -> bool {
    list.nodes.iter().all(|node| match node {
        Node::Text { text, .. } => text.trim().is_empty(),
        _ => false,
    })
}
