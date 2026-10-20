# Architecture

This document describes how workflint is put together: the crates, how a file flows through them, and the main types. The ADRs in `docs/adr/` record why. Read this before working on any crate.

This document describes the target design. Each crate is created in the milestone that first needs it (ADR 0006), so `crates/` holds only the crates built so far, and `docs/roadmap.md` says when the others arrive.

## Crates

```text
workflint                      binary: CLI, output formats
├── workflint-gotemplate       Go text/template subset for -format
└── workflint-linter           discovery, config, parallel driver, caches, external tools
    └── workflint-rules        rule registry and rules
        └── workflint-syntax   workflow and action.yml AST, parser, glob validator
            ├── workflint-yaml YAML parser fork, span tree, comments, source maps
            └── workflint-expr ${{ }} lexer, parser, types, checker
                └── workflint-data  GitHub platform data, also used by workflint-rules

workflint-diagnostics          Diagnostic, Span, Severity, renderers; every crate except
                               workflint-data and workflint-gotemplate depends on it
```

ADR 0006 has the full dependency table and the boundary rules, and ADR 0027 adds the dependency of `workflint-expr` on `workflint-data`. Only `workflint-linter` and `workflint` touch the file system, spawn processes, or read environment variables. Every other crate is a pure function of its inputs, so it compiles to WASM and needs no mocks in tests.

Rules that another crate reports are an exception to "rules live in `workflint-rules`". `workflint-yaml`, `workflint-syntax`, and `workflint-expr` report diagnostics while parsing, and `workflint-expr` also while checking, when a rule calls `TypeEnv::check`. The crate that reports a diagnostic defines the `RuleMeta` of its rule, and the registry in `workflint-rules` lists those rules with its own (ADR 0024, ADR 0028).

## Outside the crates

- `xtask/` is the package behind `cargo xtask`, the developer commands. `cargo xtask --help` lists them. Its `conformance` test target is the conformance suite, `xtask/goyaml-dump/` is the Go program that `cargo xtask yaml-parity` builds to dump go-yaml's tree, and `xtask/expr-check/` is the Go program that `cargo xtask expr-parity` builds around the reference's expression checker. It is never published (ADR 0006).
- `xtask/go-quote-hash/` is a Go program that hashes the output of Go's `strconv.Quote` and `strconv.QuoteRune` for every Unicode scalar value. The tests of `workflint_diagnostics::quote` compare their own output with these hashes.
- `fuzz/` holds the cargo-fuzz targets. It is a Cargo workspace of its own, with its own `Cargo.lock` and a pinned nightly toolchain, because `cargo fuzz` needs nightly (ADR 0025). It is never published.

## How a run works

1. The CLI parses arguments (Go-style and modern flags, ADR 0003) and builds a `LintRequest`: the files or stdin to lint, the config override, the output format, and the failure threshold.
2. `workflint-linter` finds each file's project root and loads its config (ADR 0010). With no file arguments, it lists `.github/workflows/*.y{a,}ml` in the project and, from M8, `action.y{a,}ml` files.
3. For each file, in parallel (ADR 0013):
   1. `workflint-yaml` decodes the bytes and parses the first YAML document into a span tree. The first YAML syntax error stops the file: it becomes one diagnostic, and no later step runs on that file, as in actionlint (ADR 0022).
   2. `workflint-syntax` walks the tree and builds the typed AST: `Workflow` or `ActionMetadata`. Unknown keys, wrong value types, and missing required keys become `syntax-check` diagnostics. Each string in a place where GitHub evaluates `${{ }}` becomes a `Template`, and each `${{ }}` in it is parsed into an expression AST with `workflint-expr`. Once the AST is built, the parser reports the syntax errors of these expressions as `invalid-expression-syntax` and `invalid-number-literal` diagnostics. The `uses:` values of steps and jobs are classified into their parts, such as owner, repo, path, and ref, so rules and caches match on the kind instead of parsing the string.
   3. `workflint-rules` creates the rules for the file in the reference's order and visits the AST with them: the workflow-pre hook of every rule, then for each job in source order its job-pre hook, the step hook for each step, and the job-post hook, then the workflow-post hook. Rules use `workflint-expr` to type-check expressions against contexts built from the workflow, and `workflint-data` for platform facts. The `expression` rule builds one `TypeEnv` for the workflow, clones it for each job, and sets the workflow key of each location before it checks the expressions there. It checks the placeholders that `workflint_syntax::placeholders` visits, the walk that reports their parse errors, and a test compares the two. Rules that need other files (local actions, local reusable workflows) look them up by `uses:` value through the `LocalMetadata` that the `Context` holds. The linter implements it with a per-project cache, which reads each file once per run and decodes it with `ActionMetadata::decode` or `ReusableWorkflowMetadata::decode`. A linted reusable workflow registers its `on.workflow_call` in the cache, so its callers do not read the file again.
   4. Rules that need shellcheck or a Python linter queue script jobs. The external tool pool runs them (ADR 0012), and their findings are mapped back to workflow positions through the script's source map, which the YAML layer builds on demand.
4. The linter applies ignore comments (ADR 0009), config severity overrides, and `-ignore` patterns, then sorts and deduplicates (ADR 0007).
5. The CLI renders the diagnostics in the chosen format and exits with 0, 1, 2, or 3.

## Main types

