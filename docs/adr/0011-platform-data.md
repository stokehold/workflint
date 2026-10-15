# ADR 0011: GitHub platform data

- Status: accepted
- Date: 2026-10-15

## Context

Much of what a workflow linter knows is data about GitHub: runner labels, permission scopes and their allowed levels, webhook events and activity types, context properties, valid workflow and action keys, deprecated Node runtimes, and the inputs and outputs of popular actions. GitHub changes this data every few weeks.

actionlint keeps some of it in hand-written tables and generates the rest with weekly jobs. The generators have failed silently since March 2026, and the hand-written tables were not updated. Stale data is the largest group of user complaints (`docs/research/05-issues.md`, about 500 reactions), for example `ubuntu-26.04`, parallel steps, `$/` references, and new permissions.

## Decision

Sources:

- `crates/workflint-data` holds all platform data. No other crate hard-codes platform facts.
- Data is generated from official, machine-readable sources where they exist. These include GitHub's workflow schema in `actions/languageservices`, the action metadata schema and runtime list in `actions/runner`, the webhook schemas in `octokit/webhooks`, the runner image documentation, and the `action.yml` files of popular actions at each major version still in use.
- Each generated file records its source URL, the source commit or retrieval date, and the generator version.

Generation:

- `cargo xtask codegen <dataset>` runs a generator and writes Rust source into `crates/workflint-data/src/generated/`. The generated files are checked in, so builds never need the network. This includes `cargo install` builds from crates.io.
- Until M7, the data is ported from the reference's tables so that output matches the reference (ADR 0004). M7 replaces the ported tables with generators.

Refresh:

- A scheduled workflow runs every generator daily. When the output changes, it opens or updates one pull request per dataset.
- When a generator fails, the workflow opens or updates an issue labelled `data-refresh-failed`. A failure is never silent.
- Merging a data pull request triggers a patch release through release-plz (ADR 0014).

Determinism:

- workflint never downloads data at run time. The same version, input, and config always give the same result.
- Results never depend on the current date. Dates in the data, such as a runner image's retirement date, are compared with the data snapshot's date, not the clock.

User extensions:

- Config can add runner labels, configuration variables, secrets, and per-action overrides, such as declaring that an action has dynamic outputs. An outdated workflint binary therefore never blocks a user who knows a fact the data lacks.

## Consequences

Data updates reach users within days without anyone editing tables by hand. Each data change goes through review as a normal pull request.

Generators depend on upstream formats that can change. The failure issue makes that visible, and a maintainer fixes the generator.
