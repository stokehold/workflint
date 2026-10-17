# ADR 0006: Workspace and crate boundaries

- Status: accepted
- Date: 2026-10-15
- Amended by: ADR 0025, ADR 0027, ADR 0028

## Context

Several agents will work in parallel. Crate boundaries enforce module boundaries at compile time, keep incremental builds fast, and let a WASM build reuse the core without the CLI. workflint also publishes to crates.io (ADR 0014), which requires every crate the binary depends on to be published.

## Decision

The repository is one Cargo workspace. Crates live in `crates/` and are created in the milestone that first needs them:

| Crate | Contents | Depends on |
|---|---|---|
| `workflint-diagnostics` | `Span`, `Diagnostic`, `Severity`, rule metadata, `Fix` and `TextEdit` types, the default and one-line renderers | none |
| `workflint-yaml` | vendored YAML parser fork, span tree, comments, scalar source maps (ADR 0005) | diagnostics |
| `workflint-expr` | `${{ }}` lexer, parser, AST, type system, semantic checker | diagnostics |
| `workflint-syntax` | workflow and action metadata AST and the parser that builds it from the YAML tree | yaml, expr, diagnostics |
| `workflint-data` | generated platform data: runner labels, permissions, events, contexts, popular actions (ADR 0011) | none |
| `workflint-rules` | rule registry and every rule | syntax, expr, data, diagnostics |
| `workflint-linter` | project discovery, config, parallel driver, caches, external tools | rules, syntax, yaml, diagnostics |
| `workflint-gotemplate` | Go `text/template` subset for `-format` | none |
| `workflint` | the binary: CLI parsing, output formats | linter, gotemplate, diagnostics |

`xtask/` holds developer commands (`cargo xtask codegen`, `cargo xtask oracle`, `cargo xtask bench`). It is never published.

Rules for boundaries:

- Dependencies point down the table. No cycles.
- Only `workflint-linter` and `workflint` may touch the file system, spawn processes, or read the environment. Lower crates take input as values. This keeps them usable from WASM and easy to test.
- A crate's public items are its interface to other crates. Keep them few and documented.

Publishing:

- Every crate except `xtask` is published to crates.io from 1.0, at the same version as the binary.
- Every crate other than `workflint` states in its README that it is an internal crate with no stability promise until the project says otherwise.
- Crates keep test corpora out of the published package with `include`, to stay under the 10 MB crate limit.

## Consequences

Agents can own a crate each during M2 and M3 without merge conflicts. A future library offering, for example a reusable expression checker, already has its own crate.

Moving a type between crates later costs a pull request that touches several `Cargo.toml` files. That is acceptable.
