//! The `events` rule, the reference's `RuleEvents` (`rule_events.go`): the cron expressions and
//! time zones of `schedule`, the webhook events of `on`, their activity types and filters, and
//! the inputs of `workflow_call` and `workflow_dispatch`.
//!
//! The parser has reported every value that is not a valid string, and the rule skips such a
//! value instead of checking the reference's `""` in its place
//! (`docs/differences.md#invalid-value-reported-once`). The value still counts where a check
//! only needs it to be written, such as `types` on an event without activity types.

mod cron;

use std::collections::BTreeSet;
use std::sync::Arc;

use workflint_data::{
    ActivityTypes, EventFilter, MAX_WORKFLOW_DISPATCH_INPUTS, MIN_SCHEDULE_INTERVAL_SECONDS,
    filter_events, is_time_zone, webhook_event,
};
use workflint_diagnostics::quote::{quote, quotes, sorted_quotes};
use workflint_diagnostics::{Category, RuleMeta, Severity};
use workflint_syntax::strconv::{format_float, parse_float};
use workflint_syntax::{
    Event, Field, KeyedEntry, ScheduleEvent, Template, WebhookEvent, Workflow, WorkflowCallEvent,
    WorkflowCallInputType, WorkflowDispatchEvent, WorkflowDispatchInput, WorkflowDispatchInputType,
    contains_expression,
};

use crate::kind::{self, Kind};
use crate::{Context, Rule};

/// A `schedule` cron expression that robfig/cron cannot parse: `invalid CRON format %q in
/// schedule event: %s`.
pub(crate) static INVALID_CRON: RuleMeta = events(
    "invalid-cron",
    "A `schedule` cron expression does not parse",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-cron.md",
);

/// A schedule that runs more often than every 5 minutes: `scheduled job runs too frequently. it
/// runs once per %g seconds`.
pub(crate) static CRON_TOO_FREQUENT: RuleMeta = events(
    "cron-too-frequent",
    "A `schedule` cron expression runs more often than every 5 minutes",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/cron-too-frequent.md",
);

/// A `schedule` time zone that is not an IANA time zone name: `invalid timezone %q in schedule
/// event`.
pub(crate) static INVALID_TIMEZONE: RuleMeta = events(
    "invalid-timezone",
    "A `schedule` time zone is not an IANA time zone name",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-timezone.md",
);

/// `on` names an event that is not a webhook event: `unknown Webhook event %q`.
pub(crate) static UNKNOWN_EVENT: RuleMeta = events(
    "unknown-event",
    "`on` names an event that is not a GitHub webhook event",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/unknown-event.md",
);

/// `types` on an event that has no activity types: `"types" cannot be specified for %q Webhook
/// event`.
pub(crate) static ACTIVITY_TYPES_NOT_SUPPORTED: RuleMeta = events(
    "activity-types-not-supported",
    "`types` is set on an event that has no activity types",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/activity-types-not-supported.md",
);

/// `types` lists a type that the event does not have: `invalid activity type %q for %q Webhook
/// event`.
pub(crate) static INVALID_ACTIVITY_TYPE: RuleMeta = events(
    "invalid-activity-type",
    "`types` lists an activity type that the event does not have",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-activity-type.md",
);

/// `workflow_run` without `workflows`: `no workflow is configured for "workflow_run" event`.
pub(crate) static MISSING_WORKFLOW_RUN_WORKFLOWS: RuleMeta = events(
    "missing-workflow-run-workflows",
    "A `workflow_run` event has no `workflows`",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/missing-workflow-run-workflows.md",
);

/// `workflows` on another event than `workflow_run`: `"workflows" cannot be configured for %q
/// event`.
pub(crate) static WORKFLOWS_FILTER_NOT_SUPPORTED: RuleMeta = events(
    "workflows-filter-not-supported",
    "`workflows` is set on an event other than `workflow_run`",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/workflows-filter-not-supported.md",
);

/// A branch, tag, or path filter on an event that does not take it: `%q filter is not available
/// for %s event`.
pub(crate) static FILTER_NOT_SUPPORTED: RuleMeta = events(
    "filter-not-supported",
    "A branch, tag, or path filter is set on an event that does not support it",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/filter-not-supported.md",
);

/// A filter and its `-ignore` filter on one event: `both %q and %q filters cannot be used for
/// the same event %q`.
pub(crate) static CONFLICTING_FILTERS: RuleMeta = events(
    "conflicting-filters",
    "A filter and its `-ignore` filter are both set on one event",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/conflicting-filters.md",
);

