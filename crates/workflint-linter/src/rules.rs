//! The rules whose diagnostics the driver reports, and their kind.

use workflint_diagnostics::{Category, RuleMeta, Severity};
use workflint_rules::kind::Kind;

/// The kind of [`INTERNAL_ERROR`]. The reference has no such kind, because a panic stops it, so
/// this kind is not in [`workflint_rules::kind::ALL`].
pub static INTERNAL_ERROR_KIND: Kind = Kind {
    name: "internal-error",
    description: "Reports a bug in workflint that stopped the lint of a file",
};

/// workflint panicked while linting a file, or a panic left a cache it needs unusable.
pub static INTERNAL_ERROR: RuleMeta = RuleMeta {
    id: "internal-error",
    description: "workflint stopped linting a file because of a bug",
    kind: "internal-error",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/internal-error.md",
};

/// Every rule in this module, in the order of their IDs.
pub static ALL: [&RuleMeta; 1] = [&INTERNAL_ERROR];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_error_has_its_own_kind() {
        assert_eq!(INTERNAL_ERROR.kind, INTERNAL_ERROR_KIND.name);
        assert!(
            workflint_rules::kind::ALL
                .iter()
                .all(|kind| kind.name != INTERNAL_ERROR_KIND.name)
        );
    }
}
