//! The `shell-name` rule: `shell` values of `defaults.run` and of steps that are not a shell of
//! the job's platform.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `rule_shell_name.go`, lines 34-178.

use workflint_data::{ShellPlatform, shell_names};
use workflint_diagnostics::quote::{quote, sorted_quotes};
use workflint_diagnostics::{Category, RuleMeta, Severity};
use workflint_syntax::{
    Defaults, Exec, Field, Job, KeyedEntry, Runner, Step, Template, Workflow, contains_expression,
};
use workflint_yaml::simple_lowercase;

use crate::{Context, Rule};

/// A `shell` value is not a shell name that the job's platform has, and is not a command with
/// `{0}`.
pub(crate) static INVALID_SHELL_NAME: RuleMeta = RuleMeta {
    id: "invalid-shell-name",
    description: "A `shell` value is not a shell of the job's platform or a command with `{0}`",
    kind: "shell-name",
    category: Category::Correctness,
    default_severity: Severity::Error,
    docs_url: "https://github.com/stokehold/workflint/blob/main/docs/rules/invalid-shell-name.md",
};

/// The reference's `RuleShellName`. It keeps the platform of the current job.
pub(crate) struct ShellName {
    platform: ShellPlatform,
}

impl ShellName {
    pub(crate) fn new() -> Self {
        Self {
            platform: ShellPlatform::Any,
        }
    }
}

impl<'a> Rule<'a> for ShellName {
    fn workflow_pre(&mut self, workflow: &'a Workflow<'a>, cx: &mut Context<'_>) {
        check_defaults(&workflow.defaults, ShellPlatform::Any, cx);
    }

    fn job_pre(&mut self, job: &'a KeyedEntry<'a, Job<'a>>, cx: &mut Context<'_>) {
        // The reference checks the job's defaults only when `runs-on` is written.
        if !job.value.runs_on.is_present() {
            return;
        }
        self.platform = job
            .value
            .runs_on
            .value()
            .map_or(ShellPlatform::Any, platform_of);
        check_defaults(&job.value.defaults, self.platform, cx);
    }

    fn step(&mut self, step: &'a Step<'a>, cx: &mut Context<'_>) {
        if let Some(Exec::Run(run)) = &step.exec {
            check(&run.shell, self.platform, cx);
        }
    }

    fn job_post(&mut self, _job: &'a KeyedEntry<'a, Job<'a>>, _cx: &mut Context<'_>) {
        self.platform = ShellPlatform::Any;
    }
}

/// Checks `defaults.run.shell` of `defaults` on `platform`.
fn check_defaults(
    defaults: &Field<'_, Defaults<'_>>,
    platform: ShellPlatform,
    cx: &mut Context<'_>,
) {
    if let Some(run) = defaults.value().and_then(|defaults| defaults.run.value()) {
        check(&run.shell, platform, cx);
    }
}

/// Reports `shell` unless it is a shell name of `platform`, ignoring case, a command with `{0}`,
/// or contains an expression. An invalid `shell` is skipped, because the parser has reported it
/// (`docs/differences.md#invalid-value-reported-once`).
fn check(shell: &Field<'_, Template<'_>>, platform: ShellPlatform, cx: &mut Context<'_>) {
    let Some(shell) = shell.value() else {
        return;
    };
    let value = shell.value();
    if value.contains("{0}") || contains_expression(value) {
        return;
    }
    let name = simple_lowercase(value);
    let available = shell_names(platform);
    if available.contains(&name.as_ref()) {
        return;
    }
    // The platform is named only when another platform has the shell. Otherwise the value is no
    // shell name at all, and the message says how to run another program
    // (`docs/differences.md#custom-shell-hint`).
    let known = shell_names(ShellPlatform::Any).contains(&name.as_ref());
    let on_platform = match platform {
        ShellPlatform::Windows if known => " on Windows",
        ShellPlatform::MacOrLinux if known => " on macOS or Linux",
        _ => "",
    };
    let hint = if known {
        String::new()
    } else {
        format!(
            ". use a command with {{0}} for a custom shell, as in {}",
            quote(&format!("{value} {{0}}"))
        )
    };
    cx.report_in(
        shell.source,
        &INVALID_SHELL_NAME,
        shell.span(),
        format!(
            "shell name {} is invalid{on_platform}. available names are {}{hint}",
            quote(value),
            sorted_quotes(available.iter().copied()),
        ),
    );
}

/// The reference's `getPlatformFromRunner`: the platform that the literal labels of `runner`
/// name, or [`ShellPlatform::Any`] when they name none or two. Labels from an expression name
/// none.
fn platform_of(runner: &Runner<'_>) -> ShellPlatform {
    let mut found = ShellPlatform::Any;
    for label in runner
        .labels
        .literal()
        .into_iter()
        .flat_map(|labels| labels.iter().flatten())
    {
        let label = simple_lowercase(label.value());
        let platform = if label == "windows" || label.starts_with("windows-") {
            ShellPlatform::Windows
        } else if label == "macos"
            || label == "linux"
            || label.starts_with("macos-")
            || label.starts_with("ubuntu-")
        {
            ShellPlatform::MacOrLinux
        } else {
            continue;
        };
        if found != ShellPlatform::Any && found != platform {
            // The `runner-label` rule reports the conflict.
            return ShellPlatform::Any;
        }
        found = platform;
    }
    found
}