/// The default of a `number` or `boolean` input does not parse as its type.
pub(crate) static INVALID_INPUT_DEFAULT: RuleMeta = events(
    "invalid-input-default",
    "The default of a `number` or `boolean` input of `workflow_call` or `workflow_dispatch` does not match its type",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-input-default.md",
);

/// A required `workflow_call` input has a default: `input %q of workflow_call event has the
/// default value %q, but it is also required`.
pub(crate) static REQUIRED_INPUT_WITH_DEFAULT: RuleMeta = events(
    "required-input-with-default",
    "A required `workflow_call` input has a default, which is never used",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/required-input-with-default.md",
);

/// A `choice` input without options: `input type of %q is "choice" but "options" is not set`.
pub(crate) static CHOICE_INPUT_WITHOUT_OPTIONS: RuleMeta = events(
    "choice-input-without-options",
    "A `choice` input of `workflow_dispatch` has no `options`",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/choice-input-without-options.md",
);

/// A `choice` input lists an option twice: `option %q is duplicated in options of %q input`.
pub(crate) static DUPLICATE_INPUT_OPTION: RuleMeta = events(
    "duplicate-input-option",
    "A `choice` input of `workflow_dispatch` lists an option more than once",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/duplicate-input-option.md",
);

/// The default of a `choice` input is not an option: `default value %q of %q input is not
/// included in its options %s`.
pub(crate) static DEFAULT_NOT_IN_OPTIONS: RuleMeta = events(
    "default-not-in-options",
    "The default of a `choice` input of `workflow_dispatch` is not one of its options",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/default-not-in-options.md",
);

/// `options` on an input that is not a `choice`: `"options" can not be set to %q input because
/// its input type is not "choice"`.
pub(crate) static OPTIONS_ON_NON_CHOICE_INPUT: RuleMeta = events(
    "options-on-non-choice-input",
    "A `workflow_dispatch` input that is not a `choice` has `options`",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/options-on-non-choice-input.md",
);

/// `workflow_dispatch` has more inputs than GitHub allows: `maximum number of inputs for
/// "workflow_dispatch" event is 25 but %d inputs are provided`.
pub(crate) static TOO_MANY_DISPATCH_INPUTS: RuleMeta = events(
    "too-many-dispatch-inputs",
    "`workflow_dispatch` has more inputs than GitHub allows",
    "https://github.com/stokehold/workflint/blob/main/docs/rules/too-many-dispatch-inputs.md",
);

const fn events(id: &'static str, description: &'static str, docs_url: &'static str) -> RuleMeta {
    RuleMeta {
        id,
        description,
        kind: "events",
        category: Category::Correctness,
        default_severity: Severity::Error,
        docs_url,
    }
}

/// The `events` rule.
pub(crate) struct Events;

impl<'a> Rule<'a> for Events {
    fn kind(&self) -> &'static Kind {
        &kind::EVENTS
    }

    fn workflow_pre(&mut self, workflow: &'a Workflow<'a>, cx: &mut Context<'_>) {
        for event in workflow
            .on
            .value()
            .into_iter()
            .flat_map(|on| on.iter().flatten())
        {
            match event {
                Event::Webhook(event) => check_webhook(event, cx),
                Event::WorkflowDispatch(event) => check_workflow_dispatch(event, cx),
                Event::WorkflowCall(event) => check_workflow_call(event, cx),
                Event::Schedule(event) => check_schedule(event, cx),
                Event::RepositoryDispatch(_) | Event::ImageVersion(_) => {}
            }
        }
    }
}

/// The `schedule` case of the reference's `checkEvent`: each item's cron expression, then its
/// time zone.
fn check_schedule(event: &ScheduleEvent<'_>, cx: &mut Context<'_>) {
    for entry in event.schedules.iter() {
        if let Some(spec) = entry.cron.value() {
            check_cron(spec, cx);
        }
        if let Some(zone) = entry.timezone.value() {
            check_timezone(zone, cx);
        }
    }
}

