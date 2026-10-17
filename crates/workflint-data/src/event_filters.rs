//! The events that each branch, tag, and path filter of `on:` is available for.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: the calls of `checkExclusiveFilters` in
//! `checkWebhookEvent` in `rule_events.go`, lines 158-175. Until M7 replaces them with a
//! generated file (ADR 0011), keep them equal to that code. The test
//! `filter_events_match_the_reference_checks` compares them.

/// A pair of filters of a webhook event, such as `paths` and `paths-ignore`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventFilter {
    /// `paths` and `paths-ignore`.
    Paths,
    /// `branches` and `branches-ignore`.
    Branches,
    /// `tags` and `tags-ignore`.
    Tags,
}

/// Returns the events that may have `filter`, in the order that the reference's message lists
/// them.
#[must_use]
pub const fn filter_events(filter: EventFilter) -> &'static [&'static str] {
    match filter {
        EventFilter::Paths => &["push", "pull_request", "pull_request_target"],
        EventFilter::Branches => &[
            "merge_group",
            "push",
            "pull_request",
            "pull_request_target",
            "workflow_run",
        ],
        EventFilter::Tags => &["push"],
    }
}
