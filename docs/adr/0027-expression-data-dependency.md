# ADR 0027: Expression data dependency

- Status: accepted
- Date: 2026-10-16
- Amends: ADR 0006

## Context

ADR 0006 lets `workflint-expr` depend only on `workflint-diagnostics`. ADR 0011 puts all platform data in `workflint-data` and forbids other crates from hard-coding it.

The expression checker reads four tables of platform data: the types of the global contexts such as `github` and `runner`, the signatures of the built-in functions, which contexts each workflow key allows, and the context properties that hold untrusted input. Under ADR 0006, `workflint-expr` cannot read them from `workflint-data`. It would have to hard-code them, which ADR 0011 forbids, or take them as arguments on every call, so that `workflint-rules` and every test of `workflint-expr` pass the same tables in.

## Decision

- `workflint-expr` depends on `workflint-data`. `workflint-data` still depends on no workspace crate, so the graph has no cycle.
- In ADR 0006's crate table, `workflint-data` moves above `workflint-expr`, so dependencies still point down the table.
- `workflint-data` describes expression types with its own small type, `TypeDesc`: any, null, number, bool, string, arrays, and objects that are strict, loose, or a map with an element type. `workflint-expr` converts a `TypeDesc` to its own `Ty`, so `workflint-data` never depends on `workflint-expr`.
- The tables are `static` data, so reading them allocates nothing. `workflint-data` is `no_std`, so the compiler rejects any use of the file system, the environment, or the clock in it.

## Consequences

The expression checker finds its tables without help from its callers, and its tests need no setup. M3.4, M3.6, and M3.7 add the function signatures, availability, and untrusted input tables to `workflint-data`, and the M7 generators replace all four like every other dataset.

A change to `workflint-data` rebuilds `workflint-expr`. A test in `workflint-data` fails if its manifest gains a workspace dependency, which would risk a cycle.

## Alternatives considered

- Pass the tables from `workflint-rules` into each call of the checker. Rejected, because every caller and every test of `workflint-expr` would pass the same tables, and nothing would stop a caller from passing other ones.
- Keep the expression tables in `workflint-expr`. Rejected, because ADR 0011 keeps platform data in one crate, which the generators and the daily refresh write to.