/// The reference's `checkCron`.
fn check_cron(spec: &Template<'_>, cx: &mut Context<'_>) {
    let schedule = match cron::parse(spec.value()) {
        Ok(schedule) => schedule,
        Err(err) => {
            cx.report_in(
                spec.source,
                &INVALID_CRON,
                spec.span(),
                format!(
                    "invalid CRON format {} in schedule event: {err}",
                    quote(spec.value())
                ),
            );
            return;
        }
    };
    let interval = schedule.interval();
    if interval < i64::try_from(MIN_SCHEDULE_INTERVAL_SECONDS).unwrap_or(i64::MAX) {
        // The interval is a whole number of seconds far below 2^53, so the `f64` is exact.
        #[allow(clippy::cast_precision_loss)]
        let seconds = format_float(interval as f64);
        cx.report_in(
            spec.source,
            &CRON_TOO_FREQUENT,
            spec.span(),
            format!(
                "scheduled job runs too frequently. it runs once per {seconds} seconds. the shortest interval is {}",
                shortest_interval(MIN_SCHEDULE_INTERVAL_SECONDS),
            ),
        );
    }
}

/// Describes an interval of `seconds`, such as `once every 5 minutes`, in minutes when it is a
/// whole number of them.
fn shortest_interval(seconds: u64) -> String {
    let (count, unit) = if seconds.is_multiple_of(60) {
        (seconds / 60, "minute")
    } else {
        (seconds, "second")
    };
    let plural = if count == 1 { "" } else { "s" };
    format!("once every {count} {unit}{plural}")
}

/// The reference's `checkTimezone`. It rejects `UTC` and `Local` in any case before it looks
/// the name up, since Go's `time.LoadLocation` accepts them but they are not IANA names.
fn check_timezone(zone: &Template<'_>, cx: &mut Context<'_>) {
    let name = zone.value();
    let valid = !name.is_empty()
        && !name.eq_ignore_ascii_case("utc")
        && !name.eq_ignore_ascii_case("local")
        && is_time_zone(name);
    if !valid {
        cx.report_in(
            zone.source,
            &INVALID_TIMEZONE,
            zone.span(),
            format!(
                "invalid timezone {} in schedule event. it must be a valid IANA timezone name",
                quote(name)
            ),
        );
    }
}

/// Returns the items of a list of strings, with `None` for each item that is not a valid
/// string. An absent or invalid list has none.
fn items<'f, 'a>(field: &'f Field<'a, Arc<[Option<Template<'a>>]>>) -> &'f [Option<Template<'a>>] {
    field.value().map_or(&[], |items| items)
}

/// The reference's `checkWebhookEvent`.
fn check_webhook(event: &WebhookEvent<'_>, cx: &mut Context<'_>) {
    let hook = event.name.value();
    let Some(known) = webhook_event(hook) else {
        cx.report_in(
            event.name,
            &UNKNOWN_EVENT,
            event.name.span(),
            format!(
                "unknown Webhook event {}. see https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#webhook-events for list of all Webhook event names",
                quote(hook)
            ),
        );
        return;
    };

    check_types(event, known.activity_types, cx);

    let has_workflows = !items(&event.workflows).is_empty();
    if hook == "workflow_run" {
        if !has_workflows {
            cx.report_in(
                event.name,
                &MISSING_WORKFLOW_RUN_WORKFLOWS,
                event.name.span(),
                "no workflow is configured for \"workflow_run\" event",
            );
        }
    } else if has_workflows {
        cx.report_in(
            event.name,
            &WORKFLOWS_FILTER_NOT_SUPPORTED,
            event.name.span(),
            format!(
                "\"workflows\" cannot be configured for {} event. it is only for workflow_run event",
                quote(hook)
            ),
        );
    }

    let filters = [
        (EventFilter::Paths, &event.paths, &event.paths_ignore),
        (
            EventFilter::Branches,
            &event.branches,
            &event.branches_ignore,
        ),
        (EventFilter::Tags, &event.tags, &event.tags_ignore),
    ];
    for (filter, include, ignore) in filters {
        check_exclusive_filters(hook, filter, include, ignore, cx);
    }
}

/// The reference's `checkTypes`. `repository_dispatch`, the only event with any type, is not a
/// webhook event in the AST, so [`ActivityTypes::Any`] never gets here.
fn check_types(event: &WebhookEvent<'_>, expected: ActivityTypes, cx: &mut Context<'_>) {
    let ActivityTypes::Listed(expected) = expected else {
        return;
    };
    let types = items(&event.types);
    if expected.is_empty() {
        if !types.is_empty() {
            cx.report_in(
                event.name,
                &ACTIVITY_TYPES_NOT_SUPPORTED,
                event.name.span(),
                format!(
                    "\"types\" cannot be specified for {} Webhook event",
                    quote(event.name.value())
                ),
            );
        }
        return;
    }
    for ty in types.iter().flatten() {
        if !expected.contains(&ty.value()) {
            cx.report_in(
                ty.source,
                &INVALID_ACTIVITY_TYPE,
                ty.span(),
                format!(
                    "invalid activity type {} for {} Webhook event. available types are {}",
                    quote(ty.value()),
                    quote(event.name.value()),
                    sorted_quotes(expected)
                ),
            );
        }
    }
}

/// The reference's `checkExclusiveFilters` for one filter and its `-ignore` filter. A filter
/// with no items counts as absent, as the reference's `IsEmpty` does.
fn check_exclusive_filters(
    hook: &str,
    filter: EventFilter,
    include: &Field<'_, Arc<[Option<Template<'_>>]>>,
    ignore: &Field<'_, Arc<[Option<Template<'_>>]>>,
    cx: &mut Context<'_>,
) {
    let key = |field| {
        let field: &Field<'_, _> = field;
        field.key().filter(|_| !items(field).is_empty())
    };
    let (include, ignore) = (key(include), key(ignore));
    let available = filter_events(filter);
    if available.contains(&hook) {
        if let (Some(include), Some(ignore)) = (include, ignore) {
            let later = if include.span().start < ignore.span().start {
                ignore
            } else {
                include
            };
            cx.report_in(
                *later,
                &CONFLICTING_FILTERS,
                later.span(),
                format!(
                    "both {} and {} filters cannot be used for the same event {}. note: use '!' to negate patterns",
                    quote(include.value()),
                    quote(ignore.value()),
                    quote(hook)
                ),
            );
        }
        return;
    }
    let noun = if available.len() < 2 {
        "event"
    } else {
        "events"
    };
    for key in [include, ignore].into_iter().flatten() {
        cx.report_in(
            *key,
            &FILTER_NOT_SUPPORTED,
            key.span(),
            format!(
                "{} filter is not available for {hook} event. it is only for {} {noun}",
                quote(key.value()),
                available.join(", ")
            ),
        );
    }
}

