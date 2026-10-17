//! Checks the permission scopes against the reference's `allPermissionScopes` and
//! `checkPermissions` in the vendored `rule_permissions.go`.

#![allow(clippy::panic)]

mod go_source;

use go_source::{all_permission_scopes, permissions_for_all_scopes};
use workflint_data::{PERMISSION_SCOPES, PERMISSIONS_FOR_ALL_SCOPES, permission_scope};

#[test]
fn permission_scopes_match_the_reference_table() {
    let ours: Vec<(String, Vec<String>)> = PERMISSION_SCOPES
        .iter()
        .map(|scope| {
            let levels = scope
                .levels
                .iter()
                .map(|level| (*level).to_owned())
                .collect();
            (scope.name.to_owned(), levels)
        })
        .collect();
    let mut reference = all_permission_scopes();
    reference.sort_by(|(a, _), (b, _)| a.cmp(b));
    assert_eq!(ours, reference);
}

#[test]
fn permissions_for_all_scopes_match_the_reference_check() {
    assert_eq!(PERMISSIONS_FOR_ALL_SCOPES, permissions_for_all_scopes());
}

#[test]
fn permission_scopes_are_sorted_by_name() {
    assert!(PERMISSION_SCOPES.windows(2).all(|w| w[0].name < w[1].name));
}

#[test]
fn lookup_is_case_sensitive() {
    let Some(id_token) = permission_scope("id-token") else {
        panic!("id-token is not a permission scope");
    };
    assert_eq!(id_token.levels, ["write", "none"]);
    assert_eq!(permission_scope("Contents"), None);
}
