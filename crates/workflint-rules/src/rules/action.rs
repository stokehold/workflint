//! The `action` rule: the format of each step's `uses:`, Docker actions, outdated popular
//! actions, and the `with:` inputs of popular actions.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `rule_action.go`. M4.18 (#155) adds the
//! checks of local actions.

use workflint_data::{is_outdated_popular_action, popular_action};
use workflint_diagnostics::quote::{quote, sorted_quotes};
use workflint_diagnostics::{Category, RuleMeta, Severity, Span};
use workflint_syntax::{Exec, ExecAction, Step, StepUses, StepUsesKind, UsesPart};

use crate::{Context, Rule};

mod url;

/// A step's `uses:` is not `./path`, `docker://image`, or `owner/repo[/path]@ref`.
pub(crate) static INVALID_ACTION_FORMAT: RuleMeta = RuleMeta {
    id: "invalid-action-format",
    description: "A step's `uses:` is not a local action, a Docker image, or `{owner}/{repo}@{ref}`",
    kind: "action",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-action-format.md",
};

/// A `docker://` action has a URI that does not parse, or an empty tag.
pub(crate) static INVALID_DOCKER_ACTION: RuleMeta = RuleMeta {
    id: "invalid-docker-action",
    description: "A `docker://` action has an invalid URI or an empty tag",
    kind: "action",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-docker-action.md",
};

/// A popular action at a version whose runner GitHub no longer has.
pub(crate) static OUTDATED_ACTION: RuleMeta = RuleMeta {
    id: "outdated-action",
    description: "A popular action's version runs on a runtime that GitHub no longer has",
    kind: "action",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/outdated-action.md",
};

/// `with:` sets an input that the action does not declare.
pub(crate) static UNDEFINED_ACTION_INPUT: RuleMeta = RuleMeta {
    id: "undefined-action-input",
    description: "`with:` sets an input that the action does not declare",
    kind: "action",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/undefined-action-input.md",
};

/// A step does not set an input that the action requires.
pub(crate) static MISSING_REQUIRED_ACTION_INPUT: RuleMeta = RuleMeta {
    id: "missing-required-action-input",
    description: "A step does not set an input that its action requires",
    kind: "action",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/missing-required-action-input.md",
};

/// `with:` sets an input that the action has deprecated.
pub(crate) static DEPRECATED_ACTION_INPUT: RuleMeta = RuleMeta {
    id: "deprecated-action-input",
    description: "`with:` sets an input that the action has deprecated",
    kind: "action",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/deprecated-action-input.md",
};

/// The reference's `RuleAction`.
pub(crate) struct Action;

impl<'a> Rule<'a> for Action {
    fn step(&mut self, step: &'a Step<'a>, cx: &mut Context<'_>) {
        let Some(Exec::Action(exec)) = &step.exec else {
            return;
        };
        // An empty or non-scalar `uses` is invalid, and the parser has reported it. The
        // reference reports it again as an action whose ref is missing
        // (`docs/differences.md#invalid-value-reported-once`).
        let Some(uses) = exec.uses.value() else {
            return;
        };
        match uses.kind {
            StepUsesKind::Expression | StepUsesKind::Local { .. } => {}
            StepUsesKind::Docker { image, tag } => check_docker(uses, image, tag, cx),
            StepUsesKind::Invalid(invalid) => report_format(uses, invalid.reason(), cx),
            StepUsesKind::Repository { .. } => check_repository(exec, uses, cx),
        }
    }
}

/// The reference's `checkDockerAction`.
fn check_docker(
    uses: &StepUses<'_>,
    image: UsesPart<'_>,
    tag: Option<UsesPart<'_>>,
    cx: &mut Context<'_>,
) {
    let uri = format!("docker://{}", image.value);
    if let Some(error) = url::parse_error(&uri) {
        cx.report(
            &INVALID_DOCKER_ACTION,
            uses.span(),
            format!(
                "URI for Docker container {} is invalid: {error} (tag={})",
                quote(&uri),
                tag.map_or("", |tag| tag.value),
            ),
        );
    }
    if tag.is_some_and(|tag| tag.value.is_empty()) {
        cx.report(
            &INVALID_DOCKER_ACTION,
            uses.span(),
            format!("tag of Docker action should not be empty: {}", quote(&uri)),
        );
    }
}

/// The reference's `invalidActionFormat`.
fn report_format(uses: &StepUses<'_>, reason: &str, cx: &mut Context<'_>) {
    cx.report(
        &INVALID_ACTION_FORMAT,
        uses.span(),
        format!(
            "specifying action {} in invalid format because {reason}. available formats are \"{{owner}}/{{repo}}@{{ref}}\" or \"{{owner}}/{{repo}}/{{path}}@{{ref}}\"",
            quote(uses.value()),
        ),
    );
}