/// Returns whether `value` is `true` or `false` in any case, as the reference compares the
/// value lowercased with `strings.ToLower`. No other character lowercases to a letter of these
/// words, so ASCII case folding gives the same result.
fn is_bool(value: &str) -> bool {
    value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("false")
}

/// The reference's `checkWorkflowCallEvent`. Inputs are checked in source order, and an
/// invalid default is skipped.
fn check_workflow_call(event: &WorkflowCallEvent<'_>, cx: &mut Context<'_>) {
    for input in event.inputs.value().into_iter().flatten() {
        let Some(default) = input.value.default.value() else {
            continue;
        };
        let name = input.key.value();
        // A default may contain `${{ }}`.
        if !contains_expression(default.value()) {
            match input.value.input_type.value() {
                Some(WorkflowCallInputType::Number) => {
                    if let Err(err) = parse_float(default.value()) {
                        cx.report_in(
                            default.source,
                            &INVALID_INPUT_DEFAULT,
                            default.span(),
                            format!(
                                "input of workflow_call event {} is typed as number but its default value {} cannot be parsed as a float number: {err}",
                                quote(name),
                                quote(default.value())
                            ),
                        );
                    }
                }
                Some(WorkflowCallInputType::Boolean) => {
                    if !is_bool(default.value()) {
                        cx.report_in(
                            default.source,
                            &INVALID_INPUT_DEFAULT,
                            default.span(),
                            format!(
                                "input of workflow_call event {} is typed as boolean. its default value must be true or false but got {}",
                                quote(name),
                                quote(default.value())
                            ),
                        );
                    }
                }
                Some(WorkflowCallInputType::String) | None => {}
            }
        }
        if input.value.is_required() {
            cx.report_in(
                default.source,
                &REQUIRED_INPUT_WITH_DEFAULT,
                default.span(),
                format!(
                    "input {} of workflow_call event has the default value {}, but it is also required. if an input is marked as required, its default value will never be used",
                    quote(name),
                    quote(default.value())
                ),
            );
        }
    }
}

