# Roadmap

workflint reaches actionlint parity at 0.1.0 and adds the most requested missing features at 1.0. This file is the plan: milestones, goals, order, and dependencies. Tasks are GitHub issues, one per pull request, grouped in GitHub milestones of the same name (ADR 0020). Task IDs such as M1.3 are stable and appear in issue titles and pull request titles, for example `feat(expr): M3.2 Pratt parser`.

Milestones M2 and M3 can run in parallel, and so can M6 and M7. M6 starts alongside M1.

```text
M0 ─ M1 ─┬─ M2 ─┬─ M4 ─ M5 ═ 0.1.0 ─ M7 ─ M8 ─ M9 ═ 1.0 ─ P
         └─ M3 ─┘
M6 (release pipeline) runs in parallel from M1 and is required for 0.1.0
```

Issues exist for M1, M2, M3, M6, and M5.11. Issues for each later milestone are created when the milestone before it is nearly done, so that they reflect what the earlier work taught. Until then, the task lists below are the plan. Dependencies between tasks are recorded on the issues as "blocked by" links.

## M0: planning and skeleton

- M0.1 Research upstream actionlint, its issues, and related projects (`docs/research/`).
- M0.2 ADRs, architecture, roadmap, parity checklist, issue coverage, differences, glossary.
- M0.3 Cargo workspace with a hello-world `workflint` binary, toolchain pin, lint config, `deny.toml`, and CI.
- M0.4 Move the repository to `stokehold/workflint`.
- M0.5 Reserve the `workflint` name on crates.io, npm, and PyPI.
- M0.6 Harden the repository and the organization (ADR 0021).

## M1: foundations

Goal: the types and test infrastructure every later milestone builds on. Start with M1.0 and M1.13, which every conformance and benchmark task needs. The diagnostics lane (M1.1, M1.2), the YAML lane (M1.3 to M1.6), and M1.12 can start at once.

