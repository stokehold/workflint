# ADR 0033: Platform data updates before M7

- Status: accepted
- Date: 2026-10-20
- Amends: ADR 0011

## Context

ADR 0011 says that until M7, the data in `workflint-data` is ported from the reference's tables so that output matches the reference. M7.1 to M7.3 add the generators that refresh the data from GitHub's sources.

The reference's tables are out of date. The M4 review found runner labels, permission scopes, an activity type, and context properties that GitHub documents and the reference reports as unknown, such as `ubuntu-26.04`, `vulnerability-alerts`, and `job.workflow_sha`. It also found that `windows-latest` and `macos-latest` now run newer images than the reference says, so `[windows-latest, windows-2025]` is reported as a conflict. Release 0.1.0 comes before M7, so a workflint that keeps the reference's tables until then would report errors on valid workflows.

## Decision

- Before M7, a hand-written update may change or extend a ported table in `workflint-data` when GitHub's documentation (docs.github.com) or the GitHub changelog (github.blog/changelog) confirms the fact. Third-party mirrors do not count.
- The data module names each source page and the date it was checked.
- Each update is a difference from the reference, with an entry in `docs/differences.md` and allow-list records where it changes the output of a fixture or of the corpus (ADR 0023).
- The data tests compare each table with the reference's table plus a written list of the changes, so that a change outside the list fails them.
- When two GitHub sources disagree, or a label is moving from one image to another, the label can run on either image, so that no valid workflow gets an error.
- The M7 generators replace the hand-written updates.

## Consequences

The data matches what GitHub documents on the date in each module, and workflint no longer reports the new labels, scopes, activity type, and properties as errors. The output of some fixtures and corpus files differs from the reference, and each change is listed in `docs/differences.md`.

Hand-written data goes stale as GitHub changes. Each update records its check date, so a reader knows how old it is.

## Alternatives considered

- Keep the reference's tables until M7. Rejected, because 0.1.0 would report errors on workflows that GitHub accepts.
- Let users add the missing facts in the config file (M7.10). Rejected as the only fix, because every user would have to find and add the same facts.
