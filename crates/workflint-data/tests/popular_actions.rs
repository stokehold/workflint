//! Checks the popular actions and the outdated action specs against the reference's
//! `PopularActions` and `OutdatedPopularActionSpecs` in the vendored `popular_actions.go`.

#![allow(clippy::panic)]

mod go_source;

use std::fs;
use std::path::Path;

use go_source::{GoActionInput, GoPopularAction, outdated_popular_action_specs, popular_actions};
use workflint_data::{
    OUTDATED_POPULAR_ACTION_SPECS, POPULAR_ACTIONS, PopularAction, is_outdated_popular_action,
    popular_action,
};

/// `action` in the shape that the reader of the Go source returns.
fn as_go(action: &PopularAction) -> GoPopularAction {
    GoPopularAction {
        spec: action.spec.to_owned(),
        name: action.name.to_owned(),
        inputs: action
            .inputs
            .iter()
            .map(|input| GoActionInput {
                id: input.id.to_owned(),
                name: input.name.to_owned(),
                required: input.required,
                deprecated: input.deprecated,
                deprecation_message: input.deprecation_message.to_owned(),
            })
            .collect(),
        outputs: action
            .outputs
            .iter()
            .map(|output| (output.id.to_owned(), output.name.to_owned()))
            .collect(),
        skip_inputs: action.skip_inputs,
        skip_outputs: action.skip_outputs,
    }
}

fn lookup(spec: &str) -> &'static PopularAction {
    popular_action(spec).unwrap_or_else(|| panic!("{spec} is not a popular action"))
}

#[test]
fn popular_actions_match_the_reference_table() {
    let reference = popular_actions();
    assert_eq!(reference.len(), 218);
    let ours: Vec<GoPopularAction> = POPULAR_ACTIONS.iter().map(as_go).collect();
    assert_eq!(ours.len(), reference.len());
    for (ours, reference) in ours.iter().zip(&reference) {
        assert_eq!(ours, reference);
    }
}

#[test]
fn outdated_specs_match_the_reference_table() {
    let reference = outdated_popular_action_specs();
    assert_eq!(reference.len(), 156);
    assert_eq!(OUTDATED_POPULAR_ACTION_SPECS, reference);
}

#[test]
fn tables_are_sorted_for_binary_search() {
    assert!(POPULAR_ACTIONS.windows(2).all(|w| w[0].spec < w[1].spec));
    assert!(
        OUTDATED_POPULAR_ACTION_SPECS
            .windows(2)
            .all(|w| w[0] < w[1])
    );
    for action in POPULAR_ACTIONS {
        let spec = action.spec;
        assert!(
            action.inputs.windows(2).all(|w| w[0].id < w[1].id),
            "{spec}"
        );
        assert!(
            action.outputs.windows(2).all(|w| w[0].id < w[1].id),
            "{spec}"
        );
    }
}

#[test]
fn input_and_output_ids_are_the_lowercased_names() {
    for action in POPULAR_ACTIONS {
        for input in action.inputs {
            assert_eq!(input.id, input.name.to_lowercase(), "{}", action.spec);
        }
        for output in action.outputs {
            assert_eq!(output.id, output.name.to_lowercase(), "{}", action.spec);
        }
    }
}

#[test]
fn lookup_matches_the_exact_spec() {
    assert_eq!(lookup("actions/checkout@v4").name, "Checkout");
    assert_eq!(popular_action("Actions/checkout@v4"), None);
    assert_eq!(popular_action("actions/checkout@V4"), None);
    assert_eq!(popular_action("actions/checkout@v4.1.0"), None);
    assert_eq!(popular_action("actions/checkout"), None);

    assert!(is_outdated_popular_action("actions/checkout@v3"));
    assert!(!is_outdated_popular_action("Actions/checkout@v3"));
    assert!(!is_outdated_popular_action("actions/checkout@v4"));
}

#[test]
fn inputs_and_outputs_are_found_by_lowercased_id() {
    let lighthouse = lookup("treosh/lighthouse-ci-action@v12");
    let Some(input) = lighthouse.input("configpath") else {
        panic!("configpath is not an input of {}", lighthouse.spec);
    };
    assert_eq!(input.name, "configPath");
    assert_eq!(lighthouse.input("configPath"), None);
    let Some(output) = lighthouse.output("resultspath") else {
        panic!("resultspath is not an output of {}", lighthouse.spec);
    };
    assert_eq!(output.name, "resultsPath");
}

#[test]
fn deprecated_inputs_keep_their_message() {
    let Some(save_always) = lookup("actions/cache@v4").input("save-always") else {
        panic!("save-always is not an input of actions/cache@v4");
    };
    assert!(save_always.deprecated);
    assert!(!save_always.required);
    assert!(
        save_always
            .deprecation_message
            .starts_with("save-always does not work as intended")
    );
}

#[test]
fn skip_flags_are_set_where_the_reference_sets_them() {
    assert!(lookup("octokit/request-action@v2.x").skip_inputs);
    assert!(lookup("dorny/paths-filter@v3").skip_outputs);
    let checkout = lookup("actions/checkout@v4");
    assert!(!checkout.skip_inputs && !checkout.skip_outputs);
}

/// rhysd/actionlint#537.
#[test]
fn download_artifact_v4_has_the_artifact_ids_input() {
    let download = lookup("actions/download-artifact@v4");
    assert!(download.input("artifact-ids").is_some());
}

/// rhysd/actionlint#416.
#[test]
fn paths_filter_v3_has_the_predicate_quantifier_input() {
    let filter = lookup("dorny/paths-filter@v3");
    assert!(filter.input("predicate-quantifier").is_some());
}

/// rhysd/actionlint#255.
#[test]
fn download_artifact_has_the_download_path_output() {
    for spec in [
        "actions/download-artifact@v4",
        "actions/download-artifact@v5",
    ] {
        assert!(lookup(spec).output("download-path").is_some(), "{spec}");
    }
}

/// Lookup allocates nothing: the crate is `no_std` and never links `alloc`, so no code in it
/// can allocate.
#[test]
fn crate_cannot_allocate() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut pending = vec![src];
    let mut files = 0;
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
            let path = entry
                .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
                .path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let source =
                fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert!(
                !source.contains("extern crate alloc"),
                "{} links alloc",
                path.display()
            );
            if path.ends_with("lib.rs") {
                assert!(source.contains("#![no_std]"), "lib.rs is not no_std");
            }
            files += 1;
        }
    }
    assert!(files > 0);
}
