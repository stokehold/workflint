//! Checks the global context types against the reference's `BuiltinGlobalVariableTypes` in
//! the vendored `expr_sema.go`, and against the rules of
//! `TestBuiltinGlobalVariableTypesValidation` in `expr_sema_test.go`.

#![allow(clippy::panic)]

mod go_source;

use std::collections::BTreeMap;

use go_source::{Ty, builtin_global_variable_types, update_secrets_built_ins};
use workflint_data::{
    BUILT_IN_SECRETS, GLOBAL_CONTEXTS, ObjectDesc, Rest, TypeDesc, global_context,
};

fn workflint_contexts() -> BTreeMap<String, Ty> {
    GLOBAL_CONTEXTS
        .iter()
        .map(|context| (context.name.to_owned(), Ty::from(&context.ty)))
        .collect()
}

#[test]
fn global_contexts_match_the_reference_table() {
    let reference = builtin_global_variable_types();
    let workflint = workflint_contexts();

    assert_eq!(
        workflint.keys().collect::<Vec<_>>(),
        reference.keys().collect::<Vec<_>>(),
        "context names"
    );
    for (name, ty) in &reference {
        assert_eq!(workflint.get(name), Some(ty), "the type of {name}");
    }
}

#[test]
fn built_in_secrets_match_the_reference_update_secrets() {
    let workflint: BTreeMap<String, Ty> = BUILT_IN_SECRETS
        .iter()
        .map(|name| ((*name).to_owned(), Ty::String))
        .collect();
    assert_eq!(workflint, update_secrets_built_ins());
}

#[test]
fn built_in_secrets_are_sorted_lower_case_names() {
    let mut sorted = BUILT_IN_SECRETS.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(BUILT_IN_SECRETS, sorted);
    assert!(
        BUILT_IN_SECRETS
            .iter()
            .all(|name| !name.bytes().any(|b| b.is_ascii_uppercase()))
    );
}

#[test]
fn global_context_finds_every_context_by_name() {
    for context in GLOBAL_CONTEXTS {
        assert_eq!(
            global_context(context.name),
            Some(&context.ty),
            "{}",
            context.name
        );
    }
}

#[test]
fn jobs_is_not_a_global_context() {
    assert_eq!(global_context("jobs"), None);
}

#[test]
fn global_context_does_not_find_a_name_in_upper_case() {
    assert_eq!(global_context("GITHUB"), None);
}

/// Every name in `ty`, with the path that leads to it.
fn names(ty: &TypeDesc, path: &str, out: &mut Vec<String>) {
    match ty {
        TypeDesc::Array(elem) => names(elem, &format!("{path}.*"), out),
        TypeDesc::Object(object) => {
            for prop in object.props {
                let path = format!("{path}.{}", prop.name);
                out.push(path.clone());
                names(&prop.ty, &path, out);
            }
            if let Rest::Map(elem) = object.rest {
                names(elem, &format!("{path}.<key>"), out);
            }
        }
        TypeDesc::Any | TypeDesc::Null | TypeDesc::Number | TypeDesc::Bool | TypeDesc::String => {}
    }
}

// The reference compares context and property names without regard to case, so its test
// requires lower-case names. This test also looks inside map elements, which the reference's
// test skips.
#[test]
fn context_and_property_names_have_no_upper_case_letters() {
    let mut all = Vec::new();
    for context in GLOBAL_CONTEXTS {
        all.push(context.name.to_owned());
        names(&context.ty, context.name, &mut all);
    }

    let upper: Vec<&String> = all
        .iter()
        .filter(|path| path.bytes().any(|b| b.is_ascii_uppercase()))
        .collect();
    assert!(upper.is_empty(), "names with upper-case letters: {upper:?}");
}

fn assert_sorted_without_duplicates(object: &ObjectDesc, path: &str) {
    for pair in object.props.windows(2) {
        if let [a, b] = pair {
            assert!(
                a.name < b.name,
                "{path}: {:?} comes before {:?}",
                a.name,
                b.name
            );
        }
    }
    for prop in object.props {
        assert_objects_sorted(&prop.ty, &format!("{path}.{}", prop.name));
    }
    if let Rest::Map(elem) = object.rest {
        assert_objects_sorted(elem, &format!("{path}.<key>"));
    }
}

fn assert_objects_sorted(ty: &TypeDesc, path: &str) {
    match ty {
        TypeDesc::Array(elem) => assert_objects_sorted(elem, &format!("{path}.*")),
        TypeDesc::Object(object) => assert_sorted_without_duplicates(object, path),
        TypeDesc::Any | TypeDesc::Null | TypeDesc::Number | TypeDesc::Bool | TypeDesc::String => {}
    }
}

// `global_context` and `ObjectDesc::prop` search by binary search.
#[test]
fn contexts_and_properties_are_sorted_by_name_without_duplicates() {
    for pair in GLOBAL_CONTEXTS.windows(2) {
        if let [a, b] = pair {
            assert!(a.name < b.name, "{:?} comes before {:?}", a.name, b.name);
        }
    }
    for context in GLOBAL_CONTEXTS {
        assert_objects_sorted(&context.ty, context.name);
    }
}

#[test]
fn prop_finds_a_known_property_and_not_an_unknown_one() {
    let Some(TypeDesc::Object(job)) = global_context("job") else {
        panic!("job is not an object");
    };

    assert!(matches!(job.prop("container"), Some(TypeDesc::Object(_))));
    assert_eq!(job.prop("status"), Some(&TypeDesc::String));
    assert_eq!(job.prop("steps"), None);
}

#[test]
fn crate_depends_on_no_workspace_crate() {
    let manifest = include_str!("../Cargo.toml");

    let workspace_deps: Vec<&str> = manifest
        .lines()
        .filter(|line| line.trim_start().starts_with("workflint"))
        .collect();
    assert!(workspace_deps.is_empty(), "{workspace_deps:?}");
}
