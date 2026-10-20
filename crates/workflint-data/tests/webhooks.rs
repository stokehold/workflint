//! Checks the webhook events against the reference's `AllWebhookTypes` in the vendored
//! `all_webhooks.go`, plus the activity types that workflint adds.

#![allow(clippy::panic)]

mod go_source;

use go_source::all_webhook_types;
use workflint_data::{ActivityTypes, WEBHOOK_EVENTS, webhook_event};

/// The activity types that workflint adds after the reference's types of an event
/// (`docs/differences.md#platform-data-updates`).
const ADDED_ACTIVITY_TYPES: [(&str, &[&str]); 1] = [("issues", &["field_added", "field_removed"])];

#[test]
fn webhook_events_are_the_reference_table_and_the_added_activity_types() {
    let ours: Vec<(String, Option<Vec<String>>)> = WEBHOOK_EVENTS
        .iter()
        .map(|event| {
            let types = match event.activity_types {
                ActivityTypes::Listed(types) => {
                    Some(types.iter().map(|t| (*t).to_owned()).collect())
                }
                ActivityTypes::Any => None,
            };
            (event.name.to_owned(), types)
        })
        .collect();
    let mut expected = all_webhook_types();
    for (name, added) in ADDED_ACTIVITY_TYPES {
        let Some((_, Some(types))) = expected.iter_mut().find(|(event, _)| event == name) else {
            panic!("{name} has no listed activity types in the reference");
        };
        types.extend(added.iter().map(|t| (*t).to_owned()));
    }
    expected.sort_by(|(a, _), (b, _)| a.cmp(b));
    assert_eq!(ours, expected);
}

#[test]
fn webhook_events_are_sorted_by_name() {
    assert!(WEBHOOK_EVENTS.windows(2).all(|w| w[0].name < w[1].name));
}

#[test]
fn lookup_is_case_sensitive() {
    let Some(push) = webhook_event("push") else {
        panic!("push is not a webhook event");
    };
    assert_eq!(push.activity_types, ActivityTypes::Listed(&[]));
    assert_eq!(webhook_event("Push"), None);
    assert_eq!(webhook_event("unknown"), None);
}

#[test]
fn repository_dispatch_takes_any_activity_type() {
    let event = webhook_event("repository_dispatch").map(|event| event.activity_types);
    assert_eq!(event, Some(ActivityTypes::Any));
}
