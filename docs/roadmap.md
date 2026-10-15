# Roadmap

workflint reaches actionlint parity at 0.1.0 and adds the most requested missing features at 1.0. Each task below is sized for one pull request (ADR 0016). Task IDs are stable. Reference them in branch names and pull request titles, for example `feat(expr): M3.2 Pratt parser`.

Milestones M2 and M3 can run in parallel, and so can M6 and M7. M6 starts alongside M1.

```text
M0 ─ M1 ─┬─ M2 ─┬─ M4 ─ M5 ═ 0.1.0 ─ M7 ─ M8 ─ M9 ═ 1.0 ─ P
         └─ M3 ─┘
M6 (release pipeline) runs in parallel from M1 and is required for 0.1.0
```

Status markers: `[ ]` open, `[x]` done. Tick tasks in the pull request that completes them.

## M0: planning and skeleton

- [x] M0.1 Research upstream actionlint, its issues, and related projects (`docs/research/`).
- [x] M0.2 ADRs 0001 to 0019, architecture, roadmap, parity checklist, issue coverage, differences, glossary.
- [x] M0.3 Cargo workspace with a hello-world `workflint` binary, toolchain pin, lint config, `deny.toml`, and CI.
- [ ] M0.4 Maintainer: transfer the repository to `stokehold/workflint`.
- [ ] M0.5 Maintainer: publish the placeholder crate to crates.io to reserve `workflint`.

## M1: foundations

Goal: the types and test infrastructure every later milestone builds on.

- [ ] M1.1 `workflint-diagnostics`: `SourceFile` with line index, `Span`, `Severity`, `RuleMeta`, `Diagnostic`, `Fix` types, sorting and deduplication (ADR 0007).
- [ ] M1.2 `workflint-diagnostics`: default renderer with the `^~~~` snippet and actionlint's color codes, and the one-line renderer. Columns in code points, tab and wide-character handling. Snapshot tests.
- [ ] M1.3 `workflint-yaml`: vendor `libyaml-safer` 0.3.0 with license and origin commit; fix the end-of-file block scalar panic; add a regression test.
- [ ] M1.4 `workflint-yaml`: comment events in the scanner and parser.
- [ ] M1.5 `workflint-yaml`: arena span tree with tags, styles, anchors, aliases, duplicate key reporting, and comments attached to nodes.
- [ ] M1.6 `workflint-yaml`: scalar source maps for plain, quoted, literal, and folded scalars, with property tests against the decoded values.
- [ ] M1.7 `workflint-yaml`: position parity test that compares node positions with go-yaml over the reference fixtures and the corpus (via a small Go dump program in `xtask`).
- [ ] M1.8 Vendor the reference's `testdata/` into `tests/conformance/actionlint/` with license and commit. Add the corpus archive.
- [ ] M1.9 Conformance runner with `datatest-stable`, the expected-error matching rules, the allow-list file, and the kind mapping. All fixtures run and are marked expected-to-fail until their milestone.
- [ ] M1.10 `cargo xtask oracle`: build the reference with a pinned Go toolchain, run both binaries, diff the outputs, apply the allow-list. CI job.
- [ ] M1.11 `cargo xtask bench`, divan setup, nightly benchmark job, CodSpeed on pull requests (ADR 0019).
- [ ] M1.12 Switch CI to `cargo-nextest`; add fuzz targets for the YAML layer; nightly fuzz job.

## M2: workflow syntax

Goal: every `syntax-check` diagnostic of the reference, on a typed AST.

- [ ] M2.1 `workflint-syntax`: AST types for workflows with spans on every field and `Value<T>` for expression-capable values.
- [ ] M2.2 Parser framework: mapping walker with case-insensitive keys, unknown key, duplicate key, wrong kind, and empty value handling, matching the reference's messages.
- [ ] M2.3 Parse `on:` with every event type, filters, `workflow_dispatch` and `workflow_call` inputs, outputs, and secrets.
- [ ] M2.4 Parse workflow-level keys: `name`, `run-name`, `env`, `defaults`, `concurrency`, `permissions`.
- [ ] M2.5 Parse jobs: `runs-on`, `needs`, `if`, `strategy` and `matrix`, `container`, `services`, `outputs`, `environment`, `timeout-minutes`, `continue-on-error`, reusable workflow calls.
- [ ] M2.6 Parse steps: `run`, `uses`, `with`, `env`, `shell`, `working-directory`, and the remaining step keys.
- [ ] M2.7 Typed `Uses` enum: local `./`, Docker, repository action, reusable workflow. M7 adds `$/`.
- [ ] M2.8 Find and parse every `${{ }}` in strings into expression ASTs (needs M3.2).
- [ ] M2.9 Port the glob validator for branch, tag, and path filters.
- [ ] M2.10 Pass all `syntax-check` conformance fixtures; tick the checklist.

