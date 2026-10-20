//! Works out the strings that a `runs-on` label with `${{ }}` can be.
//!
//! The evaluator knows string, number, boolean, and null literals, `matrix.<prop>`, `format()`,
//! `case()`, `&&`, `||`, `!`, `==`, and `!=`. Any other expression has an unknown value. An
//! unknown value may still decide a condition: `cond && 'a' || 'b'` is `a` or `b` whatever `cond`
//! is, and so is `case(cond, 'a', 'b')`. A label is unknown when one of its values is unknown,
//! and the rule skips it.

use workflint_expr::{CompareOp, ExprKind, LogicalOp};
use workflint_syntax::{
    Matrix, Placeholder, RawValue, contains_expression, is_expression_assigned,
};
use workflint_yaml::Tag;

use super::combinations::{combinations, get};

/// The most values a label can have, and the most combinations a matrix can have, for the
/// evaluator to work them out. GitHub runs at most 256 jobs from one matrix.
pub(super) const MAX_VALUES: usize = 256;

/// A value of an expression, or what the evaluator knows of it.
#[derive(Debug, Clone, PartialEq)]
enum Val {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    /// A value the evaluator does not know, and whether it is truthy, if that is known.
    Unknown(Option<bool>),
}

impl Val {
    /// Returns whether GitHub treats the value as true, or `None` if that is not known.
    fn truthy(&self) -> Option<bool> {
        match self {
            Self::Null => Some(false),
            Self::Bool(b) => Some(*b),
            Self::Number(n) => Some(*n != 0.0 && !n.is_nan()),
            Self::String(s) => Some(!s.is_empty()),
            Self::Unknown(truthy) => *truthy,
        }
    }

    /// Returns the value as GitHub prints it in a string, or `None` if that is not known.
    fn text(&self) -> Option<String> {
        match self {
            Self::Null => Some(String::new()),
            Self::Bool(b) => Some(b.to_string()),
            Self::Number(n) => number_text(*n),
            Self::String(s) => Some(s.clone()),
            Self::Unknown(_) => None,
        }
    }

    /// Returns the value as GitHub converts it to a number to compare it with a value of
    /// another type.
    fn number(&self) -> f64 {
        match self {
            Self::Null => 0.0,
            Self::Bool(b) => f64::from(u8::from(*b)),
            Self::Number(n) => *n,
            Self::String(s) => string_number(s),
            Self::Unknown(_) => f64::NAN,
        }
    }
}

/// Returns the strings that a label can be, in the order they were found and without repeats,
/// or `None` if any of them cannot be worked out.
///
/// `value` is the label's text and `placeholders` its `${{ }}`. The values of `matrix.<prop>`
/// come from each combination of `matrix`.
pub(super) fn label_values(
    value: &str,
    placeholders: &[Placeholder],
    matrix: Option<&Matrix<'_>>,
) -> Option<Vec<String>> {
    let mut exprs = Vec::with_capacity(placeholders.len());
    for placeholder in placeholders {
        exprs.push(placeholder.expr.as_ref().ok()?);
    }
    let mut props = Vec::new();
    for expr in &exprs {
        matrix_props(expr, &mut props);
    }
    let mut bindings: Vec<Vec<Val>> = Vec::new();
    if props.is_empty() {
        bindings.push(Vec::new());
    } else {
        for combination in combinations(matrix?)? {
            let binding: Vec<Val> = props
                .iter()
                .map(|prop| get(&combination, prop).map_or(Val::Null, raw_value))
                .collect();
            if !bindings.contains(&binding) {
                bindings.push(binding);
            }
        }
    }

    let whole = is_expression_assigned(value) && placeholders.len() == 1;
    let mut labels = Vec::new();
    for binding in &bindings {
        let env = Env {
            props: &props,
            binding,
        };
        let texts = if whole {
            let mut texts = Vec::new();
            for val in env.eval(exprs.first()?)? {
                let Val::String(text) = val else {
                    return None;
                };
                texts.push(text);
            }
            texts
        } else {
            env.template(value, placeholders, &exprs)?
        };
        for text in texts {
            if !labels.contains(&text) {
                labels.push(text);
            }
        }
        if labels.len() > MAX_VALUES {
            return None;
        }
    }
    Some(labels)
}

