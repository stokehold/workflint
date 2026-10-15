# ADR 0009: Inline ignore comments

- Status: accepted
- Date: 2026-10-15

## Context

The most requested actionlint feature after `action.yml` linting is a way to silence one finding where it occurs (rhysd/actionlint#237, 72 reactions). Today users can only add `-ignore` regexes against message text or per-path ignores in config. Both are far from the code and break when message text changes.

## Decision

workflint reads ignore comments from YAML comments, which the YAML layer keeps (ADR 0005).

Syntax:

```yaml
uses: my-org/deploy-action@main # workflint: ignore[require-pinned-uses] -- internal action, always tracks main
```

- The comment starts with `workflint:`, then `ignore[...]` with one or more comma-separated rule IDs or kinds, then an optional reason after `--`.
- A trailing comment applies to diagnostics whose primary span starts on the same line.
- A comment on its own line applies to the next non-comment line. If that line starts a mapping entry or sequence item, the comment covers the whole node, including nested lines.
- `# workflint: ignore-file[rule-id, ...]` anywhere in a file applies to the whole file.

Checks on the comments themselves:

- An ignore comment that suppresses nothing produces an `unused-ignore` warning.
- An unknown rule ID or a malformed directive produces an `invalid-ignore` warning.
- Config can require a reason (`require-ignore-reason: true`).

Migration:

- workflint also honours comments of the form `# actionlint:ignore <rule-or-kind> -- reason`, so files prepared for actionlint-based tools keep working.

Scripts:

- Comments inside `run:` scripts are part of the script, not YAML comments. shellcheck's own `# shellcheck disable=SC2086` directives keep working, because workflint passes the script to shellcheck unchanged apart from expression placeholders.

## Consequences

Ignores sit next to the code they affect and name a stable rule ID. Stale ignores show up as warnings instead of piling up.

Ignore comments require M1's comment-preserving YAML layer, and the feature ships in M9.
