//! The `permissions` rule: the values, scopes, and levels of `permissions` in a workflow or job.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `rule_permissions.go`.

use workflint_data::{PERMISSION_SCOPES, PERMISSIONS_FOR_ALL_SCOPES, permission_scope};
use workflint_diagnostics::quote::{quote, quotes, sorted_quotes};
use workflint_diagnostics::{Category, RuleMeta, Severity};
use workflint_syntax::{Job, KeyedEntry, Permissions as PermissionsSection, Workflow};

use crate::{Context, Rule};

/// The string form of `permissions` is not `read-all` or `write-all`.
pub(crate) static INVALID_PERMISSIONS_VALUE: RuleMeta = RuleMeta {
    id: "invalid-permissions-value",
    description: "The string form of `permissions` is not `read-all` or `write-all`",
    kind: "permissions",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-permissions-value.md",
};

/// A key in `permissions` is not a permission scope.
pub(crate) static UNKNOWN_PERMISSION_SCOPE: RuleMeta = RuleMeta {
    id: "unknown-permission-scope",
    description: "A key in `permissions` is not a permission scope",
    kind: "permissions",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/unknown-permission-scope.md",
};

/// A permission scope is set to a level that the scope does not accept.
pub(crate) static INVALID_PERMISSION_LEVEL: RuleMeta = RuleMeta {
    id: "invalid-permission-level",
    description: "A permission scope is set to a level that the scope does not accept",
    kind: "permissions",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-permission-level.md",
};

/// The reference's `RulePermissions`. It checks `permissions` of the workflow and of each job.
pub(crate) struct Permissions;

impl<'a> Rule<'a> for Permissions {
    fn workflow_pre(&mut self, workflow: &'a Workflow<'a>, cx: &mut Context<'_>) {
        if let Some(permissions) = workflow.permissions.value() {
            check(permissions, cx);
        }
    }

    fn job_pre(&mut self, job: &'a KeyedEntry<'a, Job<'a>>, cx: &mut Context<'_>) {
        if let Some(permissions) = job.value.permissions.value() {
            check(permissions, cx);
        }
    }
}

/// Reports the string form unless it sets every scope at once, and each scope that is unknown or
/// has a level it does not accept.
///
/// Scope names are case-sensitive here, as in the reference, although the parser folds them to
/// find duplicates. A scope whose value the parser reported has no value and is skipped
/// (`docs/differences.md#invalid-value-reported-once`).
fn check(permissions: &PermissionsSection<'_>, cx: &mut Context<'_>) {
    match permissions {
        PermissionsSection::All(all) => {
            if !PERMISSIONS_FOR_ALL_SCOPES.contains(&all.value()) {
                cx.report_in(
                    *all,
                    &INVALID_PERMISSIONS_VALUE,
                    all.span(),
                    format!(
                        "{} is invalid for permission for all the scopes. available values are {} or {{}}",
                        quote(all.value()),
                        sorted_quotes(PERMISSIONS_FOR_ALL_SCOPES),
                    ),
                );
            }
        }
        PermissionsSection::Scopes(scopes) => {
            for entry in scopes {
                let name = entry.key.value();
                let Some(scope) = permission_scope(name) else {
                    cx.report_in(
                        entry.key,
                        &UNKNOWN_PERMISSION_SCOPE,
                        entry.key.span(),
                        format!(
                            "unknown permission scope {}. all available permission scopes are {}",
                            quote(name),
                            sorted_quotes(PERMISSION_SCOPES.iter().map(|known| known.name)),
                        ),
                    );
                    continue;
                };
                let Some(level) = &entry.value else {
                    continue;
                };
                if !scope.levels.contains(&level.value()) {
                    cx.report_in(
                        *level,
                        &INVALID_PERMISSION_LEVEL,
                        level.span(),
                        format!(
                            "{} is invalid as permission of scope {}. available values are {}",
                            quote(level.value()),
                            quote(name),
                            quotes(scope.levels),
                        ),
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use workflint_diagnostics::{Color, FileId, SourceFile, render_oneline};

    use super::Permissions;
    use crate::{Context, Rule, visit};

    /// Lints `text` with only the `permissions` rule and returns its diagnostics in the one-line
    /// format.
    fn lint(text: &str) -> Vec<String> {
        let file = SourceFile::new(FileId::new(0), "test.yaml", text).unwrap();
        let document = workflint_yaml::parse(&file).document.unwrap();
        let parsed = workflint_syntax::parse(&file, &document);
        let mut rules: Vec<Box<dyn Rule<'_>>> = vec![Box::new(Permissions)];
        let mut cx = Context::new(&file);
        visit(&parsed.workflow, &mut rules, &mut cx);
        let mut out = Vec::new();
        render_oneline(&mut out, &file, &cx.into_diagnostics(), Color::Off).unwrap();
        String::from_utf8(out)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// Returns a workflow with `permissions` written as `section` under its job.
    fn job_permissions(section: &str) -> String {
        format!(
            "on: push
jobs:
  test:
    runs-on: ubuntu-latest
    permissions:{section}
    steps:
      - run: echo
"
        )
    }

    #[test]
    fn every_scope_passes_with_each_level_it_accepts() {
        for scope in workflint_data::PERMISSION_SCOPES {
            for level in scope.levels {
                let section = format!("\n      {}: {level}", scope.name);
                assert_eq!(lint(&job_permissions(&section)), Vec::<String>::new());
            }
        }
    }

    #[test]
    fn read_all_write_all_and_an_empty_mapping_pass() {
        for section in [" read-all", " write-all", " {}"] {
            assert_eq!(lint(&job_permissions(section)), Vec::<String>::new());
        }
    }

    #[test]
    fn a_string_that_does_not_set_every_scope_is_reported() {
        assert_eq!(
            lint(&job_permissions(" none")),
            [
                "test.yaml:5:18: \"none\" is invalid for permission for all the scopes. available values are \"read-all\", \"write-all\" or {} [invalid-permissions-value]"
            ]
        );
    }

    #[test]
    fn a_scope_name_in_another_case_is_unknown() {
        let out = lint(&job_permissions("\n      Contents: read"));
        assert_eq!(out.len(), 1);
        assert!(
            out[0].starts_with("test.yaml:6:7: unknown permission scope \"Contents\". all available permission scopes are \"actions\", ")
        );
        assert!(out[0].ends_with(", \"statuses\" [unknown-permission-scope]"));
    }

    #[test]
    fn a_level_is_reported_at_its_value_with_the_levels_of_its_scope() {
        assert_eq!(
            lint(&job_permissions(
                "\n      id-token: read\n      models: write"
            )),
            [
                "test.yaml:6:17: \"read\" is invalid as permission of scope \"id-token\". available values are \"write\", \"none\" [invalid-permission-level]",
                "test.yaml:7:15: \"write\" is invalid as permission of scope \"models\". available values are \"read\", \"none\" [invalid-permission-level]",
            ]
        );
    }

    #[test]
    fn a_scope_value_that_the_parser_reported_is_not_reported_again() {
        for section in [
            "\n      contents:",
            "\n      contents: ''",
            "\n      contents: [read]",
        ] {
            assert_eq!(lint(&job_permissions(section)), Vec::<String>::new());
        }
    }

    #[test]
    fn workflow_permissions_are_checked_too() {
        let text = "on: push
permissions:
  contents: foo
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo
";
        assert_eq!(
            lint(text),
            [
                "test.yaml:3:13: \"foo\" is invalid as permission of scope \"contents\". available values are \"read\", \"write\", \"none\" [invalid-permission-level]"
            ]
        );
    }
}
