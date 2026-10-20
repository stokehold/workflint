# ADR 0030: Rule pages and the rules index

- Status: accepted
- Date: 2026-10-17
- Amends: ADR 0008
- Amended by: ADR 0034

## Context

ADR 0008 gives every rule a page, `docs/rules/<id>.md`, with at least one failing and one passing example that tests run. It also says that the registry generates the rules index in the docs, so that the index cannot drift from the registry. It does not say how a test reads a page, or how the index is generated.

A test can only run the examples if every page has the same structure. The index is a Markdown table in `docs/rules/README.md`, and the registry is Rust code in `workflint-rules`.

## Decision

- `docs/rules/README.md` describes the page format. A page has the title, a table of kind, category, and default severity, the description from the `RuleMeta`, a `## Why` section, failing examples, passing examples, and links to the `docs/differences.md` entries of the rule.
- A failing example is a `yaml` block followed by a `text` block with the one-line diagnostics of `test.yaml`. The test requires exactly those diagnostics. Each of them must come from the page's rule or from a rule whose page the page links. A passing example gives no diagnostic.
- The test lives in `workflint-rules` (`tests/rule_pages.rs`). It lints each example with the YAML layer, the workflow parser, and the rules, as the binary does, without the file system. It also checks the page's metadata and description against the registry, that every registered rule has a page, that every page belongs to a registered rule, and that each `docs_url` names its page.
- The rules index in `docs/rules/README.md` is checked in. The test renders the index from the registry and fails when the checked-in table differs, printing the table it expects. The index links a rule ID to its page when the page exists.

## Consequences

A rule task writes its page in the format the test reads, and a wrong expected diagnostic fails the build. A change to a message, a position, or the order of diagnostics shows up in the pages as well as in the conformance fixtures.

The index cannot drift from the registry, as ADR 0008 requires, but a change to the registry means pasting the new table from the test's failure message.

## Alternatives considered

- An `xtask` command that writes the index into `docs/rules/README.md`, with a test that the file is current. Rejected for now, because the test would need the same rendering code, and the command would save only the paste. It can be added if the paste becomes a chore.
- Render the index only in `workflint rules` (M9.6) and keep no table in the docs. Rejected, because the docs must list the rules for readers who do not run the binary.
