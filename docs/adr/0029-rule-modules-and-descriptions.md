# ADR 0029: Rule modules and rule descriptions

- Status: accepted
- Date: 2026-10-17
- Amends: ADR 0008, ADR 0016

## Context

ADR 0016 puts each rule in its own module in `workflint-rules`, and the glossary defines a rule as one check with a stable rule ID. Most of the reference's rules report several of workflint's rule IDs from one visitor. The `events` rule alone maps to more than a dozen IDs, such as `cron-too-frequent` and `activity-types-not-supported`, and it checks them in one walk over `on:`. The `job-needs` rule builds one graph of the jobs and reports both undefined and cyclic `needs` from it.

The linter creates the reference's rules in a fixed order (`linter.go:558`), and the order decides which rule reports an error from a shared cache of local actions or reusable workflows. That order is a list of reference rules, not of rule IDs.

ADR 0008 lists what a registry entry has: an ID, a kind, a category, a default severity, and a documentation page. SARIF output and `workflint rules` also need a short description of each rule ID. The reference describes each kind once, in the `desc` field of its rule.

## Decision

- `workflint-rules` has one module per reference rule in `src/rules/`, named after the rule's kind, such as `rules/events.rs`. The module has one `Rule` implementation and a `RuleMeta` static for each rule ID the rule reports.
- A check that is new in workflint and has no reference rule gets its own module and `Rule` implementation.
- `CREATION_ORDER` lists the reference rules in the order `linter.go` creates them. Each task that ports a rule adds its constructor there and keeps the order.
- `RuleMeta` has a `description` field: one line, without a final period, that says what the rule reports. The crate that defines a `RuleMeta` writes its description.
- The descriptions of the actionlint kinds live in `workflint_rules::kind`, byte-identical to the reference's `desc` strings.

## Consequences

A ported rule keeps the shape of the reference's code, with one walk over the nodes it checks and its shared state in one place. A reviewer can compare a module with one reference file.

A module may define many `RuleMeta` statics, and one `Rule` implementation reports all of them. Disabling one rule ID in config (M5.2) filters diagnostics by rule ID instead of skipping a visitor.

Every `RuleMeta` needs a description, so adding a rule ID means writing one more line.

## Alternatives considered

- One module per rule ID, each with its own `Rule` implementation. Rejected, because rule IDs of one reference rule share state such as the job graph of `job-needs`, and splitting them would mean several walks over the same nodes or a shared helper that every module calls.
- Keep the description of each rule ID only in the first line of its page in `docs/rules/<id>.md`. Rejected, because the binary would need to embed and parse the pages to fill SARIF output.
