# ADR 0034: Rules of the driver

- Status: accepted
- Date: 2026-10-20
- Amends: ADR 0008, ADR 0030

## Context

ADR 0007 makes a panic in the lint of a file an `internal-error` diagnostic on that file. The driver in `workflint-linter` reports it, so by the convention of ADR 0024 and ADR 0028 its `RuleMeta` lives in `workflint-linter` and the registry lists it.

ADR 0008 puts the one registry in `workflint-rules`. `workflint-linter` depends on `workflint-rules`, so that registry cannot list a rule of `workflint-linter` without a dependency cycle. The registry of `workflint-rules` also feeds the conformance tooling, which requires an actionlint kind and a parity checklist row for every rule in it. `internal-error` has neither, because a panic stops actionlint.

## Decision

- A rule that the driver reports keeps its `RuleMeta` in `workflint_linter::rules`. `internal-error` is the only one.
- `workflint_linter::registry` lists every rule: those of `workflint_rules::registry` and those of `workflint_linter::rules`, sorted by rule ID. The binary, the config, and the rule pages test use it.
- `workflint_rules::registry` stays. It lists the rules of `workflint-rules` and of the crates below it, each with an actionlint kind, and the conformance tooling reads it.
- `internal-error` has the kind `internal-error`, defined in `workflint_linter::rules`. It is not in `workflint_rules::kind::ALL`, which holds the reference's kinds only.
- The rule pages test moves to `workflint-linter` (`tests/rule_pages.rs`), so that it sees every registered rule. The rest of ADR 0030 holds.

## Consequences

`docs/rules/internal-error.md` and the rules index list the rule, and the parity checklist does not. Code that needs every rule ID, such as config validation in M5.2 or `workflint rules`, reads `workflint_linter::registry`. Reading `workflint_rules::registry` instead misses the rules of the driver.

## Alternatives considered

- Put the `RuleMeta` of `internal-error` in `workflint-rules` or `workflint-diagnostics`, so that the one registry lists it. Rejected, because the crate that reports a diagnostic defines its rule (ADR 0024), and the conformance tooling would need an exception for a rule without an actionlint kind.
- Move the whole registry to `workflint-linter`. Rejected, because the tests of `workflint-rules` and the conformance tooling look rules up by ID and compare them by address, and a dev-dependency of `workflint-rules` on `workflint-linter` would build `workflint-rules` twice.
- Give `internal-error` the kind `syntax-check`. Rejected, because it is not a syntax error, and disabling the kind in config would hide bugs.
