# ADR 0024: Parse-stage rules

- Status: accepted
- Date: 2026-10-16
- Amends: ADR 0008

## Context

ADR 0008 makes every check a rule in one registry in `workflint-rules`, with one module per rule. Some diagnostics come from the parsers themselves: `workflint-yaml` reports `yaml-syntax-error`, `unknown-anchor`, `recursive-alias`, and `unused-anchor` while it builds the tree, and `workflint-syntax` and `workflint-expr` will report syntax errors the same way.

A `Diagnostic` needs the `RuleMeta` of its rule. `workflint-rules` depends on the parsing crates (ADR 0006), so the parsers cannot use metadata defined in `workflint-rules`.

## Decision

- A crate that reports diagnostics while parsing defines the `RuleMeta` of those rules itself, for example in `workflint_yaml::rules`. Today that is `workflint-yaml`; later `workflint-syntax` and `workflint-expr` do the same. These are parse-stage rules.
- The registry in `workflint-rules` lists the parse-stage rules next to its own rules, so that `workflint rules`, config, and the generated docs see one list of rules.
- A parse-stage rule has no module in `workflint-rules`.
- Each parse-stage rule gets a page in `docs/rules/<id>.md`, like every other rule. The pages arrive in M4.x with the other rule pages, as approved in pull request #40. Until then, `docs_url` points at the page that M4.x adds.

## Consequences

The parsers report diagnostics directly, with no conversion step between a parser error type and a `Diagnostic`. Rule IDs, kinds, and categories stay in one registry for users and config.

Rule metadata lives in more than one crate, and the registry has to list each parse-stage rule by name.

## Alternatives considered

- Parsers return their own error types, and `workflint-rules` turns them into diagnostics. Rejected, because each parser would need a second error model and a conversion that keeps spans and messages identical.
