# ADR 0008: Rule registry and default rule set

- Status: accepted
- Date: 2026-10-15
- Amended by: ADR 0024

## Context

actionlint has about 15 coarse error kinds and no way to turn a single check off except through a regex on message text. Its maintainer declined many useful checks because users could not disable them. A linter that enables every check by default produces so many findings that teams remove it from CI. On the 1,503-file corpus, enabling a broad set of policy rules raised the count from 6,836 findings to about 24,300, and 92% of the increase came from five policy rules (`docs/research/09-jactionlint.md`).

## Decision

Every check is a rule in one registry in `workflint-rules`. Each registry entry has:

- An ID: kebab-case, specific, and stable, such as `undefined-property` or `unknown-runner-label`. Renamed rules keep their old ID as an alias.
- A kind: the actionlint error kind the check belongs to (`syntax-check`, `expression`, `runner-label`, and so on), or a new kind for checks actionlint does not have. The conformance suite uses the kind to compare with actionlint fixtures.
- A category: `correctness`, `pedantic`, `style`, `security-strict`, or `policy`.
- A default severity and a default enabled state.
- A documentation page, `docs/rules/<id>.md`, with at least one failing and one passing example. Tests run those examples.

The default rule set:

- `correctness` rules are on by default. They cover everything actionlint checks plus mistakes that make a workflow fail or misbehave on GitHub.
- Inside `correctness`, a rule that can be wrong about valid code (a heuristic) defaults to `warning`. A rule that is right whenever it fires defaults to `error`.
- `pedantic`, `style`, `security-strict`, and `policy` rules are off by default. Config enables them by category or by ID.

Exit codes and failure:

- `--fail-on` takes `error` (the default), `warning`, `info`, or `never`. Exit code 1 means at least one diagnostic at or above that level.
- Without config, workflint's exit code on actionlint-checked files matches actionlint's, because every actionlint check is an `error`, except the heuristics `docs/differences.md` lists.

Config:

- `rules:` maps a rule ID, kind, or category to `off`, `info`, `warning`, or `error`, globally or for path globs (ADR 0010).
- `workflint rules` lists every rule with its category and default. `workflint rules <id>` prints its documentation.

Adding a rule:

- The pull request states the category and default severity, includes failing and passing examples, and reports the rule's hit count on the 1,503-file corpus.
- A new default-on rule needs a manual review of a sample of at least 20 corpus hits, recorded in the pull request, with no false positives.

## Consequences

The default output stays close to actionlint's, plus real bugs. Opinionated checks that upstream declined can exist without bothering anyone who did not ask for them.

Every rule needs a documentation page, which adds work per rule. The registry generates `workflint rules`, the config JSON Schema, and the rules index in the docs, so these cannot drift apart.
