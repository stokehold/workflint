//! The rules, one module per reference rule, and the order the linter creates them in.

use workflint_diagnostics::RuleMeta;

use crate::Rule;
use crate::kind::{self, Kind};

mod action;
mod credentials;
mod deprecated_commands;
mod env_var;
mod events;
mod expression;
mod glob;
mod id;
mod if_cond;
mod job_needs;
mod matrix;
mod permissions;
mod runner_label;
mod shell_name;
mod workflow_call;

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
pub(crate) static ALL: [&RuleMeta; 75] = [
    &action::ACTION_FILE_NOT_FOUND,
    &events::ACTIVITY_TYPES_NOT_SUPPORTED,
    &action::BROKEN_ACTION_METADATA,
    &workflow_call::BROKEN_REUSABLE_WORKFLOW,
    &events::CHOICE_INPUT_WITHOUT_OPTIONS,
    &events::CONFLICTING_FILTERS,
    &if_cond::CONSTANT_CONDITION,
    &events::CRON_TOO_FREQUENT,
    &job_needs::CYCLIC_NEEDS,
    &events::DEFAULT_NOT_IN_OPTIONS,
    &action::DEPRECATED_ACTION_INPUT,
    &deprecated_commands::DEPRECATED_WORKFLOW_COMMAND,
    &events::DUPLICATE_INPUT_OPTION,
    &job_needs::DUPLICATE_JOB_ID,
    &matrix::DUPLICATE_MATRIX_VALUE,
    &job_needs::DUPLICATE_NEEDS,
    &id::DUPLICATE_STEP_ID,
    &action::EMPTY_DEPRECATION_MESSAGE,
    &matrix::EXCLUDE_WITHOUT_MATRIX,
    &events::FILTER_NOT_SUPPORTED,
    &glob::GLOB_PATH_DOT,
    &glob::GLOB_PATH_WHITESPACE,
    &credentials::HARDCODED_PASSWORD,
    &if_cond::IF_ALWAYS_TRUE,
    &action::INVALID_ACTION_FORMAT,
    &events::INVALID_ACTIVITY_TYPE,
    &action::INVALID_BRANDING_COLOR,
    &action::INVALID_BRANDING_ICON,
    &events::INVALID_CRON,
    &action::INVALID_DOCKER_ACTION,
    &action::INVALID_DOCKERFILE_NAME,
    &env_var::INVALID_ENV_VAR_NAME,
    &glob::INVALID_GLOB,
    &events::INVALID_INPUT_DEFAULT,
    &id::INVALID_JOB_ID,
    &permissions::INVALID_PERMISSION_LEVEL,
    &permissions::INVALID_PERMISSIONS_VALUE,
    &glob::INVALID_REF_NAME,
    &workflow_call::INVALID_REUSABLE_WORKFLOW_REF,
    &runner_label::INVALID_RUNNER_LABEL_PATTERN,
    &action::INVALID_RUNS_USING,
    &shell_name::INVALID_SHELL_NAME,
    &id::INVALID_STEP_ID,
    &events::INVALID_TIMEZONE,
    &action::MISSING_ACTION_DESCRIPTION,
    &action::MISSING_ACTION_NAME,
    &action::MISSING_PRE_POST_SCRIPT,
    &action::MISSING_REQUIRED_ACTION_INPUT,
    &workflow_call::MISSING_REQUIRED_WORKFLOW_INPUT,
    &workflow_call::MISSING_REQUIRED_WORKFLOW_SECRET,
    &action::MISSING_RUNS_KEY,
    &action::MISSING_RUNS_USING,
    &events::MISSING_WORKFLOW_RUN_WORKFLOWS,
    &expression::OBJECT_IN_TEMPLATE,
    &events::OPTIONS_ON_NON_CHOICE_INPUT,
    &action::OUTDATED_ACTION,
    &events::REQUIRED_INPUT_WITH_DEFAULT,
    &workflow_call::REUSABLE_WORKFLOW_NOT_FOUND,
    &runner_label::RUNNER_LABEL_CONFLICT,
    &action::RUNS_KEY_NOT_ALLOWED,
    &expression::SINGLE_EXPRESSION_REQUIRED,
    &events::TOO_MANY_DISPATCH_INPUTS,
    &action::UNDEFINED_ACTION_INPUT,
    &workflow_call::UNDEFINED_WORKFLOW_INPUT,
    &workflow_call::UNDEFINED_WORKFLOW_SECRET,
    &events::UNKNOWN_EVENT,
    &matrix::UNKNOWN_EXCLUDE_KEY,
    &job_needs::UNKNOWN_NEEDED_JOB,
    &permissions::UNKNOWN_PERMISSION_SCOPE,
    &runner_label::UNKNOWN_RUNNER_LABEL,
    &matrix::UNMATCHED_EXCLUDE_VALUE,
    &events::WORKFLOWS_FILTER_NOT_SUPPORTED,
    &expression::WRONG_EXPRESSION_TYPE,
    &expression::WRONG_INPUT_DEFAULT_TYPE,
    &expression::WRONG_WORKFLOW_CALL_INPUT_TYPE,
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
        "action" => Some(Box::new(action::Action)),
        "credentials" => Some(Box::new(credentials::Credentials)),
        "deprecated-commands" => Some(Box::new(deprecated_commands::DeprecatedCommands)),
        "env-var" => Some(Box::new(env_var::EnvVar)),
        "events" => Some(Box::new(events::Events)),
        "expression" => Some(Box::new(expression::Expression::new())),
        "glob" => Some(Box::new(glob::Glob)),
        "id" => Some(Box::new(id::Id::default())),
        "if-cond" => Some(Box::new(if_cond::IfCond)),
        "job-needs" => Some(Box::new(job_needs::JobNeeds::default())),
        "matrix" => Some(Box::new(matrix::Matrix)),
        "permissions" => Some(Box::new(permissions::Permissions)),
        // M5.2 passes the labels of the config's `self-hosted-runner.labels`.
        "runner-label" => Some(Box::new(runner_label::RunnerLabel::new(Vec::new()))),
        "shell-name" => Some(Box::new(shell_name::ShellName::new())),
        "workflow-call" => Some(Box::new(workflow_call::WorkflowCall)),
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
