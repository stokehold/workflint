# ADR 0031: Agents enable auto-merge

- Status: accepted
- Date: 2026-10-19
- Amends: ADR 0020

## Context

ADR 0020 said agents never merge pull requests and never enable auto-merge, so the maintainer merged each one by hand. With several agent sessions running in parallel, finished pull requests wait in the queue for that click, and each one that waits makes the others more likely to conflict with `main`.

The repository already allows auto-merge, and the `main` ruleset decides what a merge needs: squash only, every required status check green, and every review thread resolved. It requires no approving review. Agents act as the maintainer's GitHub account, so they could not approve their own pull requests anyway.

## Decision

- When an agent marks a pull request ready for review, it also enables auto-merge with a squash: `gh pr merge --auto --squash`. GitHub merges the pull request once every required check passes.
- Agents still never merge directly, and never bypass the ruleset or a failing check.
- The maintainer holds a pull request by turning off auto-merge on it, or by leaving a review comment. The ruleset blocks the merge until every review thread is resolved.
- Agents enable auto-merge only after every "Decisions to review" question has an answer and the description no longer lists any.

## Consequences

Pull requests merge without waiting for the maintainer, so review can happen after the merge. A mistake that CI does not catch reaches `main`, and the fix is a follow-up pull request or a revert.

CI is now the only gate that always runs. A check that should block a merge has to be in the ruleset's required checks.

ADR 0020's plan for unattended agents, a separate bot identity with a required approval, still applies if the maintainer wants a review before every merge again.
