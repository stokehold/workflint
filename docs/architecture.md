# Architecture

This document describes how workflint is put together: the crates, how a file flows through them, and the main types. The ADRs in `docs/adr/` record why. Read this before working on any crate.

Most of the code does not exist yet. This document describes the target design. Each section names the milestone that builds it (see `docs/roadmap.md`).

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

## How a run works

1. The CLI parses arguments (Go-style and modern flags, ADR 0003) and builds a `LintRequest`: the files or stdin to lint, the config override, the output format, and the failure threshold.
2. `workflint-linter` finds each file's project root and loads its config (ADR 0010). With no file arguments, it lists `.github/workflows/*.y{a,}ml` in the project and, from M8, `action.y{a,}ml` files.
3. For each file, in parallel (ADR 0013):
   1. `workflint-yaml` parses the bytes into a span tree. YAML syntax errors become diagnostics, and parsing recovers where it can.
   2. `workflint-syntax` walks the tree and builds the typed AST: `Workflow` or `ActionMetadata`. Unknown keys, wrong value types, and missing required keys become `syntax-check` diagnostics. Every string that contains `${{ }}` is parsed into expression ASTs at this point.
   3. `workflint-rules` runs every enabled rule over the AST in a fixed order. Rules use `workflint-expr` to type-check expressions against contexts built from the workflow, and `workflint-data` for platform facts. Rules that need other files (local actions, local reusable workflows) ask the linter's per-project cache.
   4. Rules that need shellcheck or a Python linter queue script jobs. The external tool pool runs them (ADR 0012), and their findings are mapped back to workflow positions through the scalar source maps.
4. The linter applies ignore comments (ADR 0009), config severity overrides, and `-ignore` patterns, then sorts and deduplicates (ADR 0007).
5. The CLI renders the diagnostics in the chosen format and exits with 0, 1, 2, or 3.

## Main types

| Type | Crate | Role |
|---|---|---|
| `SourceFile` | diagnostics | Path, text, and a line index for byte offset to line and column conversion. |
| `Span` | diagnostics | File ID and byte range. Every position in workflint is a `Span`. |
| `Diagnostic` | diagnostics | Rule, severity, message, primary span, labels, optional fix (ADR 0007). |
| `RuleMeta` | diagnostics | Static rule metadata: kebab-case ID, kind, category, default severity, docs URL. |
| `yaml::Document` and `yaml::Node` | yaml | Arena-allocated span tree with tags, scalar styles, anchors, comments, and scalar source maps. |
| `Workflow`, `Job`, `Step`, `ActionMetadata` | syntax | Typed AST. Every field keeps its `Span`. Values that may be expressions are `Value<T>` (a literal `T` or an expression). |
| `Expr`, `Ty` | expr | Expression AST and types. `Ty` is an immutable enum shared through `Arc`. |
| `Rule` trait and `Registry` | rules | A rule declares its metadata and implements visitor callbacks for the nodes it cares about. |
| `Project`, `Config`, `Linter` | linter | Project root, loaded config, caches, and the parallel driver. |

## Adding things

- A new check is a new rule module in `crates/workflint-rules/src/rules/`, a registry entry, a page in `docs/rules/<id>.md`, and tests (ADR 0008, ADR 0016).
- A new platform fact goes into `workflint-data`, either through a generator in `xtask` or, until M7, into the ported tables. Rules never hard-code platform facts.
- A new output format is a renderer in the `workflint` crate that consumes `Diagnostic` values. It never re-derives positions.
- A new config key gets a field in the config types in `workflint-linter`, a schema update (generated), and an entry in `docs/config.md`.

## Testing layers

| Layer | Where | What |
|---|---|---|
| Unit tests | next to the code | Lexer tokens, type rules, single rule behaviour. |
| Snapshot tests | each crate's `tests/` | Rendered output for representative inputs, with `insta`. |
| Conformance suite | `tests/conformance/` | The reference's fixtures through the real CLI, compared with expected errors (ADR 0004). |
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
