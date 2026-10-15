# ADR 0004: Parity target and conformance testing

- Status: accepted
- Date: 2026-10-15

## Context

"Same behaviour as actionlint" needs a fixed reference, or the target moves with every upstream commit. Agents also need an oracle that tells them, without human judgement, whether a ported check is correct.

actionlint ships about 230 error fixtures (`testdata/examples`, `testdata/err`, `testdata/ok`), 30 multi-file projects, format fixtures, and 52 rendered outputs in `docs/checks.md`. It also ships a 1,503-file archive of real-world workflows that none of its tests use.

## Decision

The parity target is rhysd/actionlint at commit `011a6d15e749bb3f2d771eed9c7aa0e7e3e10ee7` (main on 2026-04-19, v1.7.12 plus unreleased fixes). This document calls it "the reference".

- We vendor the reference's `testdata/` into `tests/conformance/actionlint/` under its MIT license, with the license and commit recorded next to it.
- A test runner, built with `datatest-stable`, runs every fixture through workflint and compares the result with the fixture's expected errors, using the same rules as actionlint's own test code (exact line or `/regex/` per error). Fine-grained rule IDs are mapped back to actionlint kinds for this comparison.
- The rendered examples in the reference's `docs/checks.md` become snapshot tests.
- `cargo xtask oracle` builds the reference with a pinned Go toolchain, runs it and workflint over the fixtures and the 1,503-file corpus, and diffs the outputs. CI runs it on every pull request that touches `crates/`.
- `tests/conformance/allowed-differences.toml` lists every expected difference, keyed by an anchor in `docs/differences.md`. A diff that matches no entry fails the build.
- `docs/parity-checklist.md` lists every check, flag, and output feature of the reference. An item is done when its fixtures pass and the box is ticked in the same pull request.

Newer features from other projects are not part of the parity target. Each one enters through the roadmap as a deliberate change, recorded in `docs/differences.md` if it changes output.

## Consequences

Agents can work test-first. They pick a checklist item, run its fixtures, and make them pass. Reviewers judge the allow-list entries instead of re-deriving actionlint's behaviour.

CI needs a Go toolchain for the oracle job. The reference stays frozen, so upstream fixes after `011a6d1` reach workflint only through the roadmap.
