//! The limits that GitHub sets on `workflow_dispatch` inputs and on schedules.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `checkWorkflowDispatchEvent` in
//! `rule_events.go`, line 300, and `checkCron`, line 77. Until M7 replaces them with a generated
//! file (ADR 0011), keep them equal to that code. The tests
//! `workflow_dispatch_input_limit_matches_the_reference_check` and
//! `schedule_interval_matches_the_reference_check` compare them.

/// The most inputs that a `workflow_dispatch` event may have.
///
/// <https://github.blog/changelog/2025-12-04-actions-workflow-dispatch-workflows-now-support-25-inputs>
pub const MAX_WORKFLOW_DISPATCH_INPUTS: usize = 25;

/// The shortest interval between two runs of a schedule, in seconds.
///
/// <https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#scheduled-events>
pub const MIN_SCHEDULE_INTERVAL_SECONDS: u64 = 5 * 60;