/// The reference's `checkWorkflowDispatchEvent`. Inputs are checked in source order.
///
/// Most messages name an input by its folded key, the key of the reference's Go map. The
/// message for a `number` default names it as written, as the reference does.
fn check_workflow_dispatch(event: &WorkflowDispatchEvent<'_>, cx: &mut Context<'_>) {
    let Some(inputs) = event.inputs.value() else {
        return;
    };
    for input in inputs {
        if input.value.input_type.value() == Some(&WorkflowDispatchInputType::Choice) {
            check_choice_input(input, cx);
        } else {
            check_non_choice_input(input, cx);
        }
    }
    if inputs.len() > MAX_WORKFLOW_DISPATCH_INPUTS {
        cx.report_in(
            event.name,
            &TOO_MANY_DISPATCH_INPUTS,
            event.name.span(),
            format!(
                "maximum number of inputs for \"workflow_dispatch\" event is {MAX_WORKFLOW_DISPATCH_INPUTS} but {} inputs are provided. see https://docs.github.com/en/actions/using-workflows/events-that-trigger-workflows#providing-inputs",
                inputs.len()
            ),
        );
    }
}

/// Checks the options and the default of a `choice` input of `workflow_dispatch`.
fn check_choice_input(input: &KeyedEntry<'_, WorkflowDispatchInput<'_>>, cx: &mut Context<'_>) {
    let folded = input.folded();
    let options = items(&input.value.options);
    if options.is_empty() {
        cx.report_in(
            input.key,
            &CHOICE_INPUT_WITHOUT_OPTIONS,
            input.key.span(),
            format!(
                "input type of {} is \"choice\" but \"options\" is not set",
                quote(folded)
            ),
        );
        return;
    }
    let mut seen = BTreeSet::new();
    for option in options.iter().flatten() {
        if !seen.insert(option.value()) {
            cx.report_in(
                option.source,
                &DUPLICATE_INPUT_OPTION,
                option.span(),
                format!(
                    "option {} is duplicated in options of {} input",
                    quote(option.value()),
                    quote(folded)
                ),
            );
        }
    }
    // An invalid option has no value to compare the default with.
    let all_valid = options.iter().all(Option::is_some);
    if let Some(default) = input.value.default.value()
        && all_valid
        && !seen.contains(default.value())
    {
        let options = quotes(options.iter().flatten().map(Template::value));
        cx.report_in(
            default.source,
            &DEFAULT_NOT_IN_OPTIONS,
            default.span(),
            format!(
                "default value {} of {} input is not included in its options {options}",
                quote(default.value()),
                quote(folded)
            ),
        );
    }
}

/// Checks the options and the default of a `workflow_dispatch` input that is not a `choice`.
fn check_non_choice_input(input: &KeyedEntry<'_, WorkflowDispatchInput<'_>>, cx: &mut Context<'_>) {
    let folded = input.folded();
    if !items(&input.value.options).is_empty() {
        cx.report_in(
            input.key,
            &OPTIONS_ON_NON_CHOICE_INPUT,
            input.key.span(),
            format!(
                "\"options\" can not be set to {} input because its input type is not \"choice\"",
                quote(folded)
            ),
        );
    }
    let Some(default) = input.value.default.value() else {
        return;
    };
    match input.value.input_type.value() {
        Some(WorkflowDispatchInputType::Number) => {
            if let Err(err) = parse_float(default.value()) {
                cx.report_in(
                    default.source,
                    &INVALID_INPUT_DEFAULT,
                    default.span(),
                    format!(
                        "type of {} input is \"number\" but its default value {} cannot be parsed as a float number: {err}",
                        quote(input.key.value()),
                        quote(default.value())
                    ),
                );
            }
        }
        Some(WorkflowDispatchInputType::Boolean) => {
            if !is_bool(default.value()) {
                cx.report_in(
                    default.source,
                    &INVALID_INPUT_DEFAULT,
                    default.span(),
                    format!(
                        "type of {} input is \"boolean\". its default value {} must be \"true\" or \"false\"",
                        quote(folded),
                        quote(default.value())
                    ),
                );
            }
        }
        Some(
            WorkflowDispatchInputType::String
            | WorkflowDispatchInputType::Choice
            | WorkflowDispatchInputType::Environment,
        )
        | None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::shortest_interval;
    use workflint_data::MIN_SCHEDULE_INTERVAL_SECONDS;

    #[test]
    fn the_shortest_interval_comes_from_the_data() {
        assert_eq!(
            shortest_interval(MIN_SCHEDULE_INTERVAL_SECONDS),
            "once every 5 minutes"
        );
    }

    #[test]
    fn the_shortest_interval_is_in_minutes_when_it_is_a_whole_number_of_them() {
        assert_eq!(shortest_interval(60), "once every 1 minute");
        assert_eq!(shortest_interval(600), "once every 10 minutes");
        assert_eq!(shortest_interval(90), "once every 90 seconds");
        assert_eq!(shortest_interval(1), "once every 1 second");
    }
}
