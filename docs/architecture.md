# Architecture

This document describes how workflint is put together: the crates, how a file flows through them, and the main types. The ADRs in `docs/adr/` record why. Read this before working on any crate.

This document describes the target design. Each crate is created in the milestone that first needs it (ADR 0006), so `crates/` holds only the crates built so far, and `docs/roadmap.md` says when the others arrive.

## Crates

```text
workflint                      binary: CLI, output formats
├── workflint-gotemplate       Go text/template subset for -format
└── workflint-linter           discovery, config, parallel driver, caches, external tools
    └── workflint-rules        rule registry and rules
        ├── workflint-data     generated GitHub platform data
        └── workflint-syntax   workflow and action.yml AST and parser
            ├── workflint-yaml YAML parser fork, span tree, comments, source maps
            └── workflint-expr ${{ }} lexer, parser, types, checker

workflint-diagnostics          Diagnostic, Span, Severity, renderers; every crate except
                               workflint-data and workflint-gotemplate depends on it
```

ADR 0006 has the full dependency table and the boundary rules. Only `workflint-linter` and `workflint` touch the file system, spawn processes, or read environment variables. Every other crate is a pure function of its inputs, so it compiles to WASM and needs no mocks in tests.

Parse-stage rules are an exception to "rules live in `workflint-rules`". `workflint-yaml`, and later `workflint-syntax` and `workflint-expr`, report diagnostics while parsing and define the `RuleMeta` of those rules themselves. The registry in `workflint-rules` lists them with its own rules (ADR 0024).

## Outside the crates

- `xtask/` is the package behind `cargo xtask`, the developer commands. `cargo xtask --help` lists them. Its `conformance` test target is the conformance suite, and `xtask/goyaml-dump/` is the Go program that `cargo xtask yaml-parity` builds to dump go-yaml's tree. It is never published (ADR 0006).
- `fuzz/` holds the cargo-fuzz targets. It is a Cargo workspace of its own, with its own `Cargo.lock` and a pinned nightly toolchain, because `cargo fuzz` needs nightly (ADR 0025). It is never published.

## How a run works

1. The CLI parses arguments (Go-style and modern flags, ADR 0003) and builds a `LintRequest`: the files or stdin to lint, the config override, the output format, and the failure threshold.
2. `workflint-linter` finds each file's project root and loads its config (ADR 0010). With no file arguments, it lists `.github/workflows/*.y{a,}ml` in the project and, from M8, `action.y{a,}ml` files.
3. For each file, in parallel (ADR 0013):
   1. `workflint-yaml` decodes the bytes and parses the first YAML document into a span tree. The first YAML syntax error stops the file: it becomes one diagnostic, and no later step runs on that file, as in actionlint (ADR 0022).
   2. `workflint-syntax` walks the tree and builds the typed AST: `Workflow` or `ActionMetadata`. Unknown keys, wrong value types, and missing required keys become `syntax-check` diagnostics. Every string that contains `${{ }}` is parsed into expression ASTs at this point.
   3. `workflint-rules` runs every enabled rule over the AST in a fixed order. Rules use `workflint-expr` to type-check expressions against contexts built from the workflow, and `workflint-data` for platform facts. Rules that need other files (local actions, local reusable workflows) ask the linter's per-project cache.
   4. Rules that need shellcheck or a Python linter queue script jobs. The external tool pool runs them (ADR 0012), and their findings are mapped back to workflow positions through the script's source map, which the YAML layer builds on demand.
4. The linter applies ignore comments (ADR 0009), config severity overrides, and `-ignore` patterns, then sorts and deduplicates (ADR 0007).
5. The CLI renders the diagnostics in the chosen format and exits with 0, 1, 2, or 3.

## Main types

| Type | Crate | Role |
|---|---|---|
| `SourceFile` | diagnostics | Path, text, and a line index for byte offset to line and column conversion. |
| `Span` | diagnostics | File ID and byte range. Every position in workflint is a `Span`. |
| `Diagnostic` | diagnostics | Rule, severity, message, primary span, labels, optional fix (ADR 0007). |
| `RuleMeta` | diagnostics | Static rule metadata: kebab-case ID, kind, category, default severity, docs URL. |
| `yaml::Document` and `yaml::Node` | yaml | Arena-allocated span tree with tags, scalar styles, anchors, and comments. `Node::source_map` builds a scalar's source map on demand (ADR 0022). |
| `Workflow`, `Event`, `Job`, `Step`, `ActionMetadata` | syntax | Typed AST that borrows from the `yaml::Document`. Keys and strings are `Str` leaves, which keep their YAML node and span. A key's value is a `Field<T>` (absent, invalid, or valid) or, where it may be a `${{ }}` expression, a `Value<T>` (absent, invalid, a literal `T`, or an `Expr`). Collections keyed by name are `Keyed<T>`, in source order with case-insensitive lookup. Collections are shared through `Arc`, so an anchored subtree is built once. |
| `Expr`, `Ty` | expr | Expression AST and types. `Ty` is an immutable enum shared through `Arc`. |
| `Rule` trait and `Registry` | rules | A rule declares its metadata and implements visitor callbacks for the nodes it cares about. |
| `Project`, `Config`, `Linter` | linter | Project root, loaded config, caches, and the parallel driver. |

## Adding things

- A new check is a new rule module in `crates/workflint-rules/src/rules/`, a registry entry, a page in `docs/rules/<id>.md`, and tests (ADR 0008, ADR 0016). A check that a parser reports keeps its `RuleMeta` in the parsing crate instead of a rule module (ADR 0024).
- A new platform fact goes into `workflint-data`, either through a generator in `xtask` or, until M7, into the ported tables. Rules never hard-code platform facts.
- A new output format is a renderer in the `workflint` crate that consumes `Diagnostic` values. It never re-derives positions.
- A new config key gets a field in the config types in `workflint-linter`, a schema update (generated), and an entry in `docs/config.md`.

## Testing layers

| Layer | Where | What |
|---|---|---|
| Unit tests | next to the code | Lexer tokens, type rules, single rule behaviour. |
| Snapshot tests | each crate's `tests/` | Rendered output for representative inputs, with `insta`. |
| Conformance suite | `tests/conformance/`, run by `xtask/tests/conformance.rs` | The reference's fixtures and workflint's own through the real CLI, compared with expected errors (ADR 0004). |
| YAML parity | `cargo xtask yaml-parity` | The `workflint-yaml` tree against go-yaml's on fixtures and the 1,503-file corpus: node kinds, positions, decoded values, anchors, and aliases. |
| Oracle diff | `cargo xtask oracle` | workflint against the reference binary on fixtures and the 1,503-file corpus. |
| Rule examples | `docs/rules/*.md` | Every documented example runs as a test. |
| Fuzzing | `fuzz/` | Parsers and the full lint of one file must never panic (ADR 0018). |
| Benchmarks | `cargo xtask bench`, `divan` | Performance targets (ADR 0019). |

## Planned after 1.0

The design keeps room for these without rework:

- Autofix: rules fill `Diagnostic::fix`. Edits are byte ranges on the original source, so the comment-preserving span tree is all that is needed.
- Language server: `workflint lsp` reuses `workflint-linter` with an in-memory file system and converts spans to UTF-16 ranges.
- WASM playground: compiles everything except `workflint-linter`'s file and process access, which sits behind a small trait.
- Online mode: a resolver for remote action metadata and reusable workflows behind the same cache interface as local files.
