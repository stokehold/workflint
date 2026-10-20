//! The webhook events that can trigger a workflow, and their activity types.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `AllWebhookTypes` in `all_webhooks.go`,
//! lines 10-44, which a script generates from
//! <https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows>.
//! Until M7 replaces it with a generated file (ADR 0011), keep it equal to that table plus the
//! `field_added` and `field_removed` types of `issues`, which workflint adds from that page,
//! checked on 2026-10-20 (ADR 0033, `docs/differences.md#platform-data-updates`). The test
//! `webhook_events_are_the_reference_table_and_the_added_activity_types` compares them.

/// The activity types that `on.<event>.types` may list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityTypes {
    /// Only these types, in the order of GitHub's documentation. An empty list means the event
    /// takes no `types` at all.
    Listed(&'static [&'static str]),
    /// Any type, because the workflow that sends the event chooses it, as with
    /// `repository_dispatch`. The reference writes this as `nil`.
    Any,
}

/// A webhook event that can trigger a workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebhookEvent {
    /// The name in `on:`, such as `pull_request`.
    pub name: &'static str,
    /// The activity types that `types` may list.
    pub activity_types: ActivityTypes,
}

impl WebhookEvent {
    const fn new(name: &'static str, activity_types: ActivityTypes) -> Self {
        Self {
            name,
            activity_types,
        }
    }
}

/// Every webhook event, sorted by name.
pub static WEBHOOK_EVENTS: &[WebhookEvent] = &[
    WebhookEvent::new(
        "branch_protection_rule",
        ActivityTypes::Listed(&["created", "edited", "deleted"]),
    ),
    WebhookEvent::new(
        "check_run",
        ActivityTypes::Listed(&["created", "rerequested", "completed", "requested_action"]),
    ),
    WebhookEvent::new("check_suite", ActivityTypes::Listed(&["completed"])),
    WebhookEvent::new("create", ActivityTypes::Listed(&[])),
    WebhookEvent::new("delete", ActivityTypes::Listed(&[])),
    WebhookEvent::new("deployment", ActivityTypes::Listed(&[])),
    WebhookEvent::new("deployment_status", ActivityTypes::Listed(&[])),
    WebhookEvent::new(
        "discussion",
        ActivityTypes::Listed(&[
            "created",
            "edited",
            "deleted",
            "transferred",
            "pinned",
            "unpinned",
            "labeled",
            "unlabeled",
            "locked",
            "unlocked",
            "category_changed",
            "answered",
            "unanswered",
        ]),
    ),
    WebhookEvent::new(
        "discussion_comment",
        ActivityTypes::Listed(&["created", "edited", "deleted"]),
    ),
    WebhookEvent::new("fork", ActivityTypes::Listed(&[])),
    WebhookEvent::new("gollum", ActivityTypes::Listed(&[])),
    WebhookEvent::new("image_version", ActivityTypes::Listed(&[])),
    WebhookEvent::new(
        "issue_comment",
        ActivityTypes::Listed(&["created", "edited", "deleted"]),
    ),
    WebhookEvent::new(
        "issues",
        ActivityTypes::Listed(&[
            "opened",
            "edited",
            "deleted",
            "transferred",
            "pinned",
            "unpinned",
            "closed",
            "reopened",
            "assigned",
            "unassigned",
            "labeled",
            "unlabeled",
            "locked",
            "unlocked",
            "milestoned",
            "demilestoned",
            "typed",
            "untyped",
            "field_added",
            "field_removed",
        ]),
    ),
    WebhookEvent::new(
        "label",
        ActivityTypes::Listed(&["created", "edited", "deleted"]),
    ),
    WebhookEvent::new("merge_group", ActivityTypes::Listed(&["checks_requested"])),
    WebhookEvent::new(
        "milestone",
        ActivityTypes::Listed(&["created", "closed", "opened", "edited", "deleted"]),
    ),
    WebhookEvent::new("page_build", ActivityTypes::Listed(&[])),
    WebhookEvent::new("public", ActivityTypes::Listed(&[])),
    WebhookEvent::new(
        "pull_request",
        ActivityTypes::Listed(&[
            "assigned",
            "unassigned",
            "labeled",
            "unlabeled",
            "opened",
            "edited",
            "closed",
            "reopened",
            "synchronize",
            "converted_to_draft",
            "locked",
            "unlocked",
            "enqueued",
            "dequeued",
            "milestoned",
            "demilestoned",
            "ready_for_review",
            "review_requested",
            "review_request_removed",
            "auto_merge_enabled",
            "auto_merge_disabled",
        ]),
    ),
    WebhookEvent::new(
        "pull_request_review",
        ActivityTypes::Listed(&["submitted", "edited", "dismissed"]),
    ),
    WebhookEvent::new(
        "pull_request_review_comment",
        ActivityTypes::Listed(&["created", "edited", "deleted"]),
    ),
    WebhookEvent::new(
        "pull_request_target",
        ActivityTypes::Listed(&[
            "assigned",
            "unassigned",
            "labeled",
            "unlabeled",
            "opened",
            "edited",
            "closed",
            "reopened",
            "synchronize",
            "converted_to_draft",
            "locked",
            "unlocked",
            "enqueued",
            "dequeued",
            "milestoned",
            "demilestoned",
            "ready_for_review",
            "review_requested",
            "review_request_removed",
            "auto_merge_enabled",
            "auto_merge_disabled",
        ]),
    ),
    WebhookEvent::new("push", ActivityTypes::Listed(&[])),
    WebhookEvent::new(
        "registry_package",
        ActivityTypes::Listed(&["published", "updated"]),
    ),
    WebhookEvent::new(
        "release",
        ActivityTypes::Listed(&[
            "published",
            "unpublished",
            "created",
            "edited",
            "deleted",
            "prereleased",
            "released",
        ]),
    ),
    WebhookEvent::new("repository_dispatch", ActivityTypes::Any),
    WebhookEvent::new("schedule", ActivityTypes::Listed(&[])),
    WebhookEvent::new("status", ActivityTypes::Listed(&[])),
    WebhookEvent::new("watch", ActivityTypes::Listed(&["started"])),
    WebhookEvent::new("workflow_call", ActivityTypes::Listed(&[])),
    WebhookEvent::new("workflow_dispatch", ActivityTypes::Listed(&[])),
    WebhookEvent::new(
        "workflow_run",
        ActivityTypes::Listed(&["completed", "requested", "in_progress"]),
    ),
];

/// Returns the webhook event called `name`. Event names are case-sensitive.
#[must_use]
pub fn webhook_event(name: &str) -> Option<&'static WebhookEvent> {
    WEBHOOK_EVENTS
        .binary_search_by(|event| event.name.cmp(name))
        .ok()
        .and_then(|index| WEBHOOK_EVENTS.get(index))
}
