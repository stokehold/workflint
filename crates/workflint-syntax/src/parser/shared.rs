//! The sections that workflows and jobs share, `permissions`, `env`,
//! `defaults`, and `concurrency`, and the job's `environment`, which has
//! the shape of `concurrency`. The reference's productions at
//! `parse.go:739-858`.
//!
//! Each production takes the key of its section and the value node. A
//! value with the wrong node kind gives an invalid field. A mapping that
//! lacks a required key still gives a valid field, with that key absent,
//! so that rules can check the keys that are there.

use workflint_yaml::{Node, NodeKind};

use super::mapping::Key;
use super::{KeySpec, Label, MapEntry, Parser, Section};
use crate::rules::MISSING_REQUIRED_KEY;
use crate::{
    Concurrency, Defaults, DefaultsRun, Env, EnvVar, Environment, Field, Keyed, KeyedEntry,
    Permissions, Str, Value,
};

/// The keys of `defaults`.
struct DefaultsSection;

impl Section for DefaultsSection {
    type Ast<'a> = Defaults<'a>;
}

static DEFAULTS_KEYS: [KeySpec<DefaultsSection>; 1] = [KeySpec {
    name: "run",
    parse: |p, d, e| d.run = p.parse_defaults_run(e),
}];

/// The keys of `defaults.run`.
struct DefaultsRunSection;

impl Section for DefaultsRunSection {
    type Ast<'a> = DefaultsRun<'a>;
}

static DEFAULTS_RUN_KEYS: [KeySpec<DefaultsRunSection>; 2] = [
    KeySpec {
        name: "shell",
        parse: |p, r, e| r.shell = p.parse_template(e.key.string, e.value, false),
    },
    KeySpec {
        name: "working-directory",
        parse: |p, r, e| r.working_directory = p.parse_template(e.key.string, e.value, false),
    },
];

/// The keys of `concurrency`.
struct ConcurrencySection;

impl Section for ConcurrencySection {
    type Ast<'a> = Concurrency<'a>;
}

static CONCURRENCY_KEYS: [KeySpec<ConcurrencySection>; 2] = [
    KeySpec {
        name: "group",
        parse: |p, c, e| c.group = p.parse_template(e.key.string, e.value, false),
    },
    KeySpec {
        name: "cancel-in-progress",
        parse: |p, c, e| c.cancel_in_progress = p.parse_bool(e.key.string, e.value),
    },
];

/// The keys of `environment`.
struct EnvironmentSection;

impl Section for EnvironmentSection {
    type Ast<'a> = Environment<'a>;
}

static ENVIRONMENT_KEYS: [KeySpec<EnvironmentSection>; 3] = [
    KeySpec {
        name: "name",
        parse: |p, v, e| v.name = p.parse_template(e.key.string, e.value, false),
    },
    KeySpec {
        name: "url",
        parse: |p, v, e| v.url = p.parse_template(e.key.string, e.value, false),
    },
    KeySpec {
        name: "deployment",
        parse: |p, v, e| v.deployment = p.parse_bool(e.key.string, e.value),
    },
];

