# ADR 0016: Agent-driven development process

- Status: accepted. The "Work items" section and the roadmap-ticking part of "Definition of done" are superseded by ADR 0020.
- Date: 2026-10-15

## Context

AI coding agents write most of workflint's code, and the maintainer reviews every pull request. Agents work fast but lose context between sessions, make up behaviour when no test pins it down, and add files and abstractions faster than a reviewer can check them. A codebase built this way can grow large without growing useful, with test files named after review rounds, near-duplicate helpers, and data tables copied between rules.

## Decision

Work items:

- Work comes from `docs/roadmap.md`. Each task is sized for one pull request and names its milestone, crate, checklist items, and tests.
- One task per pull request. A pull request that changes an interface between crates does only that, before the pull requests that use it.

Test first:

- An agent writes or enables the failing test before the implementation. For parity work, the test is the conformance fixture from the reference (ADR 0004). For new behaviour, it is a unit or snapshot test plus a rule documentation example (ADR 0008).
- The reference and the conformance suite are the source of truth for actionlint behaviour. Agents check the reference's code and fixtures instead of guessing.

Definition of done:

- `cargo fmt --check`, `cargo clippy --workspace --all-targets` with warnings denied, and `cargo nextest run --workspace` pass.
- The conformance suite passes, and any new difference has an entry in `docs/differences.md` and the allow-list.
- Checklist items in `docs/parity-checklist.md` are ticked in the same pull request.
- Documentation affected by the change is updated in the same pull request.
- The pull request description states how the change was verified.

Rules against sprawl:

- Each rule lives in its own module in `workflint-rules`. Platform facts live only in `workflint-data`.
- Tests are named after the behaviour they test, never after a review round or an issue number alone.
- No new dependency without a sentence in the pull request on why it is needed. `cargo deny` must pass.
- No `TODO` without a linked issue. Dead code gets deleted, not commented out.
- Helpers used once stay inline. A shared helper needs at least two callers.

Prose:

- Agents load the `unslop` skill (`.claude/skills/unslop`) before writing or editing prose: documentation, ADRs, code comments, commit messages, and pull request text.

Commits:

- Commit messages follow Conventional Commits, which release-plz uses for the changelog (ADR 0014).

## Consequences

Reviewers can check each pull request against a named task, a failing-then-passing test, and the checklist. The conventions in `AGENTS.md` summarise this ADR for agents and link back to it.

The process is slower per pull request than letting agents work freely. It prevents the cleanup work that unreviewable growth would cause later.
