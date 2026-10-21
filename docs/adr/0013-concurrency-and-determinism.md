# ADR 0013: Concurrency and determinism

- Status: accepted
- Date: 2026-10-15
- Amended by: ADR 0035

## Context

A linter in CI must give the same answer every time for the same input. The reference does not always do so. It visits jobs in Go map order, which is random, so cycle and conflict messages vary between runs. When several workflows use the same local action, the error from loading that action is reported in whichever file wins a race. Verbose log lines from parallel files interleave (`docs/research/01-parsing-ast.md`, `03-rules.md`).

## Decision

Parallelism:

- `workflint-linter` lints files in parallel with rayon, one task per file. Rules within a file run sequentially.
- Results are collected per file and printed in a fixed order. Files are ordered as given on the command line, or sorted by path when discovered. Diagnostics within a file are sorted as ADR 0007 describes.
- No async runtime. External tool processes run on a bounded pool (ADR 0012).
- Log output goes through `tracing` to stderr, buffered per file so that lines from different files do not interleave.

Shared caches:

- Parsed local actions and local reusable workflows are cached per project. Each entry is computed once, even when several files request it at the same time.
- A diagnostic about a cached file is reported at a deterministic place: in the cached file itself when it is part of the lint run, and otherwise at the first referencing `uses:` in sorted file and position order.

Determinism rules:

- Iteration over any map that affects output follows source order or sorted order, never hash order.
- Output does not depend on thread count, scheduling, the current date or time, the locale, or environment variables other than documented ones (`NO_COLOR`, `XDG_*`, `GITHUB_ACTIONS` for format detection).
- workflint makes no network requests.
- CI runs the conformance suite with one thread and with many threads and requires identical output.

## Consequences

Two runs on the same input always print the same bytes, so CI results are reproducible and snapshot tests stay stable.

Sorting and buffering hold each file's output until that file is done. Output streams per file instead of per diagnostic, which is fast enough for files of normal size.
