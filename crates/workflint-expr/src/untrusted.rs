//! The check for untrusted input in expressions of scripts.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `UntrustedInputChecker` in
//! `expr_insecure.go`.

use workflint_data::UntrustedInput;
use workflint_diagnostics::RuleMeta;
use workflint_diagnostics::quote::{quote, sorted_quotes};

use crate::ast::{Expr, ExprKind, Visit};
use crate::error::ExprError;
use crate::rules::{UNTRUSTED_INPUT, UNTRUSTED_OBJECT_FILTER};

const ADVICE_LINK: &str = "see https://docs.github.com/en/actions/reference/security/secure-use#good-practices-for-mitigating-script-injection-attacks for more details";

/// Returns the errors about the untrusted values that `expr` reads, with the paths in `roots`,
/// such as [`workflint_data::UNTRUSTED_INPUTS`].
///
/// This visits every node of the tree, as the reference's `VisitExprNode` does. Inside
/// [`TypeEnv::check`](crate::TypeEnv::check), the check skips the arguments of a function that
/// is not built in, because the type checker does not visit them.
///
/// The check follows each chain of property accesses, indices, and object filters from a
/// context in `roots`. When the chain ends on an untrusted value, it reports
/// [`UNTRUSTED_INPUT`]. When an object filter makes it end on several, it reports
/// [`UNTRUSTED_OBJECT_FILTER`] with all of them. An index other than a string literal counts
/// as any array element, `*`. Nothing inside `contains`, `startsWith`, or `endsWith` is
/// reported, because they return a boolean. Each error starts at the context and ends at the
/// end of the chain.
///
/// ```
/// use workflint_data::UNTRUSTED_INPUTS;
/// use workflint_expr::{find_untrusted_inputs, parse_expression};
///
/// let (expr, _) = parse_expression("github.event.commits[0].message }}").unwrap();
/// let errors = find_untrusted_inputs(&expr, UNTRUSTED_INPUTS);
/// assert!(errors[0].message.starts_with(r#""github.event.commits.*.message" is potentially untrusted."#));
///
/// let (expr, _) = parse_expression("contains(github.event.issue.title, 'x') }}").unwrap();
/// assert!(find_untrusted_inputs(&expr, UNTRUSTED_INPUTS).is_empty());
/// ```
#[must_use]
pub fn find_untrusted_inputs(expr: &Expr, roots: &'static [UntrustedInput]) -> Vec<ExprError> {
    let mut checker = UntrustedChecker::new(roots);
    expr.walk(
        &mut |node: &Expr, _: Option<&Expr>, visit: Visit| match visit {
            Visit::Enter => checker.enter(node),
            Visit::Leave => checker.leave(node),
        },
    );
    checker.finish()
}

/// The state of the check while a walk visits the nodes of one expression. The reference's
/// `UntrustedInputChecker`.
///
/// The walk must visit the index of an index access before its operand. Then a chain inside an
/// index, as in `foo[github.head_ref].bar`, ends before the chain around it resumes.
#[derive(Debug)]
pub(crate) struct UntrustedChecker {
    roots: &'static [UntrustedInput],
    /// The nodes of the tree that the current chain has reached. An object filter can reach
    /// several.
    cursors: Vec<Cursor>,
    /// The byte offset of the context that starts the current chain.
    start: u32,
    /// The byte offset one past the last node of the current chain.
    end: u32,
    /// Whether the chain had an object filter that an index does not undo yet. An index after a
    /// filter selects an element of the filtered array, so it keeps the cursors where they are:
    /// `github.event.*.body[0]` reads the same properties as `github.event.*.body`.
    filtering_object: bool,
    /// How many calls of `contains`, `startsWith`, or `endsWith` the walk is inside.
    safe_calls: usize,
    errors: Vec<ExprError>,
}

impl UntrustedChecker {
    pub(crate) fn new(roots: &'static [UntrustedInput]) -> Self {
        Self {
            roots,
            cursors: Vec::new(),
            start: 0,
            end: 0,
            filtering_object: false,
            safe_calls: 0,
            errors: Vec::new(),
        }
    }

    /// Called before the children of `node`.
    pub(crate) fn enter(&mut self, node: &Expr) {
        if is_safe_call(node) {
            self.safe_calls += 1;
        }
    }

