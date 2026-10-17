//! Checks the webhook events against the reference's `AllWebhookTypes` in the vendored
//! `all_webhooks.go`.

#![allow(clippy::panic)]

mod go_source;

use go_source::all_webhook_types;
use workflint_data::{ActivityTypes, WEBHOOK_EVENTS, webhook_event};

#[test]
fn webhook_events_match_the_reference_table() {
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
    let mut reference = all_webhook_types();
    reference.sort_by(|(a, _), (b, _)| a.cmp(b));
    assert_eq!(ours, reference);
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
