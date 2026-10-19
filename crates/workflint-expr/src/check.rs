//! The type checker of expressions.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `ExprSemanticsChecker` in `expr_sema.go`.

use workflint_data::{RESERVED_CONFIG_VARIABLE_PREFIX, is_config_variable_name_char};
use workflint_diagnostics::RuleMeta;
use workflint_diagnostics::quote::{ordinal, quote, quotes, sorted_quotes};

use crate::ast::{CompareOp, Expr, ExprKind, LogicalOp, Name};
use crate::env::TypeEnv;
use crate::error::ExprError;
use crate::format::placeholders;
use crate::functions::{self, Function, Mismatch, Signature};
use crate::json;
use crate::rules::{
    CASE_ARGUMENT_COUNT, CONTEXT_NOT_ALLOWED, FUNCTION_NOT_ALLOWED, INVALID_COMPARISON,
    INVALID_CONFIG_VARIABLE_NAME, INVALID_INDEX_ACCESS, INVALID_JSON_LITERAL,
    INVALID_OBJECT_FILTER, INVALID_PROPERTY_ACCESS, MISSING_FORMAT_ARGUMENT,
    UNDEFINED_CONFIG_VARIABLE, UNDEFINED_FUNCTION, UNDEFINED_PROPERTY, UNDEFINED_VARIABLE,
    UNUSED_FORMAT_ARGUMENT, WRONG_ARGUMENT_COUNT, WRONG_ARGUMENT_TYPE,
};
use crate::suggest::{closest, did_you_mean};
use crate::types::{ObjectTy, Ty};
use crate::untrusted::UntrustedChecker;

const CONTEXT_AVAILABILITY: &str = "see https://docs.github.com/en/actions/learn-github-actions/contexts#context-availability for more details";

const CONFIG_VARIABLE_CONVENTION: &str = "note: see the convention at https://docs.github.com/en/actions/learn-github-actions/variables#naming-conventions-for-configuration-variables";

impl TypeEnv {
    /// Type-checks `expr` against the contexts of this environment and returns its type and
    /// errors, in the reference's order.
    ///
    /// Each error starts at the token that the reference reports it at and ends at the end of
    /// the node it is about. Its rule is one of the rules in [`rules`](crate::rules) besides
    /// the syntax rules.
    ///
    /// A call of a built-in function has the return type of the first overload that accepts
    /// its arguments, and `any` when no overload does or the function is not built in.
    ///
    /// When [`set_untrusted_inputs`](Self::set_untrusted_inputs) turned the check on, the
    /// errors about untrusted input follow the semantic errors, as in the reference's `Check`.
    #[must_use]
    pub fn check(&self, expr: &Expr) -> (Ty, Vec<ExprError>) {
        let mut checker = Checker {
            env: self,
            errors: Vec::new(),
            untrusted: self.untrusted_inputs().map(UntrustedChecker::new),
        };
        let ty = checker.check(expr);
        let mut errors = checker.errors;
        if let Some(untrusted) = checker.untrusted {
            errors.extend(untrusted.finish());
        }
        (ty, errors)
    }
}

impl Expr {
    /// Returns whether the expression's value is the same in every run, such as `1 == 2` or
    /// `!null`, as the `if-cond` rule needs. The reference's `IsConstant`.
    ///
    /// Literals are constant, and contexts are not. An operator is constant when all its
    /// operands are, even when it would short-circuit: `foo && false` is not constant. A call
    /// is constant when its arguments are and the function returns the same value for the
    /// same arguments, as `startsWith` does and `success` and `hashFiles` do not.
    ///
    /// ```
    /// use workflint_expr::parse_expression;
    ///
    /// let (expr, _) = parse_expression("true && ('a' == 'b') }}").unwrap();
    /// assert!(expr.is_constant());
    /// let (expr, _) = parse_expression("github.ref == 'main' }}").unwrap();
    /// assert!(!expr.is_constant());
    /// let (expr, _) = parse_expression("format('{0}', startsWith('foo', 'f')) }}").unwrap();
    /// assert!(expr.is_constant());
    /// ```
    #[must_use]
    pub fn is_constant(&self) -> bool {
        match &self.kind {
            ExprKind::Null
            | ExprKind::Bool(_)
            | ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::String(_) => true,
            ExprKind::Variable(_)
            | ExprKind::Property { .. }
            | ExprKind::ObjectFilter { .. }
            | ExprKind::Index { .. } => false,
            ExprKind::Call { callee, args } => {
                args.iter().all(Expr::is_constant)
                    && workflint_data::builtin_function(callee.as_str())
                        .is_some_and(|function| function.constant)
            }
            ExprKind::Not { operand } => operand.is_constant(),
            ExprKind::Compare { left, right, .. } => left.is_constant() && right.is_constant(),
            ExprKind::Logical { operands, .. } => operands.iter().all(Expr::is_constant),
        }
    }
}

