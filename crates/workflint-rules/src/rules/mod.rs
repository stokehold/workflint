//! The rules, one module per reference rule, and the order the linter creates them in.

use workflint_diagnostics::RuleMeta;

use crate::Rule;
use crate::kind::{self, Kind};

mod events;
mod expression;
mod if_cond;

/// The kinds of the reference's rules in the order its linter creates them for each file
/// (`linter.go:558`): matrix, credentials, shell-name, runner-label, events, job-needs, action,
/// env-var, id, glob, permissions, workflow-call, expression, deprecated-commands, if-cond,
/// shellcheck, and pyflakes.
///
/// The order decides which rule reports an error that a shared cache of local actions or
/// reusable workflows returns only once, so `action` and `workflow-call` report it before
/// `expression` does. Keep it when adding rules.
pub static CREATION_ORDER: [&Kind; 17] = [
    &kind::MATRIX,
    &kind::CREDENTIALS,
    &kind::SHELL_NAME,
    &kind::RUNNER_LABEL,
    &kind::EVENTS,
    &kind::JOB_NEEDS,
    &kind::ACTION,
    &kind::ENV_VAR,
    &kind::ID,
    &kind::GLOB,
    &kind::PERMISSIONS,
    &kind::WORKFLOW_CALL,
    &kind::EXPRESSION,
    &kind::DEPRECATED_COMMANDS,
    &kind::IF_COND,
    &kind::SHELLCHECK,
    &kind::PYFLAKES,
];

/// The rule metadata that the rule modules of this crate define, sorted by rule ID.
pub(crate) static ALL: [&RuleMeta; 20] = [
    &events::ACTIVITY_TYPES_NOT_SUPPORTED,
    &events::CHOICE_INPUT_WITHOUT_OPTIONS,
    &events::CONFLICTING_FILTERS,
    &if_cond::CONSTANT_CONDITION,
    &events::DEFAULT_NOT_IN_OPTIONS,
    &events::DUPLICATE_INPUT_OPTION,
    &events::FILTER_NOT_SUPPORTED,
    &if_cond::IF_ALWAYS_TRUE,
    &events::INVALID_ACTIVITY_TYPE,
    &events::INVALID_INPUT_DEFAULT,
    &events::MISSING_WORKFLOW_RUN_WORKFLOWS,
    &expression::OBJECT_IN_TEMPLATE,
    &events::OPTIONS_ON_NON_CHOICE_INPUT,
    &events::REQUIRED_INPUT_WITH_DEFAULT,
    &expression::SINGLE_EXPRESSION_REQUIRED,
    &events::TOO_MANY_DISPATCH_INPUTS,
    &events::UNKNOWN_EVENT,
    &events::WORKFLOWS_FILTER_NOT_SUPPORTED,
    &expression::WRONG_EXPRESSION_TYPE,
    &expression::WRONG_INPUT_DEFAULT_TYPE,
];

/// Creates a fresh instance of each rule for one file, in [`CREATION_ORDER`].
#[must_use]
pub fn create_rules<'a>() -> Vec<Box<dyn Rule<'a> + 'a>> {
    create_in_order(create)
}

/// Creates the rule of `kind`, or returns `None` while the rule does not exist yet.
///
/// Each task that ports a reference rule adds an arm here. shellcheck and pyflakes come with
/// M5.5, and only when their executable is found.
fn create<'a>(kind: &Kind) -> Option<Box<dyn Rule<'a> + 'a>> {
    match kind.name {
        "events" => Some(Box::new(events::Events)),
        "expression" => Some(Box::new(expression::Expression::new())),
        "if-cond" => Some(Box::new(if_cond::IfCond)),
        _ => None,
    }
}

/// Calls `create` for each kind of [`CREATION_ORDER`] and keeps the rules it returns.
fn create_in_order<'a>(
    create: impl Fn(&'static Kind) -> Option<Box<dyn Rule<'a> + 'a>>,
) -> Vec<Box<dyn Rule<'a> + 'a>> {
    CREATION_ORDER
        .iter()
        .filter_map(|kind| create(kind))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    #[test]
    fn the_creation_order_is_the_reference_order() {
        let names: Vec<_> = CREATION_ORDER.iter().map(|kind| kind.name).collect();
        assert_eq!(
            names,
            [
                "matrix",
                "credentials",
                "shell-name",
                "runner-label",
                "events",
                "job-needs",
                "action",
                "env-var",
                "id",
                "glob",
                "permissions",
                "workflow-call",
                "expression",
                "deprecated-commands",
                "if-cond",
                "shellcheck",
                "pyflakes",
            ]
        );
    }

    #[test]
    fn every_kind_except_syntax_check_has_a_place_in_the_creation_order() {
        for kind in kind::ALL {
            assert_eq!(
                CREATION_ORDER.contains(&kind),
                kind.name != "syntax-check",
                "{}",
                kind.name
            );
        }
    }

    /// A rule with no hooks.
    struct Stub;

    impl Rule<'_> for Stub {}

    #[test]
    fn rules_are_created_in_the_creation_order_and_missing_rules_are_skipped() {
        let created = RefCell::new(Vec::new());
        let rules = create_in_order(|kind| {
            created.borrow_mut().push(kind.name);
            (kind.name != "glob").then(|| Box::new(Stub) as Box<dyn Rule<'_>>)
        });
        let order: Vec<_> = CREATION_ORDER.iter().map(|kind| kind.name).collect();
        assert_eq!(created.into_inner(), order);
        assert_eq!(rules.len(), order.len() - 1);
    }

    #[test]
    fn every_rule_that_exists_is_created() {
        let rules = create_rules();
        let exist = CREATION_ORDER
            .iter()
            .filter(|kind| create(kind).is_some())
            .count();
        assert_eq!(rules.len(), exist);
    }
}
