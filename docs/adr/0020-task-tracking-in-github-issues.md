# ADR 0020: Task tracking in GitHub issues

- Status: accepted
- Date: 2026-10-15
- Supersedes: the "Work items" section and the roadmap-ticking item of "Definition of done" in ADR 0016

## Context

ADR 0016 made `docs/roadmap.md` the task list. Agents ticked a task's checkbox in the pull request that completed it. With about 90 tasks, that file mixes the plan (milestones, order, dependencies) with status, so every task pull request edits it, and status updates depend on agents remembering to tick a box. Outside contributors cannot see which tasks are free, and a task has no room for acceptance criteria or discussion.

GitHub can track issues per milestone, close an issue when the pull request that references it merges, and record "blocked by" relationships between issues natively.

## Decision

Where tasks live:

- Each roadmap task is one GitHub issue in `stokehold/workflint`, created from the task issue form. The issue body holds the goal, context, acceptance criteria, and what is out of scope.
- Each roadmap milestone is one GitHub milestone, named like `M1: Foundations`. The descriptions of M5 and M9 say which release closing them produces.
- `docs/roadmap.md` is the plan. It keeps milestones, goals, order, and dependencies, and links each task to its issue. It has no checkboxes and no status.
- Issues for a milestone are created when the previous milestone is nearly done, so that tasks reflect what earlier milestones taught. M1 and M6 issues exist from the start because they run in parallel.

Dependencies:

- Dependencies between tasks are GitHub issue dependencies ("blocked by"). A task is ready when it has no open blockers.

Claiming and finishing work:

- An agent picks an open issue in the current milestone that has no open blockers and no linked pull request.
- It claims the issue by opening a draft pull request with `Closes #N` in the description right away. The linked pull request on the issue shows that the task is taken.
- Merging the pull request closes the issue. Nobody updates status by hand.
- Agents never merge pull requests and never enable auto-merge. The maintainer reviews and merges. Nothing enforces this technically, because agents act as the maintainer's GitHub account. If agents start running unattended, a separate bot identity and a required approval will enforce it.

What stays in the repository:

- `docs/parity-checklist.md` stays a checklist in the repository, ticked in the pull request that makes its fixtures pass (ADR 0004). It is reference material for parity, not a task list.

Labels:

- Type: `feature`, `bug`, `docs`, `chore`, `parity`, `difference`.
- Area: `area: <crate or topic>`, one or more per issue.
- Automation: `data-refresh-failed`, `performance`, `dependencies`.
- Triage: `duplicate`, `invalid`, `question`, `wontfix`, `good first issue`, `help wanted`.

There are no priority labels. The roadmap holds the order.

## Consequences

Status updates itself when pull requests merge, and anyone can see which tasks are open, blocked, or taken. Task pull requests no longer edit `docs/roadmap.md`.

Issue bodies become part of the project's specification, so they need the same care as docs. The task issue form gives every task the same structure.
