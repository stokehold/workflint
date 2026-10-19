//! The `credentials` rule: passwords written directly in `container` and `services`.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `rule_credentials.go`.

use workflint_diagnostics::quote::quote;
use workflint_diagnostics::{Category, RuleMeta, Severity};
use workflint_syntax::{Container, Job, KeyedEntry, is_expression_assigned};

use crate::{Context, Rule};

/// `credentials.password` of a container or service is not a single `${{ }}` expression.
pub(crate) static HARDCODED_PASSWORD: RuleMeta = RuleMeta {
    id: "hardcoded-password",
    description: "`credentials.password` of a container or service is not a single `${{ }}` expression",
    kind: "credentials",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/hardcoded-password.md",
};

/// The reference's `RuleCredentials`. It checks the job's `container` and each service.
pub(crate) struct Credentials;

impl<'a> Rule<'a> for Credentials {
    fn job_pre(&mut self, job: &'a KeyedEntry<'a, Job<'a>>, cx: &mut Context<'_>) {
        if let Some(container) = job.value.container.value() {
            check(container, "\"container\" section", cx);
        }
        if let Some(services) = job.value.services.literal() {
            for service in services {
                let place = format!("{} service", quote(service.key.value()));
                check(&service.value, &place, cx);
            }
        }
    }
}

/// Reports the password of `container` unless it is a single `${{ }}` expression. `place` names
/// the container in the message.
fn check(container: &Container<'_>, place: &str, cx: &mut Context<'_>) {
    let Some(password) = container
        .credentials
        .literal()
        .and_then(|credentials| credentials.password.value())
    else {
        return;
    };
    if !is_expression_assigned(password.value()) {
        cx.report_in(
            password.source,
            &HARDCODED_PASSWORD,
            password.span(),
            format!(
                "\"password\" section in {place} should be specified via secrets. do not put password value directly"
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use workflint_diagnostics::{Color, FileId, SourceFile, render_oneline};

    use super::Credentials;
    use crate::{Context, Rule, visit};

    /// Lints `text` with only the `credentials` rule and returns its diagnostics in the one-line
    /// format.
    fn lint(text: &str) -> Vec<String> {
        let file = SourceFile::new(FileId::new(0), "test.yaml", text).unwrap();
        let document = workflint_yaml::parse(&file).document.unwrap();
        let parsed = workflint_syntax::parse(&file, &document);
        let mut rules: Vec<Box<dyn Rule<'_>>> = vec![Box::new(Credentials)];
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

    /// Returns a workflow whose job container and service `db` both have `value` as the YAML
    /// scalar of `credentials.password`.
    fn workflow(value: &str) -> String {
        format!(
            "on: push
jobs:
  test:
    runs-on: ubuntu-latest
    container:
      image: node
      credentials:
        username: user
        password: {value}
    services:
      db:
        image: postgres
        credentials:
          username: user
          password: {value}
    steps:
      - run: echo
"
        )
    }

    #[test]
    fn a_literal_password_is_reported_in_the_container_and_each_service() {
        assert_eq!(
            lint(&workflow("pass")),
            [
                "test.yaml:9:19: \"password\" section in \"container\" section should be specified via secrets. do not put password value directly [hardcoded-password]",
                "test.yaml:15:21: \"password\" section in \"db\" service should be specified via secrets. do not put password value directly [hardcoded-password]",
            ]
        );
    }

    #[test]
    fn a_password_that_is_one_whole_expression_passes() {
        assert_eq!(
            lint(&workflow("${{ secrets.PASSWORD }}")),
            Vec::<String>::new()
        );
        assert_eq!(
            lint(&workflow("\"  ${{ secrets.PASSWORD }}  \"")),
            Vec::<String>::new()
        );
        // The reference checks the form only, so a literal inside `${{ }}` passes.
        assert_eq!(lint(&workflow("${{ 'pass' }}")), Vec::<String>::new());
    }

    #[test]
    fn a_password_with_text_around_its_expression_is_reported() {
        assert_eq!(lint(&workflow("pre-${{ secrets.PASSWORD }}")).len(), 2);
        assert_eq!(lint(&workflow("${{ secrets.A }}${{ secrets.B }}")).len(), 2);
    }

    #[test]
    fn credentials_written_as_an_expression_pass() {
        let text = "on: push
jobs:
  test:
    runs-on: ubuntu-latest
    container:
      image: node
      credentials: ${{ fromJSON(secrets.CREDENTIALS) }}
    steps:
      - run: echo
";
        assert_eq!(lint(text), Vec::<String>::new());
    }

    #[test]
    fn the_service_name_keeps_its_case_and_is_quoted() {
        let text = "on: push
jobs:
  test:
    runs-on: ubuntu-latest
    services:
      My\"DB:
        image: postgres
        credentials:
          username: user
          password: pass
    steps:
      - run: echo
";
        assert_eq!(
            lint(text),
            [
                "test.yaml:10:21: \"password\" section in \"My\\\"DB\" service should be specified via secrets. do not put password value directly [hardcoded-password]"
            ]
        );
    }
}