| Type | Crate | Role |
|---|---|---|
| `SourceFile` | diagnostics | Path, text, and a line index for byte offset to line and column conversion. |
| `Span` | diagnostics | File ID and byte range. Every position in workflint is a `Span`. |
| `Diagnostic` | diagnostics | Rule, severity, message, primary span, labels, optional fix, optional help text (ADR 0007, ADR 0032). |
| `RuleMeta` | diagnostics | Static rule metadata: kebab-case ID, one-line description, kind, category, default severity, docs URL. |
| `yaml::Document` and `yaml::Node` | yaml | Arena-allocated span tree with tags, scalar styles, anchors, and comments. `Node::source_map` builds a scalar's source map on demand (ADR 0022). |
| `Workflow`, `Event`, `Job`, `Step` | syntax | Typed AST that borrows from the `yaml::Document`. Keys and strings are `Str` leaves, which keep their YAML node and span. A string in a place where GitHub evaluates `${{ }}` is a `Template`: a `Str` and its parsed `Placeholder`s. A key's value is a `Field<T>` (absent, invalid, or valid) or, where it may be a `${{ }}` expression, a `Value<T>` (absent, invalid, a literal `T`, or an `Expr`). An invalid item of a list is `None`, so the list keeps its length. Collections keyed by name are `Keyed<T>`, in source order with case-insensitive lookup. A `Keyed<T>` also holds the expressions written under `${{ insert }}` keys, whose mappings GitHub merges into it when the workflow runs. The other AST nodes whose mapping accepts the key, such as `Strategy` and `Container`, hold them in an `inserts` field. Collections are shared through `Arc`, so cloning a subtree is cheap. The parser parses an anchored subtree again at each alias that uses it, as the reference does, and parses each template and expression once, sharing it between aliases. Each `Str` records its alias path, the aliases through which the parser reached it, and `Context::report_in` turns that path into labels. |
| `ActionMetadata`, `ReusableWorkflowMetadata` | syntax | The metadata of a local action or a local reusable workflow that calls are checked against, from typed decoding of the file. It owns its strings, so the linter can cache it per project. Decoding fails with the error text that the reference reports. |
| `Expr`, `Ty`, `TypeEnv` | expr | Expression AST, types, and the type environment that `TypeEnv::check` type-checks an `Expr` against. `Ty` is an immutable enum shared through `Arc`. A `TypeEnv` is cheap to clone, so a rule builds one per workflow and changes a clone for each job and step. A new `TypeEnv` allows no context and no special function, as actionlint's checker does, so a rule calls `TypeEnv::set_workflow_key` at every place it checks, also where the key is empty. A missing call reports every context instead of hiding errors. `TypeEnv::set_untrusted_inputs` makes `check` also report untrusted input, for an expression in a script. |
| `TypeDesc`, `GLOBAL_CONTEXTS`, `BUILTIN_FUNCTIONS`, `UNTRUSTED_INPUTS` | data | Expression types as `static` platform data, the types of the global contexts, the signatures of the built-in functions, and the tree of untrusted inputs. `workflint-expr` converts a `TypeDesc` to a `Ty` (ADR 0027). |
| `WORKFLOW_KEYS`, `SPECIAL_FUNCTIONS` | data | The workflow keys that allow some context, each with its `Availability`: the contexts and special functions its expressions may use. A key that the table does not list allows none. |
| `Rule`, `Context`, `visit` | rules | A rule implements some of the five hooks of the reference's `Pass` and reports diagnostics through the `Context`, which also gives the `SourceFile` and the project's `LocalMetadata`. `visit` calls the hooks in the reference's order, and `create_rules` creates the rules for each file in `CREATION_ORDER`. |
| `registry`, `kind` | rules | `registry::rules` lists the `RuleMeta` of every rule, sorted by rule ID, including the parse-stage and checker rules of other crates, and `registry::find` looks one up by ID. `kind::ALL` has the reference's description of each actionlint kind. |
| `LocalMetadata`, `Lookup`, `MemoryProject` | rules | The interface through which rules look up a local action or local reusable workflow by `uses:` value. A `Lookup` is nothing, the metadata with whether this is the first lookup of the value in the run, or the reference's error text, which only the first lookup gets. A rule can also register the linted workflow's `on.workflow_call`, or record an invalid `./` reference so that it is never read. `NoProject` finds nothing, and `MemoryProject` holds files in memory for the tests of rules and rule pages. |
| `find_project_root`, `LocalCache` | linter | The project root of a file, as the reference finds it, and the per-project cache that implements `LocalMetadata` for each linted file through `LocalCache::for_file`. Each entry is computed once. |
| `Config`, `Linter` | linter | Loaded config and the parallel driver, from M5. |

## Adding things

- A ported reference rule is one module in `crates/workflint-rules/src/rules/`, named after its kind, such as `events.rs`. It has one `Rule` implementation, a `RuleMeta` static for each rule ID it reports, and an arm in `create` at its place in `CREATION_ORDER`. Its `RuleMeta` statics go into `ALL` in `rules/mod.rs`, which the registry reads, and its rule IDs leave `tests/conformance/rule-kinds.toml`. Each rule ID gets a page in `docs/rules/<id>.md` and tests (ADR 0008, ADR 0016, ADR 0029).
- A new check that no reference rule makes gets its own module and `Rule` implementation the same way.
- A check that a parser or the expression checker reports keeps its `RuleMeta` in that crate instead of a rule module, and the registry lists it (ADR 0024, ADR 0028).
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
| Expression parity | `cargo xtask expr-parity` | `workflint-expr` against the reference's expression checker on about 14,000 expressions in several setups: parse errors, types, the constant flag, and each error with its position. |
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
