# ADR 0007: Diagnostic model

- Status: accepted
- Date: 2026-10-15
- Amended by: ADR 0032, which adds an optional help text

## Context

actionlint's error type holds a message, a file name, a line, a column, and a coarse kind. It has no end position, no severity, and no way to attach a fix. Its columns mix Unicode code points and bytes, so markers land on the wrong character after non-ASCII text (`docs/research/02-expressions.md`, `04-cli-output-distribution.md`).

workflint needs more from a diagnostic: severity (ADR 0008), a rule ID for ignore comments (ADR 0009), regions for SARIF and editors, and room for autofix after 1.0.

## Decision

`workflint-diagnostics` defines one `Diagnostic` type used by every crate:

- `rule`: a static reference to the rule's registry entry, which gives the fine-grained rule ID, the actionlint kind, the category, and the documentation URL.
- `severity`: `error`, `warning`, or `info`. The rule's default applies unless config overrides it.
- `message`: the text. It is byte-identical to actionlint's message wherever actionlint is correct (ADR 0003).
- `span`: the primary location, as a file ID plus a byte range. It always covers the token the message is about, not just its first character.
- `labels`: optional secondary spans with short notes, such as "anchor defined here" or "job `build` declared here".
- `fix`: optional. A list of text edits with an applicability of `safe` or `unsafe`. The type exists from M1. Rules start filling it after 1.0.
- `help`: optional. A hint that the actionlint-compatible formats do not print (ADR 0032).

Positions:

- Internally everything is a byte offset. Line and column are computed at output time from a per-file line index.
- Text output and JSON report 1-based lines and 1-based columns counted in Unicode code points, as actionlint intends.
- SARIF and the future language server use UTF-16 code units, as their specifications require. JSON output also carries byte offsets.

Ordering and duplicates:

- Diagnostics are sorted by file, then line, then column, then rule ID.
- Two diagnostics with the same file, line, column, and message collapse into one, matching actionlint.

Robustness:

- No input may make workflint panic. If a rule panics anyway, the driver catches it, reports an `internal-error` diagnostic for that file, continues with the other files, and exits with code 3.

Machine formats:

- JSON output has a versioned schema, published with each release.
- SARIF output is version 2.1.0, with a `rules` entry per rule ID, `helpUri`, and `partialFingerprints` that do not depend on line numbers, so results stay stable in GitHub code scanning when unrelated lines move.

## Consequences

The renderer underlines the full token. SARIF regions, editor ranges, and future autofix edits all derive from the same span. Code-point columns in text output match what actionlint users expect for ASCII files and fix the non-ASCII cases, which `docs/differences.md` records.
