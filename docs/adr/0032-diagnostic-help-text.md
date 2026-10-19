# ADR 0032: Diagnostic help text

- Status: accepted
- Date: 2026-10-19
- Amends: ADR 0007

## Context

An undefined context, property, or function is usually a typo, such as `gihub` for `github` or `always` for `always()`. actionlint's message lists every available name, but leaves the reader to spot the right one in a list of eleven contexts or thirteen functions.

A suggestion inside the message would change actionlint's text, and the conformance suite compares messages byte for byte (ADR 0003). Every fixture with a typo would need an allow-list record.

## Decision

- `Diagnostic` gets an optional `help` field: a hint on how to fix the problem, such as `did you mean "github"?`. `Diagnostic::with_help` sets it, and `Context::report_with_help` reports a diagnostic with it.
- The default and one-line formats do not print `help`, so their output stays byte-identical to actionlint's. The JSON, SARIF, and GitHub formats (M5.8) print it.
- The `undefined-variable`, `undefined-property`, and `undefined-function` errors suggest the closest known name: a context in scope, a property of the object type, or a built-in function. A variable named like a function that takes no arguments, such as `always`, suggests the call. The distance counts characters inserted, deleted, or replaced, with a swap of two adjacent characters as one edit. A suggestion must be within a third of the length of the name, and at least one edit, so unrelated names get none.

## Consequences

Other rules can attach help text without touching their messages, which keeps the actionlint-compatible output stable. Until M5.8 the help text is visible only through the library API.