## M3: expressions

Goal: the expression language with the reference's type checking, without its bugs.

- [ ] M3.1 Lexer with byte offsets and code-point columns; JSON number grammar (exponent sign, large integers).
- [ ] M3.2 Pratt parser with GitHub's operator precedence. Confirm the precedence table against the expression parser in `actions/languageservices` first, and record the result in `docs/differences.md#comparison-precedence`.
- [ ] M3.3 Type system: `Ty` enum, object and array types, assignability, merging, `*` filters without shared mutation.
- [ ] M3.4 Built-in functions with overloads and format string checks.
- [ ] M3.5 Context types: `github`, `env`, `vars`, `job`, `jobs`, `steps`, `runner`, `secrets`, `strategy`, `matrix`, `needs`, `inputs`, with case-insensitive property lookup.
- [ ] M3.6 Context availability per workflow key, ported from the reference's table.
- [ ] M3.7 Untrusted input detection for script sinks.
- [ ] M3.8 Fuzz targets for the lexer and parser.

## M4: rules and data

Goal: every reference rule, with fine-grained rule IDs.

- [ ] M4.1 `workflint-rules`: `Rule` trait, visitor, registry, rule metadata, fixed rule order.
- [ ] M4.2 `workflint-data`: port the reference's tables (runner labels, permissions, webhooks, popular actions, availability, deprecated commands) as generated-style Rust files.
- [ ] M4.3 to M4.n One pull request per reference rule or group of closely related rules, in the order of `docs/parity-checklist.md`: expression checks, `if:` conditions, job `needs`, matrix, events, globs, permissions, runner labels, shell names, environment variable names, credentials, deprecated commands, actions and action metadata, reusable workflow calls.
- [ ] M4.x Rule documentation pages for every rule, with examples run as tests.

## M5: linter, CLI, and output

Goal: a drop-in binary. Release 0.1.0 at the end of this milestone.

- [ ] M5.1 `workflint-linter`: project root discovery, file discovery, stdin.
- [ ] M5.2 Config loading: actionlint-compatible keys, path globs relative to the project root, `-init-config`.
- [ ] M5.3 Parallel driver, per-project caches with deterministic error attribution, panic isolation.
- [ ] M5.4 External tool process pool with timeouts and concurrent I/O; 200 KiB script test on every OS.
- [ ] M5.5 shellcheck and pyflakes rules with source-mapped positions, deduplication, and the opt-in cache. Evaluate batching.
- [ ] M5.6 CLI with Go-style flag normalisation over clap, exit codes, `-version`, help.
- [ ] M5.7 `workflint-gotemplate`: lexer, parser, executor, actionlint's functions; fixtures from the reference's format tests.
- [ ] M5.8 Output formats: default, `-oneline`, `-format`, `json`, `sarif`, `github`.
- [ ] M5.9 Whole conformance suite green; oracle diff clean apart from allowed differences; performance targets met.
- [ ] M5.10 Release 0.1.0.

## M6: release pipeline

Goal: every distribution channel works before 0.1.0 (ADR 0014, ADR 0015). Runs in dry-run mode until then.

