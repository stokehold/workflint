# ADR 0019: Performance targets and tracking

- Status: accepted
- Date: 2026-10-15
- Amended by: ADR 0028, ADR 0038

## Context

"Faster than actionlint" only means something with numbers and a way to keep them. On an Apple M1 Pro, the reference lints the 1,503-file corpus in 0.27 to 0.31 seconds using 36 to 44 MB of memory with external tools off (`docs/research/00-baseline.md`). With shellcheck enabled, 50 files take about 18 seconds, and the full corpus hung on macOS.

## Decision

Targets, measured against the reference built from commit `011a6d1` on the same machine, in the same job:

| Scenario | Target |
|---|---|
| 1,503-file corpus, external tools off | Wall time at most one third of the reference. Peak memory at most half of the reference. |
| 1,503-file corpus, shellcheck on | Wall time at most one fifth of the reference. No run ever hangs. Every subprocess has a timeout. |
| One small workflow (`bench/small.yaml`), end to end | Under 10 ms wall time. |

The targets apply from release 0.1.0 onward. Before 0.1.0 they are tracked but do not block merges.

Tracking:

- `cargo xtask bench` builds workflint in release mode and the reference with the pinned Go toolchain, runs both with hyperfine over the corpus and the benchmark files, measures peak memory, and writes the results as JSON and as a Markdown table.
- A nightly CI job runs `cargo xtask bench` and appends the JSON to a history file on a `benchmarks` branch, so trends are visible over time. If a target is missed, or wall time regresses by more than 10% against the previous night, the job opens or updates an issue labelled `performance`.
- Each crate has micro-benchmarks written with `divan` for its hot paths: YAML parsing, expression parsing and checking, and the rule pass over one file. CI runs them on pull requests through CodSpeed, which measures instruction counts instead of wall time, so results are stable on shared runners and regressions show up as a pull request comment.
- The release workflow runs `cargo xtask bench` and refuses to publish when a target is missed.
- shellcheck is pinned to one version in all benchmark jobs.

Benchmarks are set up in M1, before most of the code exists, so that every later milestone is measured from its first pull request.

## Consequences

The speed claim is checked on every night and every release. Regressions show up within a day and are tied to a pull request.

The nightly job needs a Go toolchain and shellcheck, and its wall-time numbers vary between runners. Comparing against the reference in the same job, rather than against fixed numbers, cancels most of that variation.