struct Checker<'env> {
    env: &'env TypeEnv,
    errors: Vec<ExprError>,
    /// The untrusted-input check, if the environment turns it on. It sees each node that the
    /// type checker checks, before and after the node's children, in the order the type
    /// checker checks them.
    untrusted: Option<UntrustedChecker>,
}

impl Checker<'_> {
    /// Reports an error about `node` at its token, as the reference's `errorAtExpr` does.
    fn error(&mut self, node: &Expr, rule: &'static RuleMeta, message: String) {
        self.error_with_help(node, rule, message, None);
    }

    /// Reports an error about `node` at its token, with a help text.
    fn error_with_help(
        &mut self,
        node: &Expr,
        rule: &'static RuleMeta,
        message: String,
        help: Option<String>,
    ) {
        self.errors.push(ExprError {
            message,
            start: node.token.start,
            end: node.end,
            rule,
            help,
        });
    }

    fn check(&mut self, expr: &Expr) -> Ty {
        if let Some(untrusted) = &mut self.untrusted {
            untrusted.enter(expr);
        }
        let ty = self.check_node(expr);
        if let Some(untrusted) = &mut self.untrusted {
            untrusted.leave(expr);
        }
        ty
    }

    fn check_node(&mut self, expr: &Expr) -> Ty {
        match &expr.kind {
            ExprKind::Variable(name) => self.check_variable(expr, name),
            ExprKind::Null => Ty::Null,
            ExprKind::Bool(_) => Ty::Bool,
            ExprKind::Int(_) | ExprKind::Float(_) => Ty::Number,
            ExprKind::String(_) => Ty::String,
            ExprKind::Property { receiver, name } => self.check_property(expr, receiver, name),
            ExprKind::ObjectFilter { receiver } => self.check_object_filter(expr, receiver),
            ExprKind::Index { operand, index } => self.check_index(expr, operand, index),
            ExprKind::Call { callee, args } => self.check_call(expr, callee, args),
            // `!` converts its operand to a boolean, which every type allows
            // (`docs/differences.md#bool-assignability`).
            ExprKind::Not { operand } => {
                self.check(operand);
                Ty::Bool
            }
            ExprKind::Compare { op, left, right } => {
                let left_ty = self.check(left);
                let right_ty = self.check(right);
                if !comparable(*op, &left_ty, &right_ty) {
                    let message = format!(
                        "{} value cannot be compared to {} value with {} operator",
                        quote(&left_ty.to_string()),
                        quote(&right_ty.to_string()),
                        quote(op.as_str()),
                    );
                    self.error(expr, &INVALID_COMPARISON, message);
                }
                Ty::Bool
            }
            ExprKind::Logical { op, operands } => self.check_chain(*op, operands),
        }
    }

    fn check_variable(&mut self, expr: &Expr, name: &Name) -> Ty {
        if let Some(ty) = self.env.variable(name.as_str()) {
            self.check_available_context(expr, name);
            return ty.clone();
        }
        let message = format!(
            "undefined variable {}. available variables are {}",
            quote(name.original()),
            sorted_quotes(self.env.variable_names()),
        );
        let help = self.variable_suggestion(name.as_str());
        self.error_with_help(expr, &UNDEFINED_VARIABLE, message, help);
        Ty::Any
    }

    /// Suggests the variable closest to `name`, or the call of a function that takes no
    /// arguments, such as `always()` for `always`.
    fn variable_suggestion(&self, name: &str) -> Option<String> {
        let variables = self
            .env
            .variable_names()
            .map(|variable| (variable, variable.to_owned()));
        let calls = functions::all()
            .filter(|function| function.takes_no_arguments())
            .map(|function| (function.name, format!("{}()", function.name)));
        closest(name, variables.chain(calls)).map(|candidate| did_you_mean(&candidate))
    }

    /// The reference's `checkAvailableContext`. The message names the context in lower case,
    /// as the reference does.
    fn check_available_context(&mut self, expr: &Expr, name: &Name) {
        let availability = self.env.availability();
        let context = name.as_str();
        if availability.contexts.contains(&context) {
            return;
        }
        let note = match availability.contexts {
            [] => "no context is available here".to_owned(),
            [only] => format!("available context is {}", quote(only)),
            contexts => format!("available contexts are {}", quotes(contexts)),
        };
        let message = format!(
            "context {} is not allowed here. {note}. {CONTEXT_AVAILABILITY}",
            quote(context)
        );
        self.error(expr, &CONTEXT_NOT_ALLOWED, message);
    }

    /// The reference's `checkObjectDeref`.
    fn check_property(&mut self, expr: &Expr, receiver: &Expr, name: &Name) -> Ty {
        let receiver_ty = self.check(receiver);
        let prop = name.as_str();
        if let Some(ty) = self.event_inputs(receiver, prop) {
            return ty;
        }
        match &receiver_ty {
            Ty::Any => Ty::Any,
            Ty::Object(object) => {
                if let Some(ty) = object.prop_ignore_case(prop) {
                    return ty.clone();
                }
                if let Some(ty) = object.unknown_prop_ty() {
                    if matches!(&receiver.kind, ExprKind::Variable(var) if var.as_str() == "vars") {
                        self.check_config_variable(expr, prop);
                    }
                    return ty.clone();
                }
                let message = format!(
                    "property {} is not defined in object type {object}",
                    quote(prop)
                );
                let help = property_suggestion(object, prop);
                self.error_with_help(expr, &UNDEFINED_PROPERTY, message, help);
                Ty::Any
            }
            Ty::Array(array) if array.is_filtered() => match array.elem() {
                Ty::Any => receiver_ty.clone(),
                Ty::Object(elem) => {
                    if let Some(ty) = elem
                        .prop_ignore_case(prop)
                        .or_else(|| elem.unknown_prop_ty())
                    {
                        return Ty::filtered_array(ty.clone());
                    }
                    let message = format!(
                        "property {} is not defined in object type {elem} as element of filtered array",
                        quote(prop)
                    );
                    let help = property_suggestion(elem, prop);
                    self.error_with_help(expr, &UNDEFINED_PROPERTY, message, help);
                    Ty::filtered_array(Ty::Any)
                }
                elem => {
                    let message = format!(
                        "property filtered by {} at object filtering must be type of object but got {}",
                        quote(prop),
                        quote(&elem.to_string())
                    );
                    self.error(expr, &INVALID_PROPERTY_ACCESS, message);
                    Ty::Any
                }
            },
            Ty::Null | Ty::Number | Ty::Bool | Ty::String | Ty::Array(_) => {
                let message = format!(
                    "receiver of object dereference {} must be type of object but got {}",
                    quote(prop),
                    quote(&receiver_ty.to_string())
                );
                self.error(expr, &INVALID_PROPERTY_ACCESS, message);
                Ty::Any
            }
        }
    }

    /// Returns the type of `github.event.inputs` when `receiver` is `github.event`, `prop` is
    /// `inputs`, and the environment has inputs from `workflow_dispatch`. The reference copies
    /// `github` and adds `inputs` to `event`. This reads it from the environment instead.
    fn event_inputs(&self, receiver: &Expr, prop: &str) -> Option<Ty> {
        if prop != "inputs" {
            return None;
        }
        let event = match &receiver.kind {
            ExprKind::Property { receiver, name } if name.as_str() == "event" => receiver,
            ExprKind::Index { operand, index } if is_literal_key(index, "event") => operand,
            _ => return None,
        };
        if !matches!(&event.kind, ExprKind::Variable(name) if name.as_str() == "github") {
            return None;
        }
        self.env.event_inputs().cloned()
    }

    /// The reference's `checkConfigVariables`. `name` is lower case.
    fn check_config_variable(&mut self, expr: &Expr, name: &str) {
        if name.starts_with(RESERVED_CONFIG_VARIABLE_PREFIX) {
            let message = format!(
                "configuration variable name {} must not start with the GITHUB_ prefix (case insensitive). {CONFIG_VARIABLE_CONVENTION}",
                quote(name)
            );
            self.error(expr, &INVALID_CONFIG_VARIABLE_NAME, message);
            return;
        }
        // The lexer already rejects a name that starts with a digit.
        if !name.chars().all(is_config_variable_name_char) {
            let message = format!(
                "configuration variable name {} can only contain alphabets, decimal numbers, and '_'. {CONFIG_VARIABLE_CONVENTION}",
                quote(name)
            );
            self.error(expr, &INVALID_CONFIG_VARIABLE_NAME, message);
            return;
        }

        let Some(config_vars) = self.env.config_vars() else {
            return;
        };
        if config_vars.is_empty() {
            let message = format!(
                "no configuration variable is allowed since the variables list is empty in actionlint.yaml. you may forget adding the variable {} to the list",
                quote(name)
            );
            self.error(expr, &UNDEFINED_CONFIG_VARIABLE, message);
            return;
        }
        if config_vars.iter().any(|var| var.eq_ignore_ascii_case(name)) {
            return;
        }
        let message = format!(
            "undefined configuration variable {}. defined configuration variables in actionlint.yaml are {}",
            quote(name),
            sorted_quotes(config_vars),
        );
        self.error(expr, &UNDEFINED_CONFIG_VARIABLE, message);
    }

    /// The reference's `checkArrayDeref`. A filter returns a new type and leaves the type of
    /// its receiver unchanged (`docs/differences.md#object-filter-mutation`).
    fn check_object_filter(&mut self, expr: &Expr, receiver: &Expr) -> Ty {
        let receiver_ty = self.check(receiver);
        match &receiver_ty {
            Ty::Any => Ty::filtered_array(Ty::Any),
            Ty::Array(array) => Ty::Array(array.to_filtered()),
            Ty::Object(object) => {
                if let Some(elem) = object.unknown_prop_ty() {
                    return match elem {
                        Ty::Any | Ty::Object(_) => Ty::filtered_array(elem.clone()),
                        _ => {
                            let message = format!(
                                "elements of object at receiver of object filtering `.*` must be type of object but got {}. the type of receiver was {}",
                                quote(&elem.to_string()),
                                quote(&receiver_ty.to_string())
                            );
                            self.error(expr, &INVALID_OBJECT_FILTER, message);
                            Ty::Any
                        }
                    };
                }
                if object
                    .props()
                    .values()
                    .any(|ty| matches!(ty, Ty::Object(_)))
                {
                    return Ty::filtered_array(Ty::Any);
                }
                let message = format!(
                    "object type {} cannot be filtered by object filtering `.*` since it has no object element",
                    quote(&receiver_ty.to_string())
                );
                self.error(expr, &INVALID_OBJECT_FILTER, message);
                Ty::Any
            }
            Ty::Null | Ty::Number | Ty::Bool | Ty::String => {
                let message = format!(
                    "receiver of object filtering `.*` must be type of array or object but got {}",
                    quote(&receiver_ty.to_string())
                );
                self.error(expr, &INVALID_OBJECT_FILTER, message);
                Ty::Any
            }
        }
    }

    /// The reference's `checkIndexAccess`. The index is checked before the operand, as in the
    /// reference, which orders the errors.
    ///
    /// A string literal key is compared without regard to case, as GitHub does, and messages
    /// keep its spelling (`docs/differences.md#literal-index-case`).
    fn check_index(&mut self, expr: &Expr, operand: &Expr, index: &Expr) -> Ty {
        let index_ty = self.check(index);
        let operand_ty = self.check(operand);
        if let ExprKind::String(lit) = &index.kind
            && let Some(ty) = self.event_inputs(operand, lit.folded())
        {
            return ty;
        }
        match &operand_ty {
            Ty::Any => Ty::Any,
            Ty::Array(array) => match index_ty {
                Ty::Any | Ty::Number => array.elem().clone(),
                _ => {
                    let message = format!(
                        "index access of array must be type of number but got {}",
                        quote(&index_ty.to_string())
                    );
                    self.error(index, &INVALID_INDEX_ACCESS, message);
                    Ty::Any
                }
            },
            Ty::Object(object) => match index_ty {
                Ty::Any => Ty::Any,
                Ty::String => {
                    if let ExprKind::String(lit) = &index.kind {
                        // The exact key first, for keys that are not lower case.
                        let prop = object
                            .prop(lit.value())
                            .or_else(|| object.prop_ignore_case(lit.folded()));
                        if let Some(ty) = prop.or_else(|| object.unknown_prop_ty()) {
                            return ty.clone();
                        }
                        let message = format!(
                            "property {} is not defined in object type {object}",
                            quote(lit.value())
                        );
                        let help = property_suggestion(object, lit.folded());
                        self.error_with_help(expr, &UNDEFINED_PROPERTY, message, help);
                    }
                    object.unknown_prop_ty().cloned().unwrap_or(Ty::Any)
                }
                _ => {
                    let message = format!(
                        "property access of object must be type of string but got {}",
                        quote(&index_ty.to_string())
                    );
                    self.error(index, &INVALID_INDEX_ACCESS, message);
                    Ty::Any
                }
            },
            Ty::Null | Ty::Number | Ty::Bool | Ty::String => {
                let message = format!(
                    "index access operand must be type of object or array but got {}",
                    quote(&operand_ty.to_string())
                );
                self.error(expr, &INVALID_INDEX_ACCESS, message);
                Ty::Any
            }
        }
    }

    /// The reference's `checkFuncCall`. The arguments of a function that is not built in are
    /// not checked.
    fn check_call(&mut self, expr: &Expr, callee: &Name, args: &[Expr]) -> Ty {
        let Some(function) = functions::builtin(callee.as_str()) else {
            let message = format!(
                "undefined function {}. available functions are {}",
                quote(callee.original()),
                sorted_quotes(functions::keys()),
            );
            let help = closest(
                callee.as_str(),
                functions::all().map(|function| (function.name, function.name)),
            )
            .map(did_you_mean);
            self.error_with_help(expr, &UNDEFINED_FUNCTION, message, help);
            return Ty::Any;
        };

        let arg_tys: Vec<Ty> = args.iter().map(|arg| self.check(arg)).collect();
        // The first overload that accepts the arguments wins. The messages are built only
        // when every overload fails, and then every overload's error is reported.
        if let Some(sig) = function
            .overloads
            .iter()
            .find(|sig| sig.mismatch(&arg_tys).is_none())
        {
            return self.check_resolved_call(expr, callee, function, sig, args);
        }
        for sig in &function.overloads {
            if let Some(mismatch) = sig.mismatch(&arg_tys) {
                self.report_mismatch(expr, sig, mismatch, args, &arg_tys);
            }
        }
        Ty::Any
    }

    /// Reports why `sig` does not accept the arguments, as the reference's
    /// `checkFuncSignature` does.
    fn report_mismatch(
        &mut self,
        expr: &Expr,
        sig: &Signature,
        mismatch: Mismatch,
        args: &[Expr],
        arg_tys: &[Ty],
    ) {
        let sig_text = sig.to_string();
        match mismatch {
            Mismatch::Count => {
                let at_least = if sig.is_variadic() { "at least " } else { "" };
                let message = format!(
                    "number of arguments is wrong. function {} takes {at_least}{} parameters but {} arguments are given",
                    quote(&sig_text),
                    sig.param_count(),
                    args.len(),
                );
                self.error(expr, &WRONG_ARGUMENT_COUNT, message);
            }
            Mismatch::Type(index) => {
                let (Some(arg), Some(arg_ty), Some(param)) =
                    (args.get(index), arg_tys.get(index), sig.param(index))
                else {
                    return;
                };
                let message = format!(
                    "{} argument of function call is not assignable. {} cannot be assigned to {}. called function type is {}",
                    ordinal(index + 1),
                    quote(&arg_ty.to_string()),
                    quote(&param.to_string()),
                    quote(&sig_text),
                );
                self.error(arg, &WRONG_ARGUMENT_TYPE, message);
            }
        }
    }

    /// The reference's `checkBuiltinFuncCall`: the checks of a call whose arguments `sig`
    /// accepts. Returns the type of the call.
    fn check_resolved_call(
        &mut self,
        expr: &Expr,
        callee: &Name,
        function: &Function,
        sig: &Signature,
        args: &[Expr],
    ) -> Ty {
        self.check_special_function_availability(expr, callee);
        match function.key.as_str() {
            "format" => self.check_format(expr, args),
            "fromjson" => {
                if let Some(ty) = self.check_from_json(args) {
                    return ty;
                }
            }
            "case" if args.len().is_multiple_of(2) => {
                let message = format!(
                    "case() requires an odd number of arguments (pred/value pairs + default) but got {}",
                    args.len()
                );
                self.error(expr, &CASE_ARGUMENT_COUNT, message);
            }
            _ => {}
        }
        sig.ret.clone()
    }

    /// The reference's `checkSpecialFunctionAvailability`. It runs only for a call whose
    /// arguments an overload accepts, so a call with the wrong arguments gets one error. The
    /// message keeps the spelling of the call.
    fn check_special_function_availability(&mut self, expr: &Expr, callee: &Name) {
        let availability = self.env.availability();
        let name = callee.as_str();
        let Some(keys) = workflint_data::special_function_keys(name) else {
            return;
        };
        if availability.special_functions.contains(&name) {
            return;
        }
        let original = quote(callee.original());
        let message = format!(
            "calling function {original} is not allowed here. {original} is only available in {}. {CONTEXT_AVAILABILITY}",
            quotes(keys)
        );
        self.error(expr, &FUNCTION_NOT_ALLOWED, message);
    }

    /// Checks the placeholders of `format()` against its arguments when the format string is
    /// a literal. The reference reports the placeholders beyond the arguments in Go map order,
    /// which varies. This reports them in ascending order.
    fn check_format(&mut self, expr: &Expr, args: &[Expr]) {
        let Some((first, values)) = args.split_first() else {
            return;
        };
        let ExprKind::String(lit) = &first.kind else {
            return;
        };
        let format = quote(lit.value());
        let mut remaining = placeholders(lit.value());
        let count = u64::try_from(values.len()).unwrap_or(u64::MAX);
        for i in 0..count {
            if !remaining.remove(&i) {
                let message = format!(
                    "format string {format} does not contain placeholder {{{i}}}. remove argument which is unused in the format string"
                );
                self.error(expr, &UNUSED_FORMAT_ARGUMENT, message);
            }
        }
        for i in remaining {
            let message = format!(
                "format string {format} contains placeholder {{{i}}} but only {count} arguments are given to format"
            );
            self.error(expr, &MISSING_FORMAT_ARGUMENT, message);
        }
    }

    /// Returns the type of the JSON value when the argument of `fromJSON()` is a string
    /// literal, or reports Go's syntax error at the literal when it is broken JSON.
    fn check_from_json(&mut self, args: &[Expr]) -> Option<Ty> {
        let arg = args.first()?;
        let ExprKind::String(lit) = &arg.kind else {
            return None;
        };
        match json::type_of(lit.value()) {
            Ok(ty) => ty,
            Err(err) => {
                let message = format!(
                    "broken JSON string is passed to fromJSON() at offset {}: {}",
                    err.offset, err.message
                );
                self.error(arg, &INVALID_JSON_LITERAL, message);
                None
            }
        }
    }

    /// The reference's `checkLogicalOp` on a chain, which the reference nests to the right:
    /// `a || b || c` is `a || (b || c)`. When `l && r` is falsy, its value is `l`, and when
    /// `l || r` is truthy, its value is `l`. So the type of `l` is narrowed with that
    /// assumption and merged with the type of `r`. The loop checks the operands in source
    /// order and merges their types from the right, as the reference's recursion does.
    fn check_chain(&mut self, op: LogicalOp, operands: &[Expr]) -> Ty {
        let truthy = op == LogicalOp::Or;
        if let [left, right] = operands {
            // The common case, without a vector.
            let left_ty = self.check_narrowed(left, truthy);
            return left_ty.merge(&self.check(right));
        }
        let Some((last, init)) = operands.split_last() else {
            return Ty::Any;
        };
        let narrowed: Vec<Ty> = init
            .iter()
            .map(|operand| self.check_narrowed(operand, truthy))
            .collect();
        let last_ty = self.check(last);
        if operands.len() > 2 {
            self.leave_nested_chain();
        }
        narrowed
            .into_iter()
            .rev()
            .fold(last_ty, |right, left| left.merge(&right))
    }

    /// The reference's `checkWithNarrowing`: the type of `expr` when its value is assumed to
    /// be truthy or falsy. A truthy `l && r` and a falsy `l || r` have the type of `r`, and `!`
    /// flips the assumption for its operand.
    fn check_narrowed(&mut self, expr: &Expr, truthy: bool) -> Ty {
        match &expr.kind {
            ExprKind::Logical { op, operands } => {
                if (*op == LogicalOp::And) == truthy
                    && let Some((first, rest)) = operands.split_first()
                {
                    self.check(first);
                    if let [right] = rest {
                        return self.check(right);
                    }
                    let ty = self.check_chain(*op, rest);
                    self.leave_nested_chain();
                    return ty;
                }
                self.check_chain(*op, operands)
            }
            ExprKind::Not { operand } => self.check_narrowed(operand, !truthy),
            _ => self.check(expr),
        }
    }

    /// Tells the untrusted-input check that the walk leaves the node of the chain after an
    /// operand, which the reference's tree has and this one does not. The reference checks
    /// `b || c` in `a || b || c` through `check`, which leaves that node and so ends the
    /// current chain of property accesses.
    fn leave_nested_chain(&mut self) {
        if let Some(untrusted) = &mut self.untrusted {
            untrusted.leave_nested_chain();
        }
    }
}