impl<'a> Parser<'a, '_> {
    /// Parses `permissions`, the reference's `parsePermissions`.
    ///
    /// A scalar is the string form, such as `read-all`. An empty one is
    /// reported and kept, so the `permissions` rule reports it as well, as
    /// in the reference. Anything else is a case-insensitive mapping of
    /// scopes to non-empty strings. The mapping may have no entries, since
    /// `permissions: {}` turns every scope off. The `permissions` rule
    /// checks the scope names and values.
    pub(crate) fn parse_permissions(
        &mut self,
        key: Key<'a>,
        node: &'a Node,
    ) -> Field<'a, Permissions<'a>> {
        if let Some(all) = Str::new(node) {
            self.check_string(node, false);
            return Field::Valid {
                key: key.string,
                span: node.span,
                value: Permissions::All(all),
            };
        }
        let entries = self.parse_mapping(Label::Section("permissions"), node, true, false);
        let scopes = self.keyed_strings(entries, false);
        mapping_field(key, node, Permissions::Scopes(scopes))
    }

    /// Parses `env` of a workflow, job, step, or container, the reference's
    /// `parseEnv`.
    ///
    /// A scalar must be a single `${{ }}` expression. A null `env:` is a
    /// scalar too, so it gets that message rather than the empty-section
    /// one, as in the reference. Anything else is a case-insensitive
    /// mapping of names to strings that may be empty, and must have
    /// entries.
    pub(crate) fn parse_env(&mut self, key: Key<'a>, node: &'a Node) -> Value<'a, Env<'a>> {
        let key = key.string;
        if let NodeKind::Scalar(_) = node.kind {
            return match self.parse_expression(node, "mapping value for \"env\" section") {
                Some(expr) => Value::Expression { key, expr },
                None => Value::Invalid {
                    key,
                    span: node.span,
                },
            };
        }
        let entries = self.parse_mapping(Label::Section("env"), node, false, false);
        let vars = self.env_vars(entries);
        match node.kind {
            NodeKind::Mapping(_) => Value::Literal {
                key,
                span: node.span,
                value: vars,
            },
            NodeKind::Scalar(_) | NodeKind::Sequence(_) | NodeKind::Alias(_) => Value::Invalid {
                key,
                span: node.span,
            },
        }
    }

    /// Parses `defaults`, the reference's `parseDefaults`: a mapping whose
    /// only key is `run`, with `shell` and `working-directory`.
    ///
    /// Without a `run` key it reports `"defaults" section should have "run"
    /// section` at the value. A `run` key with an invalid value counts as
    /// written.
    pub(crate) fn parse_defaults(
        &mut self,
        key: Key<'a>,
        node: &'a Node,
    ) -> Field<'a, Defaults<'a>> {
        let mut defaults = Defaults::default();
        let entries = self.parse_mapping(Label::Section("defaults"), node, false, true);
        self.parse_keys(entries, "defaults", &DEFAULTS_KEYS, &mut defaults);
        if !defaults.run.is_present() {
            self.error(
                &MISSING_REQUIRED_KEY,
                node.span,
                "\"defaults\" section should have \"run\" section".to_owned(),
            );
        }
        mapping_field(key, node, defaults)
    }

    fn parse_defaults_run(&mut self, entry: &MapEntry<'a>) -> Field<'a, DefaultsRun<'a>> {
        let mut run = DefaultsRun::default();
        let entries = self.parse_mapping(Label::Section("run"), entry.value, false, true);
        self.parse_keys(entries, "run", &DEFAULTS_RUN_KEYS, &mut run);
        mapping_field(entry.key, entry.value, run)
    }

    /// Parses `concurrency`, the reference's `parseConcurrency`.
    ///
    /// A scalar is the group, with no key. Otherwise it is a mapping with
    /// `group` and `cancel-in-progress`. Without a `group` key it reports
    /// `group name is missing in "concurrency" section` at the section's
    /// key, also when the value is not a mapping.
    pub(crate) fn parse_concurrency(
        &mut self,
        key: Key<'a>,
        node: &'a Node,
    ) -> Field<'a, Concurrency<'a>> {
        if let NodeKind::Scalar(_) = node.kind {
            let concurrency = Concurrency {
                group: self.parse_template(None, node, false),
                ..Concurrency::default()
            };
            return Field::Valid {
                key: key.string,
                span: node.span,
                value: concurrency,
            };
        }
        let mut concurrency = Concurrency::default();
        let entries = self.parse_mapping(Label::Section("concurrency"), node, false, true);
        self.parse_keys(entries, "concurrency", &CONCURRENCY_KEYS, &mut concurrency);
        if !concurrency.group.is_present() {
            self.error(
                &MISSING_REQUIRED_KEY,
                key.span,
                "group name is missing in \"concurrency\" section".to_owned(),
            );
        }
        mapping_field(key, node, concurrency)
    }

    /// Parses `environment` of a job, the reference's `parseEnvironment`.
    ///
    /// A scalar is the name, with no key. Otherwise it is a mapping with
    /// `name`, `url`, and `deployment`. Without a `name` key it reports
    /// `name is missing in "environment" section` at the section's key,
    /// also when the value is not a mapping.
    pub(crate) fn parse_environment(
        &mut self,
        key: Key<'a>,
        node: &'a Node,
    ) -> Field<'a, Environment<'a>> {
        if let NodeKind::Scalar(_) = node.kind {
            let environment = Environment {
                name: self.parse_template(None, node, false),
                ..Environment::default()
            };
            return Field::Valid {
                key: key.string,
                span: node.span,
                value: environment,
            };
        }
        let mut environment = Environment::default();
        let entries = self.parse_mapping(Label::Section("environment"), node, false, true);
        self.parse_keys(entries, "environment", &ENVIRONMENT_KEYS, &mut environment);
        if !environment.name.is_present() {
            self.error(
                &MISSING_REQUIRED_KEY,
                key.span,
                "name is missing in \"environment\" section".to_owned(),
            );
        }
        mapping_field(key, node, environment)
    }

    /// Collects the variables of `env`, a case-insensitive mapping of names
    /// to strings that may be empty. See [`keyed_strings`](Self::keyed_strings).
    fn env_vars(&mut self, entries: Vec<MapEntry<'a>>) -> Env<'a> {
        let mut vars = Vec::with_capacity(entries.len());
        for entry in entries {
            let value = self
                .check_string(entry.value, true)
                .map(|value| self.template(value));
            if let Some(key) = entry.key.string {
                let name = self.template(key);
                vars.push(KeyedEntry::new(
                    key,
                    entry.value.span,
                    EnvVar { name, value },
                ));
            }
        }
        Keyed::new(vars)
    }

    /// Collects the entries of a case-insensitive mapping of strings, such
    /// as the scopes of `permissions`. A value that is not a
    /// string, or is empty unless `allow_empty`, is reported and kept as
    /// `None`.
    ///
    /// An entry whose key is not a scalar is left out, because a
    /// [`Keyed`] collection needs a string key. The mapping walker has
    /// reported the key. The reference keeps such an entry under the name
    /// `""` (`docs/differences.md#non-scalar-keys`).
    pub(super) fn keyed_strings(
        &mut self,
        entries: Vec<MapEntry<'a>>,
        allow_empty: bool,
    ) -> Keyed<'a, Option<Str<'a>>> {
        let mut keyed = Vec::with_capacity(entries.len());
        for entry in entries {
            let value = self.check_string(entry.value, allow_empty);
            if let Some(name) = entry.key.string {
                keyed.push(KeyedEntry::new(name, entry.value.span, value));
            }
        }
        Keyed::new(keyed)
    }
}