- M1.0 Create the xtask crate [#1](https://github.com/stokehold/workflint/issues/1)
- M1.1 Add the diagnostic types to workflint-diagnostics [#2](https://github.com/stokehold/workflint/issues/2)
- M1.2 Add the default and one-line renderers [#3](https://github.com/stokehold/workflint/issues/3)
- M1.3 Vendor libyaml-safer into workflint-yaml and fix the end-of-file panic [#4](https://github.com/stokehold/workflint/issues/4)
- M1.4 Report comments from the vendored YAML scanner and parser [#5](https://github.com/stokehold/workflint/issues/5)
- M1.5 Build the arena span tree in workflint-yaml [#6](https://github.com/stokehold/workflint/issues/6)
- M1.6 Add scalar source maps to workflint-yaml [#7](https://github.com/stokehold/workflint/issues/7)
- M1.7 Compare YAML node positions with go-yaml [#8](https://github.com/stokehold/workflint/issues/8)
- M1.8 Vendor the reference's test data into tests/conformance [#9](https://github.com/stokehold/workflint/issues/9)
- M1.9 Add the conformance runner [#10](https://github.com/stokehold/workflint/issues/10)
- M1.10 Add cargo xtask oracle [#11](https://github.com/stokehold/workflint/issues/11)
- M1.11 Add cargo xtask bench and the nightly benchmark job [#12](https://github.com/stokehold/workflint/issues/12)
- M1.12 Run the test suite with cargo-nextest [#13](https://github.com/stokehold/workflint/issues/13)
- M1.13 Add cargo xtask reference to recreate .reference/ [#14](https://github.com/stokehold/workflint/issues/14)
- M1.14 Add divan micro-benchmarks and run them on pull requests with CodSpeed [#15](https://github.com/stokehold/workflint/issues/15)
- M1.15 Fuzz the YAML layer [#16](https://github.com/stokehold/workflint/issues/16)
- M1.16 Match go-yaml in the vendored YAML event layer [#50](https://github.com/stokehold/workflint/issues/50)
- M1.17 Fix comment attachment, duplicate keys and deep trees in the YAML tree [#51](https://github.com/stokehold/workflint/issues/51)
- M1.18 Make the conformance runner and the oracle fail loudly [#52](https://github.com/stokehold/workflint/issues/52)
- M1.19 Scope CI credentials and verify tool installs [#53](https://github.com/stokehold/workflint/issues/53)
- M1.20 Fix renderer columns and test Go quoting over every code point [#54](https://github.com/stokehold/workflint/issues/54)
- M1.21 Update the docs and ADRs after the M1 review [#55](https://github.com/stokehold/workflint/issues/55)
- M1.22 Stop conformance tests from racing on the workflint binary [#101](https://github.com/stokehold/workflint/issues/101)

## M2: workflow syntax

Goal: every `syntax-check` diagnostic of the reference, on a typed AST. Start with M2.0, which every task that ticks a fixture needs, and M2.1. M2.8 also needs M3.2.

- M2.0 Add a minimal workflint binary for the conformance suite [#65](https://github.com/stokehold/workflint/issues/65)
- M2.1 Add the workflow AST types to workflint-syntax [#66](https://github.com/stokehold/workflint/issues/66)
- M2.2 Add the parser framework and the workflow root to workflint-syntax [#67](https://github.com/stokehold/workflint/issues/67)
- M2.3 Parse the on section and its events [#68](https://github.com/stokehold/workflint/issues/68)
- M2.4 Parse env, defaults, concurrency, permissions, and environment [#69](https://github.com/stokehold/workflint/issues/69)
- M2.5 Parse jobs and reusable workflow call jobs [#70](https://github.com/stokehold/workflint/issues/70)
- M2.6 Parse steps [#71](https://github.com/stokehold/workflint/issues/71)
- M2.7 Type the uses values of steps and jobs [#72](https://github.com/stokehold/workflint/issues/72)
- M2.8 Parse the expressions in workflow strings [#73](https://github.com/stokehold/workflint/issues/73)
- M2.9 Port the glob validator [#74](https://github.com/stokehold/workflint/issues/74)
- M2.10 Match the reference's syntax-check output on the fixtures and the corpus [#75](https://github.com/stokehold/workflint/issues/75)
- M2.11 Parse strategy and matrix [#76](https://github.com/stokehold/workflint/issues/76)
- M2.12 Parse container and services [#77](https://github.com/stokehold/workflint/issues/77)
- M2.13 Compare selected kinds in cargo xtask oracle [#78](https://github.com/stokehold/workflint/issues/78)
- M2.14 Decode local action and reusable workflow metadata [#79](https://github.com/stokehold/workflint/issues/79)

## M3: expressions

Goal: the expression language with the reference's type checking, without its bugs. Start with M3.0 and M3.1, which can run in parallel.

- M3.0 Create workflint-data with the global context types [#81](https://github.com/stokehold/workflint/issues/81)
- M3.1 Add the expression lexer to workflint-expr [#82](https://github.com/stokehold/workflint/issues/82)
- M3.2 Add the expression parser with GitHub's operator precedence [#83](https://github.com/stokehold/workflint/issues/83)
- M3.3 Add the expression type system [#84](https://github.com/stokehold/workflint/issues/84)
- M3.4 Check calls to built-in functions [#85](https://github.com/stokehold/workflint/issues/85)
- M3.5 Type-check expressions against context types [#86](https://github.com/stokehold/workflint/issues/86)
- M3.6 Check context and special function availability [#87](https://github.com/stokehold/workflint/issues/87)
- M3.7 Detect untrusted input in script expressions [#88](https://github.com/stokehold/workflint/issues/88)
- M3.8 Fuzz and benchmark the expression engine [#89](https://github.com/stokehold/workflint/issues/89)
- M3.9 Type fromJSON literals and report Go's JSON errors [#90](https://github.com/stokehold/workflint/issues/90)
- M3.10 Fuzz and benchmark the expression checker [#107](https://github.com/stokehold/workflint/issues/107)

## M4: rules and data

Goal: every reference rule, with fine-grained rule IDs.

- M4.1 `workflint-rules`: `Rule` trait, visitor, registry, rule metadata, fixed rule order.
- M4.2 `workflint-data`: port the remaining tables of the reference (runner labels, permissions, webhooks, popular actions, deprecated commands) as generated-style Rust files. M3 ports the expression tables.
- M4.3 to M4.n One pull request per reference rule or group of closely related rules, in the order of `docs/parity-checklist.md`: expression checks, `if:` conditions, job `needs`, matrix, events, globs, permissions, runner labels, shell names, environment variable names, credentials, deprecated commands, actions and action metadata, reusable workflow calls.
- M4.x Rule documentation pages for every rule, with examples run as tests.

## M5: linter, CLI, and output

Goal: a drop-in binary. Release 0.1.0 at the end of this milestone.

- M5.1 `workflint-linter`: project root discovery, file discovery, stdin.
- M5.2 Config loading: actionlint-compatible keys, path globs relative to the project root, `-init-config`.
- M5.3 Parallel driver, per-project caches with deterministic error attribution, panic isolation.
- M5.4 External tool process pool with timeouts and concurrent I/O; 200 KiB script test on every OS.
- M5.5 shellcheck and pyflakes rules with source-mapped positions, deduplication, and the opt-in cache. Evaluate batching.
- M5.6 CLI with Go-style flag normalisation over clap, exit codes, `-version`, help.
- M5.7 `workflint-gotemplate`: lexer, parser, executor, actionlint's functions; fixtures from the reference's format tests.
- M5.8 Output formats: default, `-oneline`, `-format`, `json`, `sarif`, `github`. A hint line under go-yaml's `did not find expected key` error that names the likely cause, such as a mis-indented list item (rhysd/actionlint#519).
- M5.9 Whole conformance suite green; oracle diff clean apart from allowed differences, and the oracle job made blocking; performance targets met. Needs M5.11.
- M5.10 Release 0.1.0.
- M5.11 Apply allow-list entries to corpus files in cargo xtask oracle [#49](https://github.com/stokehold/workflint/issues/49)

## M6: release pipeline

Goal: every distribution channel works before 0.1.0 (ADR 0014, ADR 0015, ADR 0021). Each packaging task adds a dry-run job, and M6.11 combines them into one required check. Publishing stays disabled until 0.1.0.

- M6.0 Create the stokehold-bot GitHub App and the protected environments [#17](https://github.com/stokehold/workflint/issues/17)
- M6.1 Configure trusted publishing on crates.io, npm, and PyPI [#18](https://github.com/stokehold/workflint/issues/18)
- M6.2 Add the release workflow with archives, checksums, attestations, and an SBOM [#19](https://github.com/stokehold/workflint/issues/19)
- M6.3 Add release-plz and a Conventional Commits check on pull request titles [#20](https://github.com/stokehold/workflint/issues/20)
- M6.4 Add install.sh and install.ps1 with checksum and attestation verification [#21](https://github.com/stokehold/workflint/issues/21)
- M6.5 Publish npm packages with a launcher and per-platform binaries [#22](https://github.com/stokehold/workflint/issues/22)
- M6.6 Build PyPI wheels with maturin and publish them with trusted publishing [#23](https://github.com/stokehold/workflint/issues/23)
- M6.7 Publish default and -full Docker images for amd64 and arm64 [#24](https://github.com/stokehold/workflint/issues/24)
- M6.8 Create the Homebrew tap and update its formula from the release workflow [#25](https://github.com/stokehold/workflint/issues/25)
- M6.9 Add pre-commit hooks and the workflint-pre-commit mirror [#26](https://github.com/stokehold/workflint/issues/26)
- M6.10 Create the workflint-action repository with verified install and automated version bumps [#27](https://github.com/stokehold/workflint/issues/27)
- M6.11 Combine the release dry runs into one required pull request check [#28](https://github.com/stokehold/workflint/issues/28)

## M7: platform data and new GitHub syntax

Goal: data from official sources that stays fresh, and the GitHub features that the reference rejects.

- M7.1 Generators for the workflow schema, permissions, and context availability from `actions/languageservices`.
- M7.2 Generators for webhook events and activity types.
- M7.3 Generator for runner labels, with lifecycle dates compared against the snapshot date.
- M7.4 Generator for popular actions: all majors still in use, exact versions, `deprecationMessage`, SHA pins resolved through `# vX.Y.Z` comments.
- M7.5 Generator for action metadata schema and Node runtime lifecycle from `actions/runner`.
- M7.6 Scheduled refresh workflow, drift pull requests, failure issues, automatic patch releases.
- M7.7 New syntax: parallel steps (`background`, `wait`, `wait-all`, `cancel`, `parallel`) with reference checks.
- M7.8 New syntax: `$/` self-repository `uses:` for actions and reusable workflows.
- M7.9 New syntax: `cache-mode`, `concurrency.queue`, and other keys the schema adds.
- M7.10 Config extensions for platform data: runner labels, known secrets, per-action overrides.

## M8: action metadata

Goal: lint `action.yml` and composite actions as first-class files.

- M8.1 `ActionMetadata` AST and parser, auto-detected by file name.
- M8.2 Discovery of action metadata files in the project.
- M8.3 Composite steps reuse the workflow step checks: expressions, shell, `uses`, shellcheck.
- M8.4 Action-specific rules: `runs.using`, required keys, inputs, outputs, branding, context availability in composite steps.
- M8.5 Errors in local actions reported in the action file itself.

## M9: rule control

Goal: severities, categories, and ignore comments. Release 1.0 at the end of this milestone.

- M9.1 Severity and category on every rule; `--fail-on`.
- M9.2 `rules:` config by rule ID, kind, or category, globally and per path glob.
- M9.3 Strict config schema for workflint config files, generated JSON Schema, `workflint config schema`.
- M9.4 Config discovery order including the user config file; `--no-config`.
- M9.5 Ignore comments, `unused-ignore`, `invalid-ignore`, `require-ignore-reason`, `# actionlint:ignore` compatibility.
- M9.6 `workflint rules` and `workflint rules <id>`.
- M9.7 First opt-in rules in `pedantic`, `style`, and `policy`: SHA pinning, required `permissions`, required `timeout-minutes`, explicit `shell`.
- M9.8 Publish all crates to crates.io, with `cargo binstall` metadata pointing at the release archives. Release 1.0.0.

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
