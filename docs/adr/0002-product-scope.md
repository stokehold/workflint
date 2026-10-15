# ADR 0002: Product scope

- Status: accepted
- Date: 2026-10-15

## Context

actionlint is the most used static checker for GitHub Actions workflows. Its last commit was in April 2026, its data generators have failed since March 2026, and 154 issues and 55 pull requests are open (see `docs/research/05-issues.md`). Users are blocked by valid new workflow syntax that it rejects, and they ask for features it does not have: linting `action.yml`, inline ignore comments, per-rule control, and severities.

A linter for GitHub Actions can aim at correctness (will this workflow run as written?) or at security hardening (could an attacker abuse this workflow?). The two goals pull in different directions. Correctness checks must have almost no false positives to stay enabled in CI. Hardening checks are opinions about risk, and many teams disagree with them.

## Decision

workflint is a correctness linter for GitHub Actions workflows and action metadata files.

In scope:

- Every check actionlint performs, with its bugs fixed (ADR 0003, ADR 0004).
- New checks for mistakes that make a workflow fail or misbehave on GitHub: unknown keys, invalid values, type errors in `${{ }}` expressions, unavailable contexts, wrong references between jobs, steps, and reusable workflows, and outdated platform data.
- Linting `action.yml` and `action.yaml` files, including composite actions.
- The security checks actionlint already has (script injection from untrusted inputs, hard-coded credentials), with better data.
- Opt-in rule categories (ADR 0008) for pedantic, style, and policy checks such as requiring SHA-pinned actions, explicit `permissions`, or `timeout-minutes`.

Out of scope for 1.0:

- Security auditing beyond actionlint's existing checks, for example taint tracking across jobs, cache poisoning analysis, or repository-level posture checks.
- Network access during a normal run. An opt-in online mode is planned after 1.0 (`docs/roadmap.md`).
- Running workflows or emulating the runner.

## Consequences

Before a rule is added, its author decides whether it finds a real mistake or enforces a preference. Mistakes can be on by default. Preferences go into an opt-in category. This keeps the default output close to what actionlint users see today, plus real bugs.

Users who want deep security auditing will run a dedicated tool next to workflint. workflint's SARIF output must therefore coexist cleanly with other tools in GitHub code scanning (ADR 0007).
