# ADR 0035: Command-line parser and environment

- Status: accepted
- Date: 2026-10-21
- Amends: ADR 0013

## Context

ADR 0003 makes workflint accept every actionlint flag in the syntax of Go's `flag` package. The roadmap planned clap behind a normaliser that rewrites `-flag` to `--flag` (`docs/research/06-rust-ecosystem.md` section 4). Since M2 the binary has had a hand-written parser that already gives Go's error messages, and its tests pass. clap prints its own help and errors, so matching Go's `flag provided but not defined: -x`, the usage that follows it, and the layout of Go's flag list would mean overriding most of what clap does.

ADR 0013 names `NO_COLOR`, `XDG_*`, and `GITHUB_ACTIONS` as the only environment variables that may change output. The reference also turns color off when `TERM` is `dumb`, and ADR 0014 lets `WORKFLINT_INSTALLED_FROM` change the second line of `-version`. ADR 0013 also asks for log output through `tracing`, buffered per file, and the reference's log lines contain the time each file took.

## Decision

Parser:

- The binary keeps its own Go-style parser. One table, `FLAGS` in `crates/workflint/src/flags.rs`, declares each flag: its name, its help text, its default, and the field of `Options` that it sets. The parser, the flag list of `-help`, and the defaults all come from that table. A task that adds a flag adds one entry and one field.
- No argument-parsing crate is added. Shell completions and the man page (M5.19, #256) are generated from the same table.

Environment:

- ADR 0013's list of environment variables that may change output gets `TERM` and `WORKFLINT_INSTALLED_FROM`. `TERM=dumb` turns color off as in the reference, unless `-color` is given. `WORKFLINT_INSTALLED_FROM` sets the second line of `-version`.
- `RAYON_NUM_THREADS` sets the number of threads and does not change output.

Logs:

- `workflint-linter` logs through `tracing`. Each file's events are in a span named `file`. The binary's layer holds a file's lines until its span closes and then writes them to stderr in one call. `INFO` events are the lines of `-verbose` and `DEBUG` events those of `-debug`, with the reference's prefixes.
- Log lines may contain times, as the reference's do, and their order between files depends on which file finishes first. The log is not part of the output that ADR 0013 keeps deterministic.

Color:

- `anstream` writes the colored output, so that the escape sequences work on Windows consoles that need them turned on or translated into console calls. The binary decides whether to print color itself, so `anstream` reads no environment variable for that decision.

## Consequences

The parser stays small and matches Go exactly, including its error messages. The help text and the completions of M5.19 need code of their own, and the table keeps them in step with the parser.

Two runs on the same input print the same stdout. The stderr of `-verbose` and `-debug` may differ between runs.

## Alternatives considered

- clap with an argument normaliser. Rejected, because the hand-written parser already matches Go, and clap's help and errors would need overriding to print Go's.
- `lexopt`. Rejected, because the parser loop is the part that `lexopt` would replace, and it is already written and tested.
- Writing log lines straight to stderr, one write per line. Rejected, because ADR 0013 asks for each file's lines together.
