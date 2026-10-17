//! The scopes of `permissions:` and the levels that each one accepts.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `allPermissionScopes` in
//! `rule_permissions.go`, lines 5-22, and the values that `checkPermissions` accepts for all
//! the scopes at once, line 58. Until M7 replaces them with generated files (ADR 0011), keep
//! them equal to that code. The tests `permission_scopes_match_the_reference_table` and
//! `permissions_for_all_scopes_match_the_reference_check` compare them.

/// A permission scope of the `GITHUB_TOKEN`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PermissionScope {
    /// The key in `permissions:`, such as `contents`.
    pub name: &'static str,
    /// The levels that the scope accepts, in the reference's order.
    pub levels: &'static [&'static str],
}

const fn scope(name: &'static str, levels: &'static [&'static str]) -> PermissionScope {
    PermissionScope { name, levels }
}

const READ_WRITE_NONE: &[&str] = &["read", "write", "none"];

/// Every permission scope, sorted by name.
pub static PERMISSION_SCOPES: &[PermissionScope] = &[
    scope("actions", READ_WRITE_NONE),
    scope("artifact-metadata", READ_WRITE_NONE),
    scope("attestations", READ_WRITE_NONE),
    scope("checks", READ_WRITE_NONE),
    scope("contents", READ_WRITE_NONE),
    scope("deployments", READ_WRITE_NONE),
    scope("discussions", READ_WRITE_NONE),
    scope("id-token", &["write", "none"]),
    scope("issues", READ_WRITE_NONE),
    scope("models", &["read", "none"]),
    scope("packages", READ_WRITE_NONE),
    scope("pages", READ_WRITE_NONE),
    scope("pull-requests", READ_WRITE_NONE),
    scope("repository-projects", READ_WRITE_NONE),
    scope("security-events", READ_WRITE_NONE),
    scope("statuses", READ_WRITE_NONE),
];

/// The values that set every scope at once, as in `permissions: read-all`. An empty mapping,
/// `permissions: {}`, is the third way.
pub static PERMISSIONS_FOR_ALL_SCOPES: &[&str] = &["write-all", "read-all"];

/// Returns the permission scope called `name`. Scope names are case-sensitive.
#[must_use]
pub fn permission_scope(name: &str) -> Option<&'static PermissionScope> {
    PERMISSION_SCOPES
        .binary_search_by(|scope| scope.name.cmp(name))
        .ok()
        .and_then(|index| PERMISSION_SCOPES.get(index))
}
