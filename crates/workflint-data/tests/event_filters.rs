//! Checks the filter availability and the limits of events against the reference's
//! `checkWebhookEvent`, `checkWorkflowDispatchEvent`, and `checkCron` in the vendored
//! `rule_events.go`.

mod go_source;

use go_source::{exclusive_filters, max_workflow_dispatch_inputs, min_schedule_interval_seconds};
use workflint_data::{
    EventFilter, MAX_WORKFLOW_DISPATCH_INPUTS, MIN_SCHEDULE_INTERVAL_SECONDS, filter_events,
};

#[test]
fn filter_events_match_the_reference_checks() {
    let ours = [
        ("Paths", EventFilter::Paths),
        ("Branches", EventFilter::Branches),
        ("Tags", EventFilter::Tags),
    ]
    .map(|(go, filter)| {
        let events = filter_events(filter)
            .iter()
            .map(|event| (*event).to_owned())
            .collect();
        (go.to_owned(), events)
    })
    .to_vec();
    assert_eq!(ours, exclusive_filters());
}

#[test]
fn workflow_dispatch_input_limit_matches_the_reference_check() {
    assert_eq!(MAX_WORKFLOW_DISPATCH_INPUTS, max_workflow_dispatch_inputs());
}

#[test]
fn schedule_interval_matches_the_reference_check() {
    assert_eq!(
        MIN_SCHEDULE_INTERVAL_SECONDS,
        min_schedule_interval_seconds()
    );
}
