# ADR 0001: Record architecture decisions

- Status: accepted
- Date: 2026-10-15

## Context

AI agents will write most of workflint, and a human maintainer will review it. Agents start each session without memory of earlier sessions. They need a written record of what was decided, why, and what was rejected, or they will reopen settled questions and drift from the plan.

## Decision

We record each significant decision as an architecture decision record (ADR) in `docs/adr/`.

- Files are named `NNNN-short-title.md` with a four-digit, never-reused number.
- Each ADR has a status (`proposed`, `accepted`, `superseded by ADR NNNN`, or `deprecated`), a date, and the sections Context, Decision, and Consequences. An Alternatives section is optional.
- An accepted ADR is not edited for substance. To change a decision, write a new ADR that supersedes the old one and update the old one's status line.
- Small fixes such as typos and broken links are fine at any time.
- `docs/adr/README.md` lists every ADR with its status.
- A pull request that changes behaviour covered by an ADR either follows the ADR or includes a superseding ADR.

The research reports in `docs/research/` hold the evidence behind these decisions. When a research report and an ADR disagree, the ADR wins.

## Consequences

Agents and reviewers can check a change against the ADR index before writing code. Decisions cost a little more to make, because each one needs a short document.