- [ ] M6.1 Maintainer: reserve the npm name `workflint`, create the `@stokehold` npm organization, reserve the PyPI name `workflint`, configure trusted publishing on npm, PyPI, and crates.io.
- [ ] M6.2 Release workflow: build matrix for six targets, archives with actionlint-style names, checksums, attestations, SBOM.
- [ ] M6.3 release-plz configuration and Conventional Commits check on pull requests.
- [ ] M6.4 `install.sh` and `install.ps1` with checksum and attestation verification; tests on three OSes.
- [ ] M6.5 npm packages: launcher and per-platform packages, publish job, smoke test with npm, pnpm, and Bun.
- [ ] M6.6 PyPI wheels with maturin, publish job, smoke test with pip and uvx.
- [ ] M6.7 Docker images, default and `-full`, multi-arch, with attestations.
- [ ] M6.8 Homebrew: create `stokehold/homebrew-tap`; formula update job using a GitHub App token.
- [ ] M6.9 pre-commit: `.pre-commit-hooks.yaml` in this repository; create `stokehold/workflint-pre-commit` with automated updates.
- [ ] M6.10 GitHub Action: create `stokehold/workflint-action` with the install script, inputs and outputs, test matrix, and automated version bumps.
- [ ] M6.11 Dry-run release on every pull request that touches packaging.

## M7: platform data and new GitHub syntax

Goal: data from official sources that stays fresh, and the GitHub features that the reference rejects.

- [ ] M7.1 Generators for the workflow schema, permissions, and context availability from `actions/languageservices`.
- [ ] M7.2 Generators for webhook events and activity types.
- [ ] M7.3 Generator for runner labels, with lifecycle dates compared against the snapshot date.
- [ ] M7.4 Generator for popular actions: all majors still in use, exact versions, `deprecationMessage`, SHA pins resolved through `# vX.Y.Z` comments.
- [ ] M7.5 Generator for action metadata schema and Node runtime lifecycle from `actions/runner`.
- [ ] M7.6 Scheduled refresh workflow, drift pull requests, failure issues, automatic patch releases.
- [ ] M7.7 New syntax: parallel steps (`background`, `wait`, `wait-all`, `cancel`, `parallel`) with reference checks.
- [ ] M7.8 New syntax: `$/` self-repository `uses:` for actions and reusable workflows.
- [ ] M7.9 New syntax: `cache-mode`, `concurrency.queue`, and other keys the schema adds.
- [ ] M7.10 Config extensions for platform data: runner labels, known secrets, per-action overrides.

## M8: action metadata

Goal: lint `action.yml` and composite actions as first-class files.

- [ ] M8.1 `ActionMetadata` AST and parser, auto-detected by file name.
- [ ] M8.2 Discovery of action metadata files in the project.
- [ ] M8.3 Composite steps reuse the workflow step checks: expressions, shell, `uses`, shellcheck.
- [ ] M8.4 Action-specific rules: `runs.using`, required keys, inputs, outputs, branding, context availability in composite steps.
- [ ] M8.5 Errors in local actions reported in the action file itself.

## M9: rule control

Goal: severities, categories, and ignore comments. Release 1.0 at the end of this milestone.

- [ ] M9.1 Severity and category on every rule; `--fail-on`.
- [ ] M9.2 `rules:` config by rule ID, kind, or category, globally and per path glob.
- [ ] M9.3 Strict config schema for workflint config files, generated JSON Schema, `workflint config schema`.
- [ ] M9.4 Config discovery order including the user config file; `--no-config`.
- [ ] M9.5 Ignore comments, `unused-ignore`, `invalid-ignore`, `require-ignore-reason`, `# actionlint:ignore` compatibility.
- [ ] M9.6 `workflint rules` and `workflint rules <id>`.
- [ ] M9.7 First opt-in rules in `pedantic`, `style`, and `policy`: SHA pinning, required `permissions`, required `timeout-minutes`, explicit `shell`.
- [ ] M9.8 Publish all crates to crates.io. Release 1.0.0.

## P: after 1.0

Not scheduled yet. Listed in rough priority order.

- Autofix with `--fix` and `--fix-unsafe`.
- Language server (`workflint lsp`) and an editor extension.
- WASM playground.
- Opt-in online mode with a persistent cache for remote action metadata and reusable workflows.
- Cross-file analysis of reusable workflows: permissions, inputs, secrets, nesting limits.
- Built-in bash syntax check.
- Syntax check for `actions/github-script` scripts.
- Typed `github.event` payloads per trigger.
- Target profiles for GitHub Enterprise Server and other platforms.
- `extends` for shared config.
- More opt-in rules, tracked in `docs/issues-coverage.md`.
