//! Checks the availability table against the reference's `WorkflowKeyAvailability`,
//! `SpecialFunctionNames`, and `AllContexts` in the vendored `availability.go`.

#![allow(clippy::panic)]

mod go_source;

use std::collections::BTreeMap;

use go_source::{Availability, all_contexts, special_function_names, workflow_key_availability};
use workflint_data::{
    Availability as Allowed, SPECIAL_FUNCTIONS, WORKFLOW_KEYS, special_function_keys, workflow_key,
};

fn owned(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn workflow_keys_match_the_reference_table() {
    let workflint: BTreeMap<String, Availability> = WORKFLOW_KEYS
        .iter()
        .map(|key| {
            let availability = Availability {
                contexts: owned(key.availability.contexts),
                special_functions: owned(key.availability.special_functions),
            };
            (key.name.to_owned(), availability)
        })
        .collect();

    assert_eq!(workflint, workflow_key_availability());
}

#[test]
fn special_function_keys_match_the_reference_special_function_names() {
    let workflint: BTreeMap<String, Vec<String>> = SPECIAL_FUNCTIONS
        .iter()
        .map(|name| {
            let Some(keys) = special_function_keys(name) else {
                panic!("{name} has no workflow keys");
            };
            ((*name).to_owned(), keys.map(str::to_owned).collect())
        })
        .collect();

    assert_eq!(workflint, special_function_names());
}

#[test]
fn contexts_of_the_workflow_keys_match_the_reference_all_contexts() {
    let mut workflint: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for key in WORKFLOW_KEYS {
        for context in key.availability.contexts {
            workflint
                .entry((*context).to_owned())
                .or_default()
                .push(key.name.to_owned());
        }
    }

    assert_eq!(workflint, all_contexts());
}

#[test]
fn all_allows_every_context_of_the_reference_and_every_special_function() {
    let contexts: Vec<String> = all_contexts().into_keys().collect();
    assert_eq!(owned(Allowed::ALL.contexts), contexts);
    assert_eq!(Allowed::ALL.special_functions, SPECIAL_FUNCTIONS);
}

#[test]
fn default_allows_nothing() {
    assert_eq!(Allowed::default(), Allowed::NONE);
}

#[test]
fn workflow_keys_are_sorted_and_unique() {
    let names: Vec<&str> = WORKFLOW_KEYS.iter().map(|key| key.name).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(names, sorted);
}

#[test]
fn workflow_key_finds_every_key() {
    for key in WORKFLOW_KEYS {
        assert_eq!(
            workflow_key(key.name),
            Some(&key.availability),
            "{}",
            key.name
        );
    }
}

#[test]
fn workflow_key_does_not_find_an_unknown_or_empty_key() {
    assert_eq!(workflow_key(""), None);
    assert_eq!(workflow_key("jobs.<job_id>.steps.shell"), None);
    assert_eq!(workflow_key("JOBS.<JOB_ID>.IF"), None);
}

#[test]
fn special_function_keys_is_none_for_a_function_that_is_not_special() {
    assert!(special_function_keys("contains").is_none());
    assert!(special_function_keys("hashFiles").is_none());
}

#[test]
fn special_functions_are_sorted_lower_case_names() {
    let mut sorted = SPECIAL_FUNCTIONS.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(SPECIAL_FUNCTIONS, sorted);
    assert!(
        SPECIAL_FUNCTIONS
            .iter()
            .all(|name| !name.bytes().any(|b| b.is_ascii_uppercase()))
    );
}