/// Adds the folded name of each `matrix.<prop>` in `expr` to `props`, once.
fn matrix_props(expr: &workflint_expr::Expr, props: &mut Vec<String>) {
    match &expr.kind {
        ExprKind::Property { receiver, name } => {
            if let ExprKind::Variable(variable) = &receiver.kind
                && variable.as_str() == "matrix"
            {
                if !props.iter().any(|prop| prop == name.as_str()) {
                    props.push(name.as_str().to_owned());
                }
            } else {
                matrix_props(receiver, props);
            }
        }
        ExprKind::ObjectFilter { receiver } => matrix_props(receiver, props),
        ExprKind::Index { operand, index } => {
            matrix_props(operand, props);
            matrix_props(index, props);
        }
        ExprKind::Not { operand } => matrix_props(operand, props),
        ExprKind::Compare { left, right, .. } => {
            matrix_props(left, props);
            matrix_props(right, props);
        }
        ExprKind::Logical { operands, .. } => {
            for operand in operands {
                matrix_props(operand, props);
            }
        }
        ExprKind::Call { args, .. } => {
            for arg in args {
                matrix_props(arg, props);
            }
        }
        ExprKind::Variable(_)
        | ExprKind::Null
        | ExprKind::Bool(_)
        | ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::String(_) => {}
    }
}

/// Returns a matrix value as the evaluator knows it. A scalar has the type of its tag, so a
/// quoted scalar is a string (`docs/differences.md#quoted-scalar-types`). A scalar with an
/// expression, a mapping, and a sequence are unknown.
fn raw_value(value: &RawValue<'_>) -> Val {
    let RawValue::Scalar(scalar) = value else {
        return Val::Unknown(None);
    };
    let text = scalar.value();
    if contains_expression(text) {
        return Val::Unknown(None);
    }
    match scalar.source.tag() {
        Tag::Null => Val::Null,
        Tag::Bool => Val::Bool(text.eq_ignore_ascii_case("true")),
        Tag::Int => int_value(text).map_or(Val::Unknown(None), Val::Number),
        Tag::Float => text
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .map_or(Val::Unknown(None), Val::Number),
        _ => Val::String(text.to_owned()),
    }
}

/// Parses a plain integer scalar of the YAML 1.2 core schema: decimal, `0x` hexadecimal, or `0o`
/// octal.
fn int_value(text: &str) -> Option<f64> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let magnitude = if let Some(hex) = digits.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).ok()
    } else if let Some(octal) = digits.strip_prefix("0o") {
        u64::from_str_radix(octal, 8).ok()
    } else {
        digits.parse::<u64>().ok()
    }?;
    #[allow(clippy::cast_precision_loss)]
    let magnitude = magnitude as f64;
    Some(if negative { -magnitude } else { magnitude })
}

/// Prints a number as GitHub does, with .NET's `G15` format, or returns `None` when that text
/// differs from the shortest decimal form Rust prints: for 16 or more significant digits, an
/// exponent, `-0`, and values that are not finite.
fn number_text(n: f64) -> Option<String> {
    if !n.is_finite() || (n == 0.0 && n.is_sign_negative()) {
        return None;
    }
    if n != 0.0 && !(1e-5..1e15).contains(&n.abs()) {
        return None;
    }
    let text = n.to_string();
    let digits = text
        .chars()
        .filter(char::is_ascii_digit)
        .skip_while(|&c| c == '0')
        .count();
    (digits <= 15).then_some(text)
}

/// Converts a string to a number as GitHub does to compare it with a number: white space is
/// trimmed, an empty string is 0, `0x` starts a hexadecimal integer, and anything that is not a
/// number is `NaN`.
fn string_number(s: &str) -> f64 {
    let s = s.trim();
    if s.is_empty() {
        return 0.0;
    }
    if let Some(hex) = s.strip_prefix("0x") {
        #[allow(clippy::cast_precision_loss)]
        return u64::from_str_radix(hex, 16).map_or(f64::NAN, |n| n as f64);
    }
    // Rust also reads `inf` and `NaN`, which GitHub does not.
    if s.chars()
        .any(|c| c.is_ascii_alphabetic() && c != 'e' && c != 'E')
    {
        return f64::NAN;
    }
    s.parse::<f64>().unwrap_or(f64::NAN)
}

