//! The `env-var` rule: names in `env` that contain `&`, `=`, or white space.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `rule_env_var.go`.

use workflint_diagnostics::quote::quote;
use workflint_diagnostics::{Category, RuleMeta, Severity};
use workflint_syntax::{Env, Job, KeyedEntry, Step, Value, Workflow, contains_expression};

use crate::kind::{self, Kind};
use crate::{Context, Rule};

/// A name in `env` contains `&`, `=`, a space, or a tab.
pub(crate) static INVALID_ENV_VAR_NAME: RuleMeta = RuleMeta {
    id: "invalid-env-var-name",
    description: "A name in `env` contains `&`, `=`, a space, or a tab",
    kind: "env-var",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-env-var-name.md",
};

/// The reference's `RuleEnvVar`. It checks the `env` of the workflow, each job, the job's
/// container, each service, and each step.
pub(crate) struct EnvVar;

impl<'a> Rule<'a> for EnvVar {
    fn kind(&self) -> &'static Kind {
        &kind::ENV_VAR
    }

    fn workflow_pre(&mut self, workflow: &'a Workflow<'a>, cx: &mut Context<'_>) {
        check(&workflow.env, cx);
    }

    fn job_pre(&mut self, job: &'a KeyedEntry<'a, Job<'a>>, cx: &mut Context<'_>) {
        check(&job.value.env, cx);
        if let Some(container) = job.value.container.value() {
            check(&container.env, cx);
        }
        if let Some(services) = job.value.services.literal() {
            for service in services {
                check(&service.value.env, cx);
            }
        }
    }

    fn step(&mut self, step: &'a Step<'a>, cx: &mut Context<'_>) {
        check(&step.env, cx);
    }
}

/// Reports each name of `env` that contains `&`, `=`, a space, or a tab. A whole `env: ${{ }}`
/// and a name with a `${{ }}` in it are skipped (rhysd/actionlint#312).
fn check(env: &Value<'_, Env<'_>>, cx: &mut Context<'_>) {
    let Some(env) = env.literal() else {
        return;
    };
    for var in env {
        let name = &var.value.name;
        if contains_expression(name.value()) {
            continue;
        }
        if name.value().contains(['&', '=', ' ', '\t']) {
            cx.report_in(
                name.source,
                &INVALID_ENV_VAR_NAME,
                name.span(),
                format!(
                    "environment variable name {} is invalid. '&', '=' and spaces should not be contained",
                    quote(name.value())
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use workflint_diagnostics::{Color, FileId, SourceFile, render_oneline};

    use super::EnvVar;
    use crate::{Context, Rule, visit};

    /// Lints `text` with only the `env-var` rule and returns its diagnostics in the one-line
    /// format.
    fn lint(text: &str) -> Vec<String> {
        let file = SourceFile::new(FileId::new(0), "test.yaml", text).unwrap();
        let document = workflint_yaml::parse(&file).document.unwrap();
        let parsed = workflint_syntax::parse(&file, &document);
        let mut rules: Vec<Box<dyn Rule<'_>>> = vec![Box::new(EnvVar)];
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

    /// Returns a workflow whose top-level `env` has one variable, written as the YAML scalar
    /// `name`.
    fn workflow_env(name: &str) -> String {
        format!(
            "on: push\nenv:\n  {name}: ''\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo\n"
        )
    }

    /// `TestRuleEnvVarCheckValidName` in `rule_env_var_test.go`.
    #[test]
    fn names_without_ampersand_equals_or_white_space_pass() {
        for name in ["foo_bar", "FOO_BAR", "foo-bar", "_", "-"] {
            assert_eq!(lint(&workflow_env(name)), Vec::<String>::new(), "{name}");
        }
    }

    /// `TestRuleEnvVarCheckInvalidName` in `rule_env_var_test.go`, which lists `a=b` twice,
    /// plus `&` and a tab, which the reference's check also rejects.
    #[test]
    fn names_with_ampersand_equals_space_or_tab_are_reported() {
        for (name, quoted) in [
            ("'a b'", r#""a b""#),
            ("a=b", r#""a=b""#),
            ("a&b", r#""a&b""#),
            (r#""a\tb""#, r#""a\tb""#),
        ] {
            assert_eq!(
                lint(&workflow_env(name)),
                [format!(
                    "test.yaml:3:3: environment variable name {quoted} is invalid. '&', '=' and spaces should not be contained [invalid-env-var-name]"
                )],
                "{name}"
            );
        }
    }

    /// `TestRuleEnvVarSkipEdgeCaseEnv` in `rule_env_var_test.go`: no `env`, and `env` written as
    /// an expression.
    #[test]
    fn a_missing_env_and_an_env_expression_pass() {
        let text =
            "on: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo\n";
        assert_eq!(lint(text), Vec::<String>::new());
        let text = "on: push\nenv: ${{ fromJSON('{}') }}\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo\n";
        assert_eq!(lint(text), Vec::<String>::new());
    }

    #[test]
    fn names_that_contain_an_expression_are_skipped() {
        assert_eq!(
            lint(&workflow_env("'${{ github.event_name }} x'")),
            Vec::<String>::new()
        );
        // `}}` before `${{` is not an expression, as in the reference's `ContainsExpression`.
        assert_eq!(lint(&workflow_env("'}} ${{ x'")).len(), 1);
    }

    #[test]
    fn every_env_section_is_checked() {
        let text = "on: push
env:
  a b: ''
jobs:
  test:
    runs-on: ubuntu-latest
    env:
      c d: ''
    container:
      image: node
      env:
        e f: ''
    services:
      db:
        image: postgres
        env:
          g h: ''
    steps:
      - run: echo
        env:
          i j: ''
";
        let lines = lint(text);
        let positions: Vec<_> = lines
            .iter()
            .map(|line| line.split(": ").next().unwrap())
            .collect();
        assert_eq!(
            positions,
            [
                "test.yaml:3:3",
                "test.yaml:8:7",
                "test.yaml:12:9",
                "test.yaml:17:11",
                "test.yaml:21:11",
            ]
        );
    }
}
