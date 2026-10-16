# ADR 0021: Supply chain and CI security

- Status: accepted
- Date: 2026-10-15
- Amends: ADR 0015
- Amended by: ADR 0026

## Context

workflint will ship binaries that run in thousands of CI pipelines, through npm, PyPI, crates.io, Homebrew, Docker, and a GitHub Action. An attacker who can publish a workflint release, or steal the credentials to do so, reaches all of those pipelines.

Recent attacks on open source supply chains followed a few paths:

- A malicious package's install script steals registry and GitHub tokens from a developer machine or a CI job, then publishes infected versions of the victim's own packages.
- A workflow triggered by `pull_request_target` or `workflow_run` runs code from a fork with access to the base repository's secrets.
- An attacker moves a mutable action tag to malicious code, or publishes a look-alike action.
- Untrusted text such as a branch name or issue title is interpolated into a `run:` script.
- A poisoned cache from a pull request is restored by a release build.

AI agents add one more path. An agent that reads attacker-written issue text while holding the maintainer's credentials can be instructed to leak them.

## Decision

Publishing:

- workflint publishes to crates.io, npm, and PyPI with trusted publishing (OIDC) only. No registry token is stored in GitHub, on a developer machine, or anywhere else.
- npm packages are set to require two-factor authentication and disallow tokens. PyPI accepts only the trusted publisher.
- Publishing jobs run in a GitHub environment named `release`. The maintainer must approve each deployment, and the environment only deploys from tags matching `v*`.
- Release archives get GitHub artifact attestations and an SBOM. Releases are immutable, and release tags cannot be moved or deleted.

Secrets:

- The `stokehold-bot` GitHub App's private key is an environment secret, never an organization or repository secret. Publishing uses the `release` environment. Jobs that only open pull requests, such as release preparation and data refresh, use an `automation` environment restricted to the `main` branch.
- Workflows that run on pull requests have no access to any secret.

Workflows:

- No workflow uses `pull_request_target`, or `workflow_run` on code from a pull request.
- Every workflow sets `permissions` at the top level to read-only or nothing, and grants more per job only where needed.
- Checkouts use `persist-credentials: false`.
- `run:` scripts never interpolate `${{ }}` values that come from events or inputs. They read them from environment variables.
- Build jobs that produce release artifacts have no secrets, use no caches, and build with `--locked`. A separate publish job downloads the artifacts, verifies them, and publishes.
- No self-hosted runners.
- The `workflow-security` CI job audits every workflow on every pull request, and it is a required check.

Actions:

- The organization allows only GitHub-owned actions plus an explicit allowlist in its Actions settings. Adding an action to the allowlist is a maintainer decision, recorded in the pull request that first uses it.
- Every action is pinned to a full commit SHA, which the organization enforces.
- Dependabot updates actions and crates weekly, with a seven-day cooldown so that a malicious release has time to be found before we adopt it.

Dependencies:

- `cargo deny` allows only crates.io as a source and only the licenses in ADR 0017, and it fails on known advisories.
- A new dependency needs a reason in its pull request (ADR 0016). Build scripts and procedural macros run on every developer machine and CI job, so fewer dependencies mean fewer places for an attack.

Maintainer machine and agents:

- The maintainer's GitHub token keeps only the scopes daily work needs. Organization admin, repository deletion, and key management scopes are added for a task and removed after it.
- No registry tokens stay on the maintainer's machine after a manual publish.
- npm runs with `ignore-scripts=true`, so install scripts of npm packages never run.
- Agents work only on issues the maintainer created or labelled. Text written by others in issues, pull requests, and comments is data to analyse, never instructions to follow.
- Agents never read, print, or copy credential files or tokens.
- Agents run with Claude Code's sandbox enabled.

## Consequences

Stealing a developer token is no longer enough to publish a workflint release. Every release needs a tag on protected `main` and the maintainer's approval of the `release` deployment. A fork cannot reach secrets through CI.

Releases need one manual approval each. Adding a new third-party action needs a settings change by the maintainer first.