/// Suggests the property of `object` closest to `prop`, which is lower case.
fn property_suggestion(object: &ObjectTy, prop: &str) -> Option<String> {
    closest(
        prop,
        object
            .props()
            .keys()
            .map(|key| (key.as_str(), key.as_str())),
    )
    .map(did_you_mean)
}

/// Returns whether `index` is a string literal equal to `key` without regard to case. `key`
/// is lower case.
fn is_literal_key(index: &Expr, key: &str) -> bool {
    matches!(&index.kind, ExprKind::String(lit) if lit.folded() == key)
}

/// Returns whether GitHub can compare values of the two types with `op`. The reference's
/// `validateCompareOpOperands`, which is not symmetric: `null == {}` is allowed, `{} == 1` is
/// not.
fn comparable(op: CompareOp, left: &Ty, right: &Ty) -> bool {
    if op.is_equality() {
        match left {
            Ty::Any | Ty::Null => true,
            // An object or an array becomes NaN, so the comparison is always false.
            Ty::Number | Ty::Bool | Ty::String => !matches!(right, Ty::Object(_) | Ty::Array(_)),
            Ty::Object(_) => matches!(right, Ty::Object(_) | Ty::Null | Ty::Any),
            Ty::Array(left) => match right {
                Ty::Array(right) => comparable(op, left.elem(), right.elem()),
                Ty::Null | Ty::Any => true,
                _ => false,
            },
        }
    } else {
        // `<`, `<=`, `>`, and `>=` cannot compare null, bool, objects, or arrays.
        match left {
            Ty::Any | Ty::Number | Ty::String => {
                !matches!(right, Ty::Null | Ty::Bool | Ty::Object(_) | Ty::Array(_))
            }
            Ty::Null | Ty::Bool | Ty::Object(_) | Ty::Array(_) => false,
        }
    }
}