/// GitHub's `==`: values of one type compare directly, strings without case, and values of two
/// types compare as numbers. `None` means the result is not known.
fn equal(a: &Val, b: &Val) -> Option<bool> {
    Some(match (a, b) {
        (Val::Unknown(_), _) | (_, Val::Unknown(_)) => return None,
        (Val::Null, Val::Null) => true,
        (Val::Bool(a), Val::Bool(b)) => a == b,
        #[allow(clippy::float_cmp)]
        (Val::Number(a), Val::Number(b)) => a == b,
        (Val::String(a), Val::String(b)) => a.to_uppercase() == b.to_uppercase(),
        #[allow(clippy::float_cmp)]
        _ => a.number() == b.number(),
    })
}

/// Adds `val` to `vals` unless it is there. Returns `None` if `vals` would hold more than
/// [`MAX_VALUES`] values.
fn push(vals: &mut Vec<Val>, val: Val) -> Option<()> {
    if !vals.contains(&val) {
        if vals.len() == MAX_VALUES {
            return None;
        }
        vals.push(val);
    }
    Some(())
}

/// The values of the matrix properties for one combination.
struct Env<'e> {
    /// The folded names of the `matrix.<prop>` that the label uses.
    props: &'e [String],
    /// The value of each of `props`, in the same order.
    binding: &'e [Val],
}