/// The lookups of the reference's `checkRepoAction`, for a value whose format is valid.
fn check_repository(exec: &ExecAction<'_>, uses: &StepUses<'_>, cx: &mut Context<'_>) {
    let spec = uses.value();
    let Some(action) = popular_action(spec) else {
        if is_outdated_popular_action(spec) {
            cx.report(
                &OUTDATED_ACTION,
                uses.span(),
                format!(
                    "the runner of {} action is too old to run on GitHub Actions. update the action's version to fix this issue",
                    quote(spec),
                ),
            );
        }
        return;
    };
    if action.skip_inputs {
        return;
    }
    let declared: Vec<_> = action.inputs.iter().map(DeclaredInput::from).collect();
    check_inputs(exec, uses.span(), &declared, &quote(spec), cx);
}

/// An input that an action's metadata declares, as the input checks read it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DeclaredInput<'d> {
    /// The name lowercased, which `with:` keys are compared with.
    pub(crate) id: &'d str,
    /// The name as the metadata writes it.
    pub(crate) name: &'d str,
    /// Whether a step must set the input: `required: true` and no `default`.
    pub(crate) required: bool,
    /// The deprecation message, if the metadata has the key. It may be empty.
    pub(crate) deprecation: Option<&'d str>,
}

impl From<&'static workflint_data::ActionInput> for DeclaredInput<'static> {
    fn from(input: &'static workflint_data::ActionInput) -> Self {
        Self {
            id: input.id,
            name: input.name,
            required: input.required,
            deprecation: input.deprecated.then_some(input.deprecation_message),
        }
    }
}

/// The reference's `checkAction`: checks the `with:` of `exec` against the inputs that the
/// action declares, sorted by ID. `describe` names the action in the messages, already quoted.
///
/// The checks of local actions in M4.18 (#155) call this too.
pub(crate) fn check_inputs(
    exec: &ExecAction<'_>,
    uses: Span,
    declared: &[DeclaredInput<'_>],
    describe: &str,
    cx: &mut Context<'_>,
) {
    let find = |id: &str| {
        declared
            .binary_search_by(|input| input.id.cmp(id))
            .ok()
            .and_then(|index| declared.get(index))
    };
    let with = exec.inputs.value();
    // The parser has reported a key that repeats an earlier one, and the reference drops it.
    let entries = with.into_iter().flat_map(|with| {
        with.iter().filter(|entry| {
            with.get(entry.folded())
                .is_some_and(|first| std::ptr::eq(first, *entry))
        })
    });
    for entry in entries {
        let name = entry.key.value();
        match find(entry.folded()) {
            None => cx.report(
                &UNDEFINED_ACTION_INPUT,
                entry.key.span(),
                format!(
                    "input {} is not defined in action {describe}. available inputs are {}",
                    quote(name),
                    sorted_quotes(declared.iter().map(|input| input.name)),
                ),
            ),
            Some(input) if !input.required => {
                if let Some(deprecation) = input.deprecation {
                    report_deprecated(name, deprecation, describe, entry.key.span(), cx);
                }
            }
            Some(_) => {}
        }
    }
    let required = || declared.iter().filter(|input| input.required);
    for input in required() {
        if with.is_some_and(|with| with.get(input.id).is_some()) {
            continue;
        }
        cx.report(
            &MISSING_REQUIRED_ACTION_INPUT,
            uses,
            format!(
                "missing input {} which is required by action {describe}. all required inputs are {}",
                quote(input.name),
                sorted_quotes(required().map(|input| input.name)),
            ),
        );
    }
}

fn report_deprecated(
    name: &str,
    deprecation: &str,
    describe: &str,
    span: Span,
    cx: &mut Context<'_>,
) {
    let mut message = format!(
        "avoid using deprecated input {} in action {describe}",
        quote(name)
    );
    let deprecation = join_lines(deprecation.trim_end_matches(['.', ' ']));
    if !deprecation.is_empty() {
        message.push_str(": ");
        message.push_str(&deprecation);
    }
    cx.report(&DEPRECATED_ACTION_INPUT, span, message);
}

/// Replaces each line break with the white space around it by one space, as the reference's
/// `reNewlineWithIndent`, `\s*\r?\n\s*`, does. Go's `\s` is ASCII white space without `\v`.
fn join_lines(text: &str) -> String {
    let is_space = |c: char| matches!(c, '\t' | '\n' | '\u{c}' | '\r' | ' ');
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(is_space) {
        let (before, from) = rest.split_at(start);
        out.push_str(before);
        let end = from.find(|c: char| !is_space(c)).unwrap_or(from.len());
        let (run, after) = from.split_at(end);
        if run.contains('\n') {
            out.push(' ');
        } else {
            out.push_str(run);
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::join_lines;

    #[test]
    fn line_breaks_with_the_white_space_around_them_become_one_space() {
        assert_eq!(join_lines("a  \n\n  b"), "a b");
        assert_eq!(join_lines("a\r\nb"), "a b");
        assert_eq!(join_lines("a  b\tc"), "a  b\tc");
        assert_eq!(join_lines("a \r b"), "a \r b");
        assert_eq!(join_lines("a\u{b}\nb"), "a\u{b} b");
    }
}
