use std::fmt;
use std::ops::Range;

use crate::token::{Token, to_usize};

/// A node of an expression's syntax tree.
///
/// Parentheses create no node: `(a)` is the node of `a`, and its range excludes the
/// parentheses. The range of a node that contains `(a)` includes them.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    /// What the node is.
    pub kind: ExprKind,
    /// The byte offset of the node's first token.
    pub start: u32,
    /// The byte offset one past the node's last token.
    pub end: u32,
    /// The token that actionlint reports errors about this node at. It is the node's first
    /// token, except that a parenthesized operand starts after its `(`: the token of `(a).b` and
    /// of `(a) == b` is `a`.
    pub token: Token,
}

impl Expr {
    /// Returns the node's byte range in the parser's input.
    #[must_use]
    pub fn range(&self) -> Range<usize> {
        to_usize(self.start)..to_usize(self.end)
    }

    /// Calls `f` for each node of the tree, before and after the node's children, as
    /// actionlint's `VisitExprNode` does.
    ///
    /// `f` gets the node, its parent (`None` for `self`), and whether the walk enters or leaves
    /// the node. Children are visited in source order, except that the index of an index access
    /// is visited before its operand. The untrusted-input check depends on that order.
    ///
    /// ```
    /// use workflint_expr::{Expr, ExprKind, Visit, parse_expression};
    ///
    /// let (expr, _) = parse_expression("a[b] }}").unwrap();
    /// let mut names = Vec::new();
    /// expr.walk(&mut |node: &Expr, _: Option<&Expr>, visit: Visit| {
    ///     if let (ExprKind::Variable(name), Visit::Enter) = (&node.kind, visit) {
    ///         names.push(name.as_str().to_owned());
    ///     }
    /// });
    /// assert_eq!(names, ["b", "a"]);
    /// ```
    pub fn walk<'a, F>(&'a self, f: &mut F)
    where
        F: FnMut(&'a Expr, Option<&'a Expr>, Visit),
    {
        self.walk_in(None, f);
    }

    fn walk_in<'a, F>(&'a self, parent: Option<&'a Expr>, f: &mut F)
    where
        F: FnMut(&'a Expr, Option<&'a Expr>, Visit),
    {
        f(self, parent, Visit::Enter);
        let me = Some(self);
        match &self.kind {
            ExprKind::Variable(_)
            | ExprKind::Null
            | ExprKind::Bool(_)
            | ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::String(_) => {}
            ExprKind::Property { receiver, .. } | ExprKind::ObjectFilter { receiver } => {
                receiver.walk_in(me, f);
            }
            ExprKind::Index { operand, index } => {
                index.walk_in(me, f);
                operand.walk_in(me, f);
            }
            ExprKind::Not { operand } => operand.walk_in(me, f),
            ExprKind::Compare { left, right, .. } | ExprKind::Logical { left, right, .. } => {
                left.walk_in(me, f);
                right.walk_in(me, f);
            }
            ExprKind::Call { args, .. } => {
                for arg in args {
                    arg.walk_in(me, f);
                }
            }
        }
        f(self, parent, Visit::Leave);
    }
}

/// Whether [`Expr::walk`] calls its function before or after a node's children.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Visit {
    /// Before the children.
    Enter,
    /// After the children.
    Leave,
}

/// The kinds of [`Expr`] nodes.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// A variable, such as `github`. Variables are contexts or, inside some functions, names
    /// that the function defines.
    Variable(Name),
    /// `null`. The keywords `null`, `true`, and `false` are case-sensitive: `NULL` is a
    /// variable.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// An integer literal, decimal or `0x` hexadecimal. GitHub evaluates numbers as doubles, so
    /// the value is the nearest double.
    Int(f64),
    /// A number literal with a fraction or an exponent.
    Float(f64),
    /// A string literal.
    String(StringLit),
    /// Property access with a dot, such as `github.event`.
    Property {
        /// The expression before the dot.
        receiver: Box<Expr>,
        /// The property after the dot.
        name: Name,
    },
    /// An object filter, `.*`, as in `labels.*.name`.
    ObjectFilter {
        /// The expression before `.*`.
        receiver: Box<Expr>,
    },
    /// Index access, such as `matrix['os']` or `list[0]`.
    Index {
        /// The expression before `[`.
        operand: Box<Expr>,
        /// The expression in the brackets.
        index: Box<Expr>,
    },
    /// The `!` operator.
    Not {
        /// The expression after `!`.
        operand: Box<Expr>,
    },
    /// A comparison: `<`, `<=`, `>`, `>=`, `==`, or `!=`.
    Compare {
        /// The operator.
        op: CompareOp,
        /// The left operand.
        left: Box<Expr>,
        /// The right operand.
        right: Box<Expr>,
    },
    /// `&&` or `||`. A chain such as `a && b && c` nests to the right, as actionlint's parser
    /// builds it: `a && (b && c)`.
    Logical {
        /// The operator.
        op: LogicalOp,
        /// The left operand.
        left: Box<Expr>,
        /// The right operand.
        right: Box<Expr>,
    },
    /// A function call, such as `contains(a, 'b')`. Only a bare name can be called.
    Call {
        /// The function name.
        callee: Name,
        /// The arguments in source order.
        args: Vec<Expr>,
    },
}