    /// Called after the children of `node`.
    pub(crate) fn leave(&mut self, node: &Expr) {
        if self.safe_calls > 0 {
            if is_safe_call(node) {
                self.safe_calls -= 1;
            }
            return;
        }
        match &node.kind {
            ExprKind::Variable(name) => {
                self.end_chain();
                if let Some(root) = self.roots.iter().find(|root| root.name == name.as_str()) {
                    self.start = node.token.start;
                    self.cursors.push(Cursor {
                        node: root,
                        path: root.name.to_owned(),
                    });
                }
            }
            ExprKind::Property { name, .. } => self.property(name.as_str()),
            // `github['event']` reads the property `event`, and the key matches without regard
            // to case (`docs/differences.md#literal-index-case`).
            ExprKind::Index { index, .. } => match &index.kind {
                ExprKind::String(lit) => self.property(lit.folded()),
                _ => self.index(),
            },
            ExprKind::ObjectFilter { .. } => self.object_filter(),
            _ => {
                self.end_chain();
                return;
            }
        }
        self.end = node.end;
    }

    /// Ends the last chain and returns the errors.
    pub(crate) fn finish(mut self) -> Vec<ExprError> {
        self.end_chain();
        self.errors
    }

    /// Moves each cursor to its child `name` and drops the cursors without one.
    fn property(&mut self, name: &str) {
        self.cursors
            .retain_mut(|cursor| match cursor.node.child(name) {
                Some(child) => {
                    cursor.descend(child);
                    true
                }
                None => false,
            });
    }

    /// Moves each cursor to its array elements, `*`, and drops the cursors that are not arrays.
    fn index(&mut self) {
        if self.filtering_object {
            self.filtering_object = false;
            return;
        }
        self.property("*");
    }

    /// Moves each cursor on an array to its elements, and each other cursor to all its
    /// children. Drops the cursors on untrusted values, which have no children.
    fn object_filter(&mut self) {
        self.filtering_object = true;
        let mut next = Vec::with_capacity(self.cursors.len());
        for mut cursor in self.cursors.drain(..) {
            match cursor.node.child("*") {
                Some(elements) => {
                    cursor.descend(elements);
                    next.push(cursor);
                }
                None => next.extend(cursor.node.children.iter().map(|child| {
                    let mut cursor = cursor.clone();
                    cursor.descend(child);
                    cursor
                })),
            }
        }
        self.cursors = next;
    }

    /// Reports the untrusted values that the chain ends on and resets the state for the next
    /// chain. The reference's `end`.
    fn end_chain(&mut self) {
        let paths: Vec<String> = self
            .cursors
            .drain(..)
            .filter(|cursor| cursor.node.is_leaf())
            .map(|cursor| cursor.path)
            .collect();
        match paths.as_slice() {
            [] => {}
            [path] => {
                let message = format!(
                    "{} is potentially untrusted. avoid using it directly in inline scripts. instead, pass it through an environment variable. {ADVICE_LINK}",
                    quote(path)
                );
                self.report(&UNTRUSTED_INPUT, message);
            }
            _ => {
                let message = format!(
                    "object filter extracts potentially untrusted properties {}. avoid using the value directly in inline scripts. instead, pass the value through an environment variable. {ADVICE_LINK}",
                    sorted_quotes(&paths)
                );
                self.report(&UNTRUSTED_OBJECT_FILTER, message);
            }
        }
        self.cursors.clear();
        self.filtering_object = false;
    }

    fn report(&mut self, rule: &'static RuleMeta, message: String) {
        self.errors.push(ExprError {
            message,
            start: self.start,
            end: self.end,
            rule,
        });
    }
}

/// Returns whether `node` calls `contains`, `startsWith`, or `endsWith`, whose boolean results
/// cannot inject code.
fn is_safe_call(node: &Expr) -> bool {
    matches!(
        &node.kind,
        ExprKind::Call { callee, .. }
            if matches!(callee.as_str(), "contains" | "startswith" | "endswith")
    )
}

/// A node of the tree that a chain has reached, with the path to it.
#[derive(Debug, Clone)]
struct Cursor {
    node: &'static UntrustedInput,
    /// The path from the root to `node`, such as `github.event.commits.*.message`.
    path: String,
}

impl Cursor {
    /// Moves the cursor to `child`, a child of its node.
    fn descend(&mut self, child: &'static UntrustedInput) {
        self.node = child;
        self.path.push('.');
        self.path.push_str(child.name);
    }
}