impl Env<'_> {
    /// Returns the values that `expr` can have, or `None` if there are more than
    /// [`MAX_VALUES`].
    fn eval(&self, expr: &workflint_expr::Expr) -> Option<Vec<Val>> {
        Some(match &expr.kind {
            ExprKind::Null => vec![Val::Null],
            ExprKind::Bool(b) => vec![Val::Bool(*b)],
            ExprKind::Int(n) | ExprKind::Float(n) => vec![Val::Number(*n)],
            ExprKind::String(s) => vec![Val::String(s.value().to_owned())],
            ExprKind::Property { receiver, name } => {
                let val = match &receiver.kind {
                    ExprKind::Variable(variable) if variable.as_str() == "matrix" => self
                        .props
                        .iter()
                        .position(|prop| prop == name.as_str())
                        .and_then(|i| self.binding.get(i))
                        .cloned()
                        .unwrap_or(Val::Unknown(None)),
                    _ => Val::Unknown(None),
                };
                vec![val]
            }
            ExprKind::Not { operand } => {
                let mut vals = Vec::new();
                for val in self.eval(operand)? {
                    let not = match val.truthy() {
                        Some(truthy) => Val::Bool(!truthy),
                        None => Val::Unknown(None),
                    };
                    push(&mut vals, not)?;
                }
                vals
            }
            ExprKind::Compare { op, left, right } => {
                let lefts = self.eval(left)?;
                let rights = self.eval(right)?;
                let mut vals = Vec::new();
                for left in &lefts {
                    for right in &rights {
                        let result = match op {
                            CompareOp::Eq => equal(left, right),
                            CompareOp::NotEq => equal(left, right).map(|eq| !eq),
                            _ => None,
                        };
                        push(&mut vals, result.map_or(Val::Unknown(None), Val::Bool))?;
                    }
                }
                vals
            }
            ExprKind::Logical { op, operands } => self.logical(*op, operands)?,
            ExprKind::Call { callee, args } if callee.as_str() == "format" => self.format(args)?,
            ExprKind::Call { callee, args } if callee.as_str() == "case" => self.case(args)?,
            ExprKind::Variable(_)
            | ExprKind::ObjectFilter { .. }
            | ExprKind::Index { .. }
            | ExprKind::Call { .. } => vec![Val::Unknown(None)],
        })
    }

    /// Evaluates a chain of `&&` or `||`. `&&` gives the first falsy operand or the last one,
    /// and `||` the first truthy operand or the last one. An operand whose truthiness is not
    /// known takes both ways.
    fn logical(&self, op: LogicalOp, operands: &[workflint_expr::Expr]) -> Option<Vec<Val>> {
        let stop_on = matches!(op, LogicalOp::Or);
        let mut done = Vec::new();
        for (i, operand) in operands.iter().enumerate() {
            let vals = self.eval(operand)?;
            if i + 1 == operands.len() {
                for val in vals {
                    push(&mut done, val)?;
                }
                break;
            }
            let mut go_on = false;
            for val in vals {
                match val.truthy() {
                    Some(truthy) if truthy == stop_on => push(&mut done, val)?,
                    Some(_) => go_on = true,
                    None => {
                        push(&mut done, Val::Unknown(Some(stop_on)))?;
                        go_on = true;
                    }
                }
            }
            if !go_on {
                break;
            }
        }
        Some(done)
    }

    /// Evaluates `case()`. A true predicate gives its value and ends the call, a false one goes
    /// on to the next pair, and one whose value is not known does both. The default is the
    /// value when no predicate is true. GitHub rejects a call without a default and a predicate
    /// that is not a boolean, so either gives an unknown value.
    fn case(&self, args: &[workflint_expr::Expr]) -> Option<Vec<Val>> {
        let unknown = Some(vec![Val::Unknown(None)]);
        if args.len() < 3 || args.len().is_multiple_of(2) {
            return unknown;
        }
        let mut done = Vec::new();
        let (pairs, default) = args.as_chunks::<2>();
        for [predicate, value] in pairs {
            let (mut take, mut go_on) = (false, false);
            for val in self.eval(predicate)? {
                match val {
                    Val::Bool(true) => take = true,
                    Val::Bool(false) => go_on = true,
                    Val::Unknown(_) => (take, go_on) = (true, true),
                    Val::Null | Val::Number(_) | Val::String(_) => return unknown,
                }
            }
            if take {
                for val in self.eval(value)? {
                    push(&mut done, val)?;
                }
            }
            if !go_on {
                return Some(done);
            }
        }
        for default in default {
            for val in self.eval(default)? {
                push(&mut done, val)?;
            }
        }
        Some(done)
    }

    /// Evaluates `format()` for each choice of argument values.
    fn format(&self, args: &[workflint_expr::Expr]) -> Option<Vec<Val>> {
        let mut choices: Vec<Vec<Val>> = vec![Vec::new()];
        for arg in args {
            let vals = self.eval(arg)?;
            if choices.len().checked_mul(vals.len())? > MAX_VALUES {
                return None;
            }
            choices = choices
                .iter()
                .flat_map(|choice| {
                    vals.iter().map(|val| {
                        let mut choice = choice.clone();
                        choice.push(val.clone());
                        choice
                    })
                })
                .collect();
        }
        let mut vals = Vec::new();
        for choice in choices {
            let val = match choice.split_first() {
                Some((Val::String(format), args)) => {
                    format_string(format, args).map_or(Val::Unknown(None), Val::String)
                }
                _ => Val::Unknown(None),
            };
            push(&mut vals, val)?;
        }
        Some(vals)
    }

    /// Returns each string that the template `value` can be, with each placeholder replaced by
    /// one of its values printed as text, or `None` if a value cannot be printed or there are
    /// more than [`MAX_VALUES`] strings.
    fn template(
        &self,
        value: &str,
        placeholders: &[Placeholder],
        exprs: &[&workflint_expr::Expr],
    ) -> Option<Vec<String>> {
        let mut texts = vec![String::new()];
        let mut at = 0;
        for (placeholder, expr) in placeholders.iter().zip(exprs) {
            let range = placeholder.range();
            let before = value.get(at..range.start)?;
            let mut parts = Vec::new();
            for val in self.eval(expr)? {
                parts.push(val.text()?);
            }
            if texts.len().checked_mul(parts.len())? > MAX_VALUES {
                return None;
            }
            texts = texts
                .iter()
                .flat_map(|text| {
                    parts
                        .iter()
                        .map(move |part| format!("{text}{before}{part}"))
                })
                .collect();
            at = range.end;
        }
        let after = value.get(at..)?;
        for text in &mut texts {
            text.push_str(after);
        }
        Some(texts)
    }
}

/// GitHub's `format()`: `{N}` is the text of argument `N`, and `{{` and `}}` are braces. Returns
/// `None` for a format string that GitHub rejects or an argument that cannot be printed.
fn format_string(format: &str, args: &[Val]) -> Option<String> {
    let mut out = String::with_capacity(format.len());
    let mut chars = format.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '{' if chars.peek().is_some_and(|&(_, next)| next == '{') => {
                chars.next();
                out.push('{');
            }
            '{' => {
                let rest = format.get(i + 1..)?;
                let close = rest.find('}')?;
                let digits = rest.get(..close)?;
                if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                    return None;
                }
                let index: usize = digits.parse().ok()?;
                out.push_str(&args.get(index)?.text()?);
                for _ in 0..=close {
                    chars.next();
                }
            }
            '}' if chars.peek().is_some_and(|&(_, next)| next == '}') => {
                chars.next();
                out.push('}');
            }
            '}' => return None,
            _ => out.push(c),
        }
    }
    Some(out)
}