/// A comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompareOp {
    /// `<`
    Less,
    /// `<=`
    LessEq,
    /// `>`
    Greater,
    /// `>=`
    GreaterEq,
    /// `==`
    Eq,
    /// `!=`
    NotEq,
}

impl CompareOp {
    /// Returns the operator as written, such as `<=`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Less => "<",
            Self::LessEq => "<=",
            Self::Greater => ">",
            Self::GreaterEq => ">=",
            Self::Eq => "==",
            Self::NotEq => "!=",
        }
    }

    /// Returns whether the operator is `==` or `!=`.
    #[must_use]
    pub const fn is_equality(self) -> bool {
        matches!(self, Self::Eq | Self::NotEq)
    }
}

impl fmt::Display for CompareOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A logical operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogicalOp {
    /// `&&`
    And,
    /// `||`
    Or,
}

impl LogicalOp {
    /// Returns the operator as written, `&&` or `||`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::And => "&&",
            Self::Or => "||",
        }
    }
}

impl fmt::Display for LogicalOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The name of a variable, a property, or a called function.
///
/// Names compare case-insensitively, so [`Name::as_str`] returns the name in lowercase. Names
/// are ASCII, because the lexer accepts only ASCII letters, digits, `_`, and `-` in them.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Name {
    folded: Box<str>,
    /// The name as written, if it differs from `folded`.
    original: Option<Box<str>>,
    start: u32,
    end: u32,
}

impl Name {
    pub(crate) fn new(text: &str, start: u32, end: u32) -> Self {
        let folded = text.to_ascii_lowercase();
        let original = (folded != text).then(|| text.into());
        Self {
            folded: folded.into_boxed_str(),
            original,
            start,
            end,
        }
    }

    /// Returns the name in lowercase.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.folded
    }

    /// Returns the name as written.
    #[must_use]
    pub fn original(&self) -> &str {
        self.original.as_deref().unwrap_or(&self.folded)
    }

    /// Returns the name's byte range in the parser's input.
    #[must_use]
    pub fn range(&self) -> Range<usize> {
        to_usize(self.start)..to_usize(self.end)
    }
}

/// The value of a string literal, with the quotes removed and `''` unescaped.
///
/// It also has a folded form, lowercased as Go's `strings.ToLower` does, so that a literal
/// index key such as `steps['MyStep']` can be compared case-insensitively.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StringLit {
    value: Box<str>,
    /// The folded value, if it differs from `value`.
    folded: Option<Box<str>>,
}

impl StringLit {
    pub(crate) fn new(value: String) -> Self {
        let folded = simple_lowercase(&value);
        let folded = (folded != value).then(|| folded.into_boxed_str());
        Self {
            value: value.into_boxed_str(),
            folded,
        }
    }

    /// Returns the value as written, without quotes and with `''` unescaped.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Returns the value lowercased as Go's `strings.ToLower` does.
    #[must_use]
    pub fn folded(&self) -> &str {
        self.folded.as_deref().unwrap_or(&self.value)
    }
}

/// Lowercases each character with its Unicode simple case mapping, as Go's `strings.ToLower`
/// does. It differs from Rust's lowercase only for U+0130, which becomes a plain `i`.
/// `workflint_yaml::simple_lowercase` does the same for YAML keys.
fn simple_lowercase(text: &str) -> String {
    text.chars()
        .map(|ch| {
            let mut lower = ch.to_lowercase();
            match (lower.next(), lower.next()) {
                (Some(lower), None) => lower,
                _ if ch == '\u{130}' => 'i',
                _ => ch,
            }
        })
        .collect()
}