/// Returns `value` as a valid field if `node` is a mapping, and an invalid
/// one otherwise. The mapping walker has reported a node of another kind.
pub(super) fn mapping_field<'a, T>(key: Key<'a>, node: &'a Node, value: T) -> Field<'a, T> {
    match node.kind {
        NodeKind::Mapping(_) => Field::Valid {
            key: key.string,
            span: node.span,
            value,
        },
        NodeKind::Scalar(_) | NodeKind::Sequence(_) | NodeKind::Alias(_) => Field::Invalid {
            key: key.string,
            span: node.span,
        },
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use std::fmt::Debug;

    use super::*;
    use crate::testing::{Text, parse_with};

    /// Runs `production` on the first entry of the root mapping of `text`,
    /// and returns the diagnostics followed by the line it returns.
    fn section(
        text: &str,
        production: impl for<'a, 'f> Fn(&mut Parser<'a, 'f>, Key<'a>, &'a Node) -> String,
    ) -> Vec<String> {
        parse_with(text, |parser, root| {
            let entries = parser.parse_mapping(Label::Section("test"), root, true, true);
            let entry = &entries[0];
            vec![production(parser, entry.key, entry.value)]
        })
    }

    fn field<T>(field: &Field<'_, T>, render: impl Fn(&T) -> String) -> String {
        match field {
            Field::Valid { value, .. } => render(value),
            Field::Invalid { .. } => "invalid".to_owned(),
            Field::Absent => "absent".to_owned(),
        }
    }

    fn value<T>(value: &Value<'_, T>, render: impl Fn(&T) -> String) -> String {
        match value {
            Value::Literal { value, .. } => render(value),
            Value::Expression { expr, .. } => format!("expression {:?}", expr.source.value()),
            Value::Invalid { .. } => "invalid".to_owned(),
            Value::Absent => "absent".to_owned(),
        }
    }

    fn string(field: &Field<'_, impl Text>) -> String {
        self::field(field, |value| format!("{:?}", value.text()))
    }

    fn debug<T: Debug>(value: &T) -> String {
        format!("{value:?}")
    }

    /// Prints a keyed collection of strings as `key=value` pairs, with `-`
    /// for a value that is not a valid string.
    fn strings(keyed: &Keyed<'_, impl Text>) -> String {
        let pairs: Vec<String> = keyed
            .iter()
            .map(|entry| format!("{}={}", entry.key.value(), entry.value.text()))
            .collect();
        format!("[{}]", pairs.join(", "))
    }

    fn permissions(text: &str) -> Vec<String> {
        section(text, |p, key, node| {
            field(
                &p.parse_permissions(key, node),
                |permissions| match permissions {
                    Permissions::All(all) => format!("all {:?}", all.value()),
                    Permissions::Scopes(scopes) => format!("scopes {}", strings(scopes)),
                },
            )
        })
    }

    fn env(text: &str) -> Vec<String> {
        section(text, |p, key, node| {
            value(&p.parse_env(key, node), |vars| {
                format!("vars {}", strings(vars))
            })
        })
    }

    fn defaults(text: &str) -> Vec<String> {
        section(text, |p, key, node| {
            field(&p.parse_defaults(key, node), |defaults: &Defaults<'_>| {
                let run = field(&defaults.run, |run| {
                    format!(
                        "shell {} working-directory {}",
                        string(&run.shell),
                        string(&run.working_directory)
                    )
                });
                format!("run {run}")
            })
        })
    }

    fn concurrency(text: &str) -> Vec<String> {
        section(text, |p, key, node| {
            field(
                &p.parse_concurrency(key, node),
                |concurrency: &Concurrency<'_>| {
                    format!(
                        "group {} key {:?} cancel-in-progress {}",
                        string(&concurrency.group),
                        concurrency.group.key().map(Str::value),
                        value(&concurrency.cancel_in_progress, debug)
                    )
                },
            )
        })
    }

    fn environment(text: &str) -> Vec<String> {
        section(text, |p, key, node| {
            field(
                &p.parse_environment(key, node),
                |environment: &Environment<'_>| {
                    format!(
                        "name {} key {:?} url {} deployment {}",
                        string(&environment.name),
                        environment.name.key().map(Str::value),
                        string(&environment.url),
                        value(&environment.deployment, debug)
                    )
                },
            )
        })
    }

    #[test]
    fn permissions_in_the_string_form_keep_the_string() {
        assert_eq!(
            permissions("permissions: read-all\n"),
            [r#"all "read-all""#]
        );
        assert_eq!(permissions("permissions: ~\n"), [r#"all "~""#]);
        assert_eq!(permissions("permissions: 1\n"), [r#"all "1""#]);
    }

    #[test]
    fn empty_permissions_are_reported_and_kept_for_the_permissions_rule() {
        assert_eq!(
            permissions("permissions:\n"),
            [
                "1:13: string should not be empty [empty-string]",
                r#"all """#
            ]
        );
        assert_eq!(
            permissions("permissions: ''\n"),
            [
                "1:14: string should not be empty [empty-string]",
                r#"all """#
            ]
        );
    }

    #[test]
    fn permission_scopes_are_case_insensitive_and_may_be_empty() {
        assert_eq!(permissions("permissions: {}\n"), ["scopes []"]);
        assert_eq!(
            permissions("permissions:\n  Contents: read\n  ID-token: write\n"),
            ["scopes [Contents=read, ID-token=write]"]
        );
    }

    #[test]
    fn invalid_permission_scopes_are_reported_and_kept_without_a_value() {
        assert_eq!(
            permissions(
                "permissions:\n  Contents: read\n  contents: write\n  issues: ''\n  pages: [a]\n  ? [x]\n  : read\n"
            ),
            [
                r#"3:3: key "contents" is duplicated in "permissions" section. previously defined at line:2,col:3. note that this key is case insensitive [duplicate-key]"#,
                "4:11: string should not be empty [empty-string]",
                r#"5:10: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
                r#"6:5: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
                "scopes [Contents=read, issues=-, pages=-]",
            ]
        );
    }

    #[test]
    fn permissions_that_are_neither_a_scalar_nor_a_mapping_are_invalid() {
        assert_eq!(
            permissions("permissions: []\n"),
            [
                r#"1:14: "permissions" section is sequence node but mapping node is expected [unexpected-node-kind]"#,
                "invalid"
            ]
        );
    }

    #[test]
    fn env_is_a_case_insensitive_mapping_of_strings_that_may_be_empty() {
        assert_eq!(
            env("env:\n  FOO: bar\n  Empty: ''\n  NUM: 1\n  NULL:\n"),
            ["vars [FOO=bar, Empty=, NUM=1, NULL=]"]
        );
        assert_eq!(
            env("env:\n  A: x\n  a: y\n  B: [x]\n"),
            [
                r#"3:3: key "a" is duplicated in "env" section. previously defined at line:2,col:3. note that this key is case insensitive [duplicate-key]"#,
                r#"4:6: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
                "vars [A=x, B=-]",
            ]
        );
    }

    #[test]
    fn env_as_a_scalar_must_be_one_expression() {
        assert_eq!(
            env("env: ${{ fromJSON(inputs.env) }}\n"),
            [r#"expression "${{ fromJSON(inputs.env) }}""#]
        );
        for (text, column) in [("env: FOO=bar\n", 6), ("env: 1\n", 6), ("env:\n", 5)] {
            assert_eq!(
                env(text),
                [
                    format!(
                        r#"1:{column}: expecting a single ${{{{...}}}} expression or mapping value for "env" section, but found plain text node [expected-single-expression]"#
                    ),
                    "invalid".to_owned()
                ],
                "{text:?}"
            );
        }
    }

    #[test]
    fn env_must_not_be_an_empty_mapping_or_a_sequence() {
        assert_eq!(
            env("env: {}\n"),
            [
                r#"1:6: "env" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
                "vars []"
            ]
        );
        assert_eq!(
            env("env: [a]\n"),
            [
                r#"1:6: "env" section is sequence node but mapping node is expected [unexpected-node-kind]"#,
                "invalid"
            ]
        );
    }

    #[test]
    fn defaults_have_run_with_shell_and_working_directory() {
        assert_eq!(
            defaults("defaults:\n  run:\n    shell: bash\n    working-directory: src\n"),
            [r#"run shell "bash" working-directory "src""#]
        );
        assert_eq!(
            defaults("defaults:\n  run:\n    shell: ''\n    x: 1\n    working-directory: [a]\n"),
            [
                "3:12: string should not be empty [empty-string]",
                r#"4:5: unexpected key "x" for "run" section. expected one of "shell", "working-directory" [unexpected-key]"#,
                r#"5:24: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
                "run shell invalid working-directory invalid",
            ]
        );
        // The reference keeps the shell as `""`, and its `shell-name` rule
        // reports `shell name "" is invalid` too.
        assert_eq!(
            defaults("defaults:\n  run:\n    shell: [x]\n"),
            [
                r#"3:12: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
                "run shell invalid working-directory absent",
            ]
        );
    }

    #[test]
    fn defaults_without_run_are_reported_at_the_value() {
        assert_eq!(
            defaults("defaults:\n"),
            [
                r#"1:10: "defaults" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
                r#"1:10: "defaults" section should have "run" section [missing-required-key]"#,
                "invalid",
            ]
        );
        assert_eq!(
            defaults("defaults: {}\n"),
            [
                r#"1:11: "defaults" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
                r#"1:11: "defaults" section should have "run" section [missing-required-key]"#,
                "run absent",
            ]
        );
        assert_eq!(
            defaults("defaults:\n  Run: {shell: bash}\n"),
            [
                r#"2:3: "defaults" section should have "run" section [missing-required-key]"#,
                r#"2:3: expected "run" key for "defaults" section but got "Run" [unexpected-key]"#,
                "run absent",
            ]
        );
        assert_eq!(
            defaults("defaults: []\n"),
            [
                r#"1:11: "defaults" section should have "run" section [missing-required-key]"#,
                r#"1:11: "defaults" section is sequence node but mapping node is expected [unexpected-node-kind]"#,
                "invalid",
            ]
        );
    }

    #[test]
    fn an_invalid_run_is_reported_and_does_not_count_as_missing() {
        assert_eq!(
            defaults("defaults:\n  run:\n"),
            [
                r#"2:7: "run" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
                "run invalid",
            ]
        );
        assert_eq!(
            defaults("defaults:\n  run: x\n"),
            [
                r#"2:8: "run" section is scalar node but mapping node is expected [unexpected-node-kind]"#,
                "run invalid",
            ]
        );
    }

    #[test]
    fn concurrency_as_a_scalar_is_the_group_with_no_key() {
        assert_eq!(
            concurrency("concurrency: ci-${{ github.ref }}\n"),
            [r#"group "ci-${{ github.ref }}" key None cancel-in-progress absent"#]
        );
        assert_eq!(
            concurrency("concurrency:\n"),
            [
                "1:13: string should not be empty [empty-string]",
                "group invalid key None cancel-in-progress absent",
            ]
        );
    }

    #[test]
    fn concurrency_as_a_mapping_has_group_and_cancel_in_progress() {
        assert_eq!(
            concurrency("concurrency:\n  group: ci\n  cancel-in-progress: true\n"),
            [r#"group "ci" key Some("group") cancel-in-progress true"#]
        );
        assert_eq!(
            concurrency("concurrency:\n  group: ci\n  cancel-in-progress: ${{ inputs.c }}\n"),
            [r#"group "ci" key Some("group") cancel-in-progress expression "${{ inputs.c }}""#]
        );
        assert_eq!(
            concurrency("concurrency:\n  group: ''\n  foo: y\n  cancel-in-progress: 1\n"),
            [
                "2:10: string should not be empty [empty-string]",
                r#"3:3: unexpected key "foo" for "concurrency" section. expected one of "cancel-in-progress", "group" [unexpected-key]"#,
                r#"4:23: expected bool value but found scalar node with "!!int" tag [invalid-literal-value]"#,
                r#"group invalid key Some("group") cancel-in-progress invalid"#,
            ]
        );
    }

    #[test]
    fn concurrency_without_a_group_is_reported_at_the_key() {
        assert_eq!(
            concurrency("concurrency:\n  cancel-in-progress: yes\n"),
            [
                r#"1:1: group name is missing in "concurrency" section [missing-required-key]"#,
                r#"2:23: expecting a single ${{...}} expression or boolean literal "true" or "false", but found plain text node [expected-single-expression]"#,
                "group absent key None cancel-in-progress invalid",
            ]
        );
        assert_eq!(
            concurrency("concurrency: {}\n"),
            [
                r#"1:1: group name is missing in "concurrency" section [missing-required-key]"#,
                r#"1:14: "concurrency" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
                "group absent key None cancel-in-progress absent",
            ]
        );
        assert_eq!(
            concurrency("concurrency: [a]\n"),
            [
                r#"1:1: group name is missing in "concurrency" section [missing-required-key]"#,
                r#"1:14: "concurrency" section is sequence node but mapping node is expected [unexpected-node-kind]"#,
                "invalid",
            ]
        );
    }

    #[test]
    fn environment_as_a_scalar_is_the_name_with_no_key() {
        assert_eq!(
            environment("environment: production\n"),
            [r#"name "production" key None url absent deployment absent"#]
        );
        assert_eq!(
            environment("environment:\n"),
            [
                "1:13: string should not be empty [empty-string]",
                "name invalid key None url absent deployment absent",
            ]
        );
    }

    #[test]
    fn environment_as_a_mapping_has_name_url_and_deployment() {
        assert_eq!(
            environment(
                "environment:\n  name: prod\n  url: ${{ steps.d.outputs.url }}\n  deployment: false\n"
            ),
            [r#"name "prod" key Some("name") url "${{ steps.d.outputs.url }}" deployment false"#]
        );
        assert_eq!(
            environment(
                "environment:\n  name: ''\n  foo: 1\n  url: ''\n  deployment: ${{ inputs.d }}\n"
            ),
            [
                "2:9: string should not be empty [empty-string]",
                r#"3:3: unexpected key "foo" for "environment" section. expected one of "deployment", "name", "url" [unexpected-key]"#,
                "4:8: string should not be empty [empty-string]",
                r#"name invalid key Some("name") url invalid deployment expression "${{ inputs.d }}""#,
            ]
        );
    }

    #[test]
    fn environment_without_a_name_is_reported_at_the_key() {
        assert_eq!(
            environment("environment:\n  url: x\n  deployment: no\n"),
            [
                r#"1:1: name is missing in "environment" section [missing-required-key]"#,
                r#"3:15: expecting a single ${{...}} expression or boolean literal "true" or "false", but found plain text node [expected-single-expression]"#,
                r#"name absent key None url "x" deployment invalid"#,
            ]
        );
        assert_eq!(
            environment("environment:\n  Name: prod\n"),
            [
                r#"1:1: name is missing in "environment" section [missing-required-key]"#,
                r#"2:3: unexpected key "Name" for "environment" section. expected one of "deployment", "name", "url" [unexpected-key]"#,
                "name absent key None url absent deployment absent",
            ]
        );
        assert_eq!(
            environment("environment: {}\n"),
            [
                r#"1:1: name is missing in "environment" section [missing-required-key]"#,
                r#"1:14: "environment" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
                "name absent key None url absent deployment absent",
            ]
        );
        assert_eq!(
            environment("environment: [a]\n"),
            [
                r#"1:1: name is missing in "environment" section [missing-required-key]"#,
                r#"1:14: "environment" section is sequence node but mapping node is expected [unexpected-node-kind]"#,
                "invalid",
            ]
        );
    }
}
