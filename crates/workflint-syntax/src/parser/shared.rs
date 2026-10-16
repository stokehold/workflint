//! The sections that workflows and jobs share, `permissions`, `env`,
//! `defaults`, and `concurrency`, and the job's `environment`, which has
//! the shape of `concurrency`. The reference's productions at
//! `parse.go:739-858`.

use workflint_yaml::Node;

use super::Parser;
use super::mapping::Key;
use crate::{Concurrency, Defaults, Env, Environment, Field, Permissions, Value};

impl<'a> Parser<'a, '_> {
    pub(crate) fn parse_permissions(
        &mut self,
        _key: Key<'a>,
        _node: &'a Node,
    ) -> Field<'a, Permissions<'a>> {
        Field::Absent
    }

    pub(crate) fn parse_env(&mut self, _key: Key<'a>, _node: &'a Node) -> Value<'a, Env<'a>> {
        Value::Absent
    }

    pub(crate) fn parse_defaults(
        &mut self,
        _key: Key<'a>,
        _node: &'a Node,
    ) -> Field<'a, Defaults<'a>> {
        Field::Absent
    }

    pub(crate) fn parse_concurrency(
        &mut self,
        _key: Key<'a>,
        _node: &'a Node,
    ) -> Field<'a, Concurrency<'a>> {
        Field::Absent
    }

    pub(crate) fn parse_environment(
        &mut self,
        _key: Key<'a>,
        _node: &'a Node,
    ) -> Field<'a, Environment<'a>> {
        Field::Absent
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use std::fmt::Debug;

    use super::*;
    use crate::parser::Label;
    use crate::testing::parse_with;
    use crate::{Keyed, Str};

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

    fn string(field: &Field<'_, Str<'_>>) -> String {
        self::field(field, |value| format!("{:?}", value.value()))
    }

    fn debug<T: Debug>(value: &T) -> String {
        format!("{value:?}")
    }

    /// Prints a keyed collection of strings as `key=value` pairs, with `-`
    /// for a value that is not a valid string.
    fn strings(keyed: &Keyed<'_, Option<Str<'_>>>) -> String {
        let pairs: Vec<String> = keyed
            .iter()
            .map(|entry| {
                let value = entry.value.map_or("-", |value| value.value());
                format!("{}={value}", entry.key.value())
            })
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
                r#"4:3: key "contents" is duplicated in "permissions" section. previously defined at line:3,col:3. note that this key is case insensitive [duplicate-key]"#,
                "5:11: string should not be empty [empty-string]",
                r#"6:10: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
                r#"7:5: expected scalar node for string value but found sequence node with "!!seq" tag [unexpected-node-kind]"#,
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
    }

    #[test]
    fn defaults_without_run_are_reported_at_the_value() {
        assert_eq!(
            defaults("defaults:\n"),
            [
                r#"1:10: "defaults" section should have "run" section [missing-required-key]"#,
                r#"1:10: "defaults" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
                "invalid",
            ]
        );
        assert_eq!(
            defaults("defaults: {}\n"),
            [
                r#"1:11: "defaults" section should have "run" section [missing-required-key]"#,
                r#"1:11: "defaults" section should not be empty. please remove this section if it's unnecessary [empty-section]"#,
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
                r#"1:11: "defaults" section is sequence node but mapping node is expected [unexpected-node-kind]"#,
                r#"1:11: "defaults" section should have "run" section [missing-required-key]"#,
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
