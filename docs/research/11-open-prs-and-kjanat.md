# 11: zizmor open PRs, the kjanat/actionlint fork, and a consolidated open-PR checklist

Snapshot taken 2026-10-15. Sources:

- `zizmorcore/zizmor`: all **25 open PRs** were read, including the diffs or discussion for the substantive ones. The 30 most recently merged PRs were also read. The reference checkout is `.reference/zizmor` at `5eb3769`.
- `kjanat/actionlint`: cloned read-only to `.reference/kjanat-actionlint` at `8ac13d2` (master, 2026-10-15). The following were all read: 5 open PRs, 118 issues (all states), 136 merged PRs (titles; bodies for the key ones), README, CHANGELOG (fork section), `UPSTREAM.md` and `docs/*`.
- The upstream comparison base is `rhysd/actionlint@011a6d1` (`.reference/actionlint`).

Raw dumps are in `.reference/research/raw/`:

| File | Contents |
|---|---|
| `zizmor-prs-open.json` | 25 open zizmor PRs: body, files, labels, reactions |
| `zizmor-prs-merged.json` | 30 most recently merged zizmor PRs |
| `kjanat-issues.json` | 118 kjanat issues, all states, with comments |
| `kjanat-prs-open.json` | 5 open kjanat PRs, with files |
| `kjanat-prs-merged.json` / `kjanat-prs-closed.json` | 136 merged PRs / 160 closed PRs (the closed set includes the merged ones; titles, authors, dates) |

Out of scope here: upstream actionlint PRs (05-issues.md §3, merged into §d below), zizmor code and issues (08), jactionlint (09) and zizmor-action (10).

---

## (a) zizmor open PRs (25)

PR links have the form `https://github.com/zizmorcore/zizmor/pull/N`. "Draft" means a GitHub draft. All but two PRs (#2424, #2423) have been idle for more than a week. William Woodruff (the maintainer) has a large review backlog: #2081, #2166 and #2159 are all waiting on him.

| PR | Status | What it does | Signal for workflint |
|---|---|---|---|
| [#2424](https://github.com/zizmorcore/zizmor/pull/2424) | draft, ready to merge 2026-11-01 | Windows ARM64 release builds (PyPI wheels). Follows merged #2420, which added Windows ARM64 tests. | **Distribution.** Ship `aarch64-pc-windows-msvc` (binary and wheel) from v1 and run CI on `windows-11-arm`. |
| [#2423](https://github.com/zizmorcore/zizmor/pull/2423) | draft, fixes #2422 | **Handles `''` escaping in "raw" expression audits.** `FOO: '${{ ''foo'' }}'` is valid: GitHub runs it, and `@actions/workflow-parser` parses it. zizmor's raw-source audits (obfuscation, unsound-ternary, secrets-outside-env, …) passed the undecoded `''` into the expression parser. The fix adds a `yamlpath::Scalar` (`crates/yamlpath/src/scalar.rs`, new) with a decoded `text()`, a per-byte `offsets` vector back to the source, `ScalarStyle`, and `scalar_at(offset)`. `yamlpatch` gains `RewriteScalarFragment`, which re-escapes `'` → `''` when patching single-quoted scalars. Woodruff called it "another good example of how brittle zizmor's current YAML abstractions are". | **Key parsing pitfall.** Never analyse raw scalar source text. workflint needs one *decoded-text ↔ source-span map* per scalar, covering all 5 styles, escapes, folding and chomping. Expressions, ShellCheck/Ruff remapping, inline-suppression scope and autofix must all use it. Autofix must re-encode for the target scalar style. Test fixture: `'${{ ''foo'' }}'`. kjanat already built this in Go (`internal/yamlsource/scalar.go:52`). |
| [#2405](https://github.com/zizmorcore/zizmor/pull/2405) | open | `superfluous-actions` no longer flags `dtolnay/rust-toolchain` on self-hosted runners (#1865). | A rule's outcome depends on hosted vs self-hosted. workflint needs a shared, tri-state "runner kind" query (see #2140). |
| [#2364](https://github.com/zizmorcore/zizmor/pull/2364) | draft, WIP | `zizmor-http-cache`: a near-verbatim lift of uv's RFC 9111 `httpcache` (with attribution). CodSpeed reported a 25% regression, which Woodruff called unreliable. | If workflint gets an online mode (remote action metadata or reusable workflows), use a standards-compliant HTTP cache. uv's crate is the model. Benchmark on dedicated runners; hosted-runner walltime numbers are noise. |
| [#2329](https://github.com/zizmorcore/zizmor/pull/2329) | open | Docs issue template. | None. |
| [#2318](https://github.com/zizmorcore/zizmor/pull/2318) | open, `needs-decision` | `--github-com-token` fallback for GitHub Enterprise Cloud (#735). Woodruff is blocked on understanding GHES. He prefers an `auth:` config section mapping domain patterns to environment variable names. Commenters raise Gitea/Forgejo URL-qualified `uses: https://github.com/actions/checkout@v4` and per-org tokens. He also says zizmor "will probably be adding support for other CI/CD platforms". | For any online feature, design per-host auth from the start (`auth: [{domain, envs}]`, never a token-packing syntax). Recognise GHES hosts. Forgejo URL `uses:` (upstream #698) only behind an explicit `platform: forgejo` mode. |
| [#2233](https://github.com/zizmorcore/zizmor/pull/2233) | draft, author says wrong approach | Audit ID aliases (#2232), so audits can be renamed (e.g. `hardcoded-container-credentials` → `hardcoded-credentials`). The preferred approach is to remap the config at load time. | **Rule IDs are API.** Ship a rule-alias table from v1. Old IDs keep working in config, CLI and inline ignores, with a deprecation warning. workflint also needs actionlint's IDs as aliases (`expression`, `syntax-check`, …). |
| [#2166](https://github.com/zizmorcore/zizmor/pull/2166) | open, `new-audit`, queued for 1.28 | `timeout-minutes` audit with an autofix that inserts a configurable default (#1023). Woodruff: "going to be *very* noisy … marginal security benefit". | Matches upstream #581 and kjanat `require-job-timeout`. Make it an **opt-in policy** with `min-minutes`/`max-minutes`, plus an optional autofix that inserts the value. |
| [#2159](https://github.com/zizmorcore/zizmor/pull/2159) | open, approved with nits | `on.workflow_call.secrets: inherit` (callee side) failed serde's untagged `Trigger` enum, and zizmor then **skipped all audits** for the file. The PR adds a `WorkflowCallSecrets { Inherit, Map }` enum and flags callee-side `inherit`. | (1) **Error containment:** a bad subtree must yield one diagnostic, never drop the whole document. This argues against serde-untagged models for the lint AST. (2) Callee-side `secrets: inherit`: **verify** against `workflow-v1.0.json` before accepting it. actionlint and kjanat only model the caller side. |
| [#2140](https://github.com/zizmorcore/zizmor/pull/2140) | draft | Foundation for a sound "self-hosted" check. `support/github-runners.py` scrapes `docs.github.com/api/article/body?pathname=/en/actions/reference/runners/github-hosted-runners` and `…/larger-runners` into `data/github-runners.txt`, compiled into an FST in `build.rs`. Planned API: `is_self_hosted() -> Vec<SelfHosted{Yes(loc), Indeterminate}>` for `runs-on: ${{ cond && 'a' \|\| 'b' }}`. | (1) The docs article-body API is a usable machine source for runner labels; GitHub has no other. (2) Evaluate `runs-on` to a **set of possible values** (constant-fold `&&`/`\|\|` and matrix), not "expression ⇒ skip". This also fixes upstream #656 and kjanat #160 (prefixed Windows labels such as `self-32vcpu-windows-2022` get bash ShellCheck). |
| [#2108](https://github.com/zizmorcore/zizmor/pull/2108) | open | An empty or comment-only `zizmor.yml` was fatal. It is now treated as defaults. | The config loader treats an empty, null or `{}` document as defaults. |
| [#2081](https://github.com/zizmorcore/zizmor/pull/2081) | open | `--select <RULE>` (repeatable) runs only the listed audits and overrides config `disable`. Woodruff accepts the semantics but first wants to refactor how CLI, env and config state merge. | Ruff-style `--select`/`--ignore`/`--extend-select` on the CLI. Build **one layered settings resolver** (defaults → config chain → env → CLI) before adding flags. zizmor is now paying for not having one. |
| [#1927](https://github.com/zizmorcore/zizmor/pull/1927) | draft | Reproducer for #1914: a `bot-conditions` false positive whose autofix changes behaviour. | Autofixes must be semantics-preserving or marked unsafe (Ruff's safe/unsafe split). |
| [#1792](https://github.com/zizmorcore/zizmor/pull/1792) | open, approved | `secrets-outside-env` also flags `secrets.*` at reusable-workflow call sites, because a reusable call cannot activate an environment. | Security domain; skip for v1. Notes that reusable-call jobs cannot set `environment:`; schema-level, already covered. |
| [#1711](https://github.com/zizmorcore/zizmor/pull/1711) | open | Part 1 of splitting #1677: clap-derive models of `docker pull/run/create` to extract the image argument. Handles `-dit`, `--flag=value`, and the `-a` ambiguity. | If workflint ever parses commands inside `run:`, model each CLI properly instead of ad-hoc flag splitting. Later. |
| [#1696](https://github.com/zizmorcore/zizmor/pull/1696) | open | Ruff-style `--quiet` (findings only) and `--silent` (nothing, exit code only). | Copy Ruff's verbosity levels: `-q`, `-qq`/`--silent`, `-v`. |
| [#1677](https://github.com/zizmorcore/zizmor/pull/1677) | changes requested | Detect unpinned `docker`/`podman` images in `run:` using tree-sitter-bash. Motivated by agent-generated workflows. Woodruff asked for it to be split into PRs of at most ~500 lines. | A shell AST for `run:` scripts unlocks many rules: unpinned images, kjanat's executable-bit check (it uses `mvdan.cc/sh`), workflow-command injection (kjanat #78), deprecated commands. Plan a bash parser crate. |
| [#1666](https://github.com/zizmorcore/zizmor/pull/1666) | draft | TOML config (#1560), using `serde_json::Value` as the shared intermediate representation. | workflint must read actionlint's YAML config for compatibility. TOML (`workflint.toml`, `[tool.workflint]`) can come later. Keep one typed model behind both formats. |
| [#1655](https://github.com/zizmorcore/zizmor/pull/1655) | open, no review | `agentic-actions` audit for Claude Code / Codex / Gemini actions: wildcard `allowed_bots`, `Bash(*)`, `sandbox: danger-full-access`, untrusted triggers, attacker-controlled prompts. | Security domain; skip for v1. Make sure the bundled action catalogue includes these AI actions' inputs, so `with:` validation works. |
| [#1621](https://github.com/zizmorcore/zizmor/pull/1621) | draft | Flag missing `concurrency` when every job is a reusable call (#1619). | Skip. |
| [#1609](https://github.com/zizmorcore/zizmor/pull/1609) | open | Detect GitHub rate limiting (403 with `x-ratelimit-remaining: 0`) as a distinct error. Retry only on 5xx/429/408. | Online mode: a distinct rate-limit error with remediation, and a narrow retry classifier. |
| [#1584](https://github.com/zizmorcore/zizmor/pull/1584) | open, stalled | Deduplicate findings duplicated by YAML anchors/aliases (#1583). Uses custom `Hash` impls, which Woodruff worries are brittle, plus a test-only env var to disable dedup. | **Decide the anchor policy up front.** Analyse each alias use in its own context, but report at the anchor with an alias note (or the reverse, as kjanat does: report at the alias use and mention the anchor). Dedupe on (rule, primary span, message) in one place in the diagnostic sink, not per rule. |
| [#1559](https://github.com/zizmorcore/zizmor/pull/1559) | open | Globs in `uses:` repository patterns (`owner/prefix-*`, `owner/*/sub-*`), with specificity ordering where exact beats glob. | `required-actions`, pinning policies and allow/deny lists should accept globs with defined specificity. kjanat's `required-actions` already does (`*` does not cross `/`). |
| [#1151](https://github.com/zizmorcore/zizmor/pull/1151) | open, old | Docker image `uses:` patterns in audit config (#737). | Later. Pinning policy for `docker://` uses digests (kjanat `require-commit-hash` checks `@sha256:`). |
| [#1010](https://github.com/zizmorcore/zizmor/pull/1010) | open, `triage` | An empty `ZIZMOR_CONFIG=` is treated as unset. | Every `WORKFLINT_*`/`ACTIONLINT_*` env var: empty means unset. Document it. |

### Signals from zizmor's open PRs

1. **YAML scalar fidelity is the recurring pain** (#2423, #1584, #2159). zizmor built on tree-sitter-yaml plus serde-yaml twin models, and its maintainer calls the abstractions brittle. workflint should own one CST/AST with lossless spans and decoded-value maps.
2. **Settings plumbing debt** (#2081, #2233, #1010, #2108, #1666). Design the rule registry, aliases and a layered config resolver before the first release.
3. **Runner awareness** (#2140, #2405): a sound tri-state "self-hosted / hosted / indeterminate" query with label data generated from docs.
4. **Online-mode hygiene** (#2318, #1609, #2364): per-host auth, rate-limit errors, RFC 9111 HTTP cache.
5. **Script ASTs** (#1677, #1711; merged #2444, a PowerShell grammar): zizmor is investing in parsing `run:` bodies for bash and PowerShell.

### Recent merged PRs (last 30, 2026-09-24 → 2026-10-15), showing direction

- **cache-mode support:** #2418 adds a `CacheMode { Read, Write, WriteOnly, None }` enum at workflow and job level, with an "effective cache mode" (job overrides workflow; a symbolic location is kept for diagnostics). #2426 adds `cache-poisoning` findings for explicit `write`/`write-only` under dangerous triggers. This is the same feature kjanat shipped on 2026-09-13 (see §b). Trigger: GitHub changelog 2026-09-10, [cache-mode](https://github.blog/changelog/2026-09-10-control-github-actions-cache-access-with-cache-mode/).
- **#2414 `Trigger` refactor:** there were "three different ways to query whether a given trigger is present". Model `on:` as one normalised structure from day one.
- **#2444:** switched to `woodruffw-trough/tree-sitter-powershell`, hill-climbed against PowerShell itself as the grammar oracle. PowerShell `run:` scripts are first-class for zizmor.
- **#2438:** a bot PR auto-updates `context-capabilities.csv` when GitHub webhooks change. Data refresh is automated, with review.
- **#2437, #2413:** typosquat checks for pre-commit/prek inputs and prek `language_version` maps. zizmor now audits **pre-commit configs** too (`plat:pre-commit`), so its scope is widening beyond GitHub Actions.
- **#2409:** `pnpm/setup` added to cache-aware actions. **#2420:** Windows ARM64 tests. **#2425:** dropped Python 3.10 (PyPI wrapper). **#2429:** `zizmor-dev` test-helper crate. **#2428:** better diff snapshots (insta). Release **1.31.0** (#2446).

---

## (b) kjanat/actionlint: overview

**What it is.** kjanat/actionlint is a hard fork of `rhysd/actionlint`, branched at **v1.7.12** (`914e7df`). Its own release series began at **v1.8.0 on 2026-08-19** and reached **v1.17.0 on 2026-09-13**, with a large `Unreleased` section on master. That is 10 releases in 4 weeks. Kaj Kowalski wrote 293 commits. There are 136 merged PRs, almost all by kjanat, plus 2 by @johnament and 1 by @muzimuzhi. Development is openly AI-assisted: CodeRabbit writes issue plans, Codex reviews PRs, a "Comment Cop" bot checks prose, and `CONTRIBUTING.md#ai-usage-and-policy` covers the policy.

**Scale.** Go code grew from **20.3k → 44.9k lines** (non-test) and tests from **13.6k → 63.0k lines**. The module moved to the vanity path `actionlint.kjanat.dev`. The Go library API is declared unstable; the CLI and config are semver.

**Upstream tracker.** `UPSTREAM.md` triages 189 upstream issues and PRs with dispositions: 84 implemented, 37 candidates, 8 still affecting the fork, 24 not applicable. It is a useful cross-check for 05-issues.md (see the disagreements in §d).

**Strategic note.** kjanat issue [#69](https://github.com/kjanat/actionlint/issues/69) (open) is titled "Explore a Rust analysis core behind a narrow Go FFI boundary". Its reasoning: enums with payloads, exhaustive matching and span-heavy structures fit Rust. The author also maintains [`kjanat/shellcheck-rs`](https://github.com/kjanat/shellcheck-rs) ("rshellcheck", an experimental Rust port of ShellCheck at v0.11.0, **GPL-3.0**). The fork's pre-commit hook already uses it as a native binary (#271). This makes kjanat both the most direct competitor and a potential collaborator.

### Feature table

Recommendations: **v1** = include in workflint 1.0; **later** = post-1.0; **skip** = don't do.

| # | Area | What kjanat does | Where | workflint |
|---|---|---|---|---|
| 1 | **Composite actions as first-class inputs** | Discovers `action.yml`/`action.yaml` alongside workflows, including unreferenced and action-only repos (#243, #267). Discovery honours `.gitignore` (#274/#289). Validates `runs.steps` shapes: `run`+`shell` or `uses`. Rejects unknown keys such as `parallel:`/`timeout-minutes:`, non-string `run`/`shell`/`uses`, null steps, and reusable-workflow `uses` (#70). Errors are reported in `action.yml`, not at the caller (#70). Composite scripts run through ShellCheck/Ruff with their own shell and working directory, without inheriting workflow defaults (#184; `composite_scripts.go:17`). | `composite_analysis.go`, `composite_scripts.go`, `action_metadata_*.go` | **v1.** Matches upstream #366/#46 and 05's "core". Model actions and workflows as two document kinds sharing step and expression machinery. |
| 2 | Composite expression context availability | `secrets`/`vars`/`needs` are not available in composite steps. Input defaults get a narrower context set. Covers `shell`, `continue-on-error`, whole `with`/`env`, and status-function availability (#124, #131, #133). | generated from runner `action_yaml.json` (row 6) | **v1.** Generate it; don't hand-write it. |
| 3 | `missing-local-action` | Default-on: a `uses: ./x` with no manifest is an error, where upstream silently skipped validation (#196, #250). `local-actions:` config maps generated action dirs to source dirs. | `local_action_paths.go`, `config_local_actions.go` | **v1** for the rule. **Later** for the mapping config. |
| 4 | **cache-mode** | `cache-mode: read\|write\|write-only\|none` at workflow, job and reusable-call job level. Invalid values are reported at their source position (#163, #164). | `cache_mode.go:95` | **v1** (new schema key, GA 2026-09-10; upstream PR #745). |
| 5 | **Cache safety policies (default-on)** | `cache-write-untrusted`: explicit `write`/`write-only` on low-trust triggers that use default-branch caches. `cache-call-unrestricted`: reusable calls on those triggers without an explicit cache ceiling. `cache-operation`: `actions/cache/save` under `read`/`none`, `restore` under `write-only`/`none`, `cache` under `none`, followed through nested local reusable workflows. Each trigger is classified Restricted, RefScoped, Trusted or Inherited (`rule_cache_policy.go:31-62`). Respects `job.if` event guards (#198, #266). | `rule_cache_policy.go`, `cache_event_guard.go`, docs/config.md §Policy checks | **v1** for `cache-operation` (correctness: the step silently does nothing). **Opt-in in v1** for the two trust policies. kjanat turning them on by default made previously clean CI fail (its own upgrade note), and zizmor covers the security side. Reuse the event-trust table. |
| 6 | **Schema conformance against GitHub's sources** | (a) Workflow schema audit of all 324 definitions against **`actions/runner@759385a` `src/Sdk/WorkflowParser/workflow-v1.0.json`** and **`actions/languageservices@4043eda` `workflow-parser/src/workflow-v1.0.json`**. The two disagree; Language Services has the newer public fields (docs/schema-audit-nested.md). (b) Action metadata rules generated from **`actions/runner` `src/Runner.Worker/action_yaml.json`** (`scripts/generate-action-metadata/main.go:23-24`). (c) JS runtimes and deprecation/removal dates scraped from **`ActionManifestManager.cs`, `ActionManifestManagerLegacy.cs`, `src/Misc/externals.sh`, `src/Runner.Common/Constants.cs`** (`runtimes.go:19-24`). (d) Context availability from **`github/docs` `content/actions/reference/workflows-and-actions/contexts.md`** (`generate-availability/main.go:30`), plus runner-only fallbacks for `cancel-timeout-minutes`, `snapshot`, `snapshot.if` (`:240`). (e) Step-ID rules from runner `Conversion/IdBuilder.cs`. (f) YAML tag semantics from runner `Conversion/YamlObjectReader.cs` (`parse.go:1059-1061`). | `docs/schema-audit*.md`, `scripts/generate-*` | **v1.** Pin and generate from exactly these sources. Record source disagreements explicitly, as kjanat does. |
| 7 | **Upstream conformance test corpus** | `go test -tags conformance` runs pinned archives with SHA-256 checks (`internal/conformance/sources.json`): **actions/languageservices** (1,020 expression fixtures, 195 workflow-reader fixtures, 13 language-service cases), **actions/runner@602c008** (20 `ActionManifestManagerL0.cs` and 4 `ExpressionParserL0.cs` cases), **SchemaStore@7d8659e** (67 workflow/action fixtures), **yaml-test-suite@6ad3d2c** (402 streams). Result: 1,721 cases, 1,394 agreements, 327 recorded differences in `testdata/conformance/differences.json`. CI fails on any new, changed or vanished difference. | `docs/conformance.md`, `upstream_conformance_test.go` | **v1.** Adopt the same corpus and the same exact-baseline approach. It is the best available oracle. |
| 8 | Hosted expression probe | Submits expressions to real GitHub Actions and records evaluated results (85 cases): numeric literal forms, coercion, `fromJSON`. Findings were filed upstream (actions/runner#4724, github/docs#45881). | `docs/expression-behavior.md`, `scripts/expression-probe`, issue #47 | **v1** (reuse the archived results as test vectors). **Later** for our own probe workflow. |
| 9 | **Exact ShellCheck/Ruff/expression positions** | `internal/yamlsource` re-scans each scalar token and records per-byte `Span{Start,End,Line,Column,Width}` covering escapes, folding and line breaks (`scalar.go:15-30, 52`). It maps ShellCheck, Ruff and expression errors to exact positions across all scalar styles, tags, anchors and CRLF (#2, #252). Falls back to the `run:` key when unmappable. Expression errors highlight the failing node, with zero-width ranges at EOF. | `internal/yamlsource/scalar.go`, `parse_scalar_source.go:74` | **v1, by design.** Build the source map into our YAML layer (same conclusion as zizmor #2423). The fallback-to-key rule for unmappable cases is good. |
| 10 | YAML semantics fixes | Matrix scalars are typed by YAML tag under the YAML 1.2 core schema: quoted, `!!str` and block scalars are strings, so `"3.10"` ≠ `3.10` and `0b10`/`1_000` stay strings (#38). Matrix values compare by kind and resolved value (#57). Exclusions are validated against original rows, not `include` additions (rhysd #634). Anchor names containing `+` are rejected (`parse.go:154`, #114). Alias type errors are reported at each alias use, with the anchor location in the message (`parse.go:118`, #149). `True` decodes as true and `0x1e`/`0o36` are numeric. | `parse.go`, `matrix_*` | **v1.** All are GitHub-behaviour facts. |
| 11 | New syntax | `background`/`wait`/`wait-all`/`cancel`/`parallel` steps (#695); `concurrency.queue` (`single`/`max`; `max` with `cancel-in-progress` is an error); service `entrypoint`/`command`; root `description`; `cancel-timeout-minutes`; `snapshot`; `image_version` event; `stacked` PR activity type; `issues.field_added/field_removed`; empty choice options; `services.x.image: ''` (disabled service); UTC timezone aliases; `copilot-requests`, `drives`, `code-quality`, `vulnerability-alerts` permissions with per-scope levels; `job.workflow_*`; `jobs.<id>.result`. Open PR #298: `${{ insert }}` mapping insertion. | `all_webhooks.go:22-33`, CHANGELOG v1.8–v1.17 | **v1** (all of it). `${{ insert }}` is **v1 accept-only**: the runner has `InsertExpressionToken.cs`, and it is upstream issue #349. |
| 12 | Typed `uses` model | A `UsesReference` sum type: Repository, Workspace (`./`), SelfRepository (`$/`), Container (`docker://`), Builtin, Unknown (`uses_reference.go:51-107`). `$/` root actions and extra slashes are accepted; bare `$/` is rejected (#199, #200, #30). | `uses_reference.go` | **v1.** Matches 05's "typed `Uses` enum". |
| 13 | **Rule catalogue and config v2** | `lint.rules` with groups `correctness`/`suspicious`/`security`/`policy`/`external`/`nursery`; presets `recommended`/`all`/`none`; levels `off`/`on`/`info`/`warn`/`error`/`default`; per-rule typed `options`; `--strict`/`--experimental`. `overrides:` are per-file glob blocks (Ruff/Biome style). `extends:` composes local files (max depth 32, cycle detection). `files.includes`/`excludes`. `ignores:` select by rule plus file/uses/job/step, with a reason. `ACTIONLINT_*` env defaults. `actionlint rules --json`. The legacy top-level `policy:`, `paths:` and `self-hosted-runner` keys still work. | `config_*.go`, `rule_registry.go`, docs/config.md | **v1** for the model: groups, levels, presets, overrides, select. Read legacy actionlint config unchanged. **Later** for `extends`. |
| 14 | Generated config JSON Schema | `actionlint.schema.json` is generated from the Go types (#120). `-init-config` writes a `yaml-language-server` directive. The schema ships in the npm package. | `scripts/generate-config-schema`, `actionlint.schema.json` | **v1** (`schemars`). Publish to SchemaStore. |
| 15 | **Inline suppressions** | `# actionlint:ignore RULE[,RULE] -- reason` on a line and `# actionlint:ignore-next-line RULE -- reason`. A directive on a block-scalar header covers the body. Malformed directives raise the `inline-suppression` rule. Directives in composite YAML are scoped to that file. The `disallow-suppressions` policy has `report: all\|suppression\|violation`. | `inline_suppression.go:48-58`, docs/config.md §Inline | **v1.** Accept the `actionlint:` prefix as an alias of `workflint:`. `disallow-suppressions` comes **later**. |
| 16 | Opt-in policies | `require-commit-hash` (40 or 64 hex digits; `docker://` must have a digest; skips `./`, `$/` and expressions); `require-job-timeout` (with `min-minutes`/`max-minutes`); `require-permissions` (`scope: workflow\|job`); `required-actions` (globs, case-insensitive names, case-sensitive refs). | `rule_require_*.go`, `rule_required_actions.go` | **v1** for `require-commit-hash` and `require-permissions`. **Later** for the others. These correspond to upstream PRs #436, #581, #474 and issue #525. |
| 17 | Config allow-lists and permission flow | `config-secrets` allow-list (#649). Caller → callee permission check for local reusable workflows, with `assume-default-permissions: restricted\|permissive` (#670). | `workflow_permissions.go` | **v1.** |
| 18 | Suspicious rules (stable, not in `recommended`) | `string-conditions` (a bare string in `if:`; `'false'` is truthy), `mixed-type-comparisons`, `case-insensitive-conditions`, `mixed-type-matrix-filters`. Default-on correctness rules: `unsound-ternary` (`a && <falsy> \|\| c`, `expr_ternary.go:7`) and `workflow-run-names` (`workflow_run.workflows` must name a real workflow in the repo). | docs/config.md §Stable suspicious, §Additional correctness | **v1** for `unsound-ternary` and `workflow-run-names`. **Later** for the suspicious group (as opt-in). |
| 19 | `executable-bit` | Reads git index modes and parses `run:` with `mvdan.cc/sh` to find direct script invocations (`./scripts/x.sh`) that lack `+x`. Accounts for `chmod`, checkout paths and Windows runners. | `rule_executable_bit.go:297-669`, `git_modes.go` | **Later.** Valuable, but it needs a bash AST and a git index reader. |
| 20 | Ruff replaces Pyflakes | Python `run:` scripts are checked with Ruff in isolated mode (no repo config, cache, fixes or `noqa`), with the `F` family by default and JSON output mapped back to YAML. Scripts are skipped when an expression is part of a Python token. Follows **astral-sh/actionlint** #22/#25 by Zanie Blue. Pyflakes was removed (#230, #234). | `rule_ruff.go`, docs/config.md §Ruff | **v1**, Ruff only, with no Pyflakes. Ruff is Rust (MIT); consider a library-level integration later. |
| 21 | ShellCheck configuration | `tools.shellcheck.config` takes inline `disable`/`enable`/`shell`/`external-sources`/`source-path` or an rc path. Path variables: `${{ configdir }}`, `${{ gitdir }}`, `${{ github.workspace }}`, `${{ github.action_path }}`. Dialect precedence is documented. Sourced files resolve from the effective `working-directory`. Templates like `bash -euxo pipefail {0}` are parsed for startup options. | docs/config.md §ShellCheck, `shellcheck_*.go` | **v1** for rc and inline config and dialect selection (upstream #755/#486). **Later** for source-following. |
| 22 | ShellCheck delivery | The pre-commit hook `actionlint-shellcheck` installs **rshellcheck** (native Rust port) instead of the go-shellcheck WASM build (a "70x startup penalty", #89). The playground runs ShellCheck as WASM (#294). | `kjanat/shellcheck-rs` | **Watch.** rshellcheck is **GPL-3.0**: only usable as a subprocess, never linked into an MIT/Apache workflint. It could become our recommended ShellCheck binary. |
| 23 | CLI redesign | Cobra subcommands `check`, `rules`, `doctor`, `completion`, `config` inspect and `version`, with legacy flags preserved. Output formats: JSON (`schema_version`), JSONL, SARIF, `github`, templates. `--output-file` refuses to overwrite inputs. Typed workflow/action documents can be exported in JSON (#222). OSC 8 hyperlinks. jq pretty-printing. Auto-colour on `GITHUB_ACTIONS`. A hidden `pre-commit` subcommand. | `internal/cli`, docs/results.md | **v1** for subcommands, `rules`, `doctor`, JSON with a versioned schema, SARIF and GitHub output. **Later** for the document-model export. |
| 24 | Completions and man page | `-completion bash\|zsh\|fish\|powershell\|auto`. Man page built with pandoc. | `cmd/`, `man/` | **v1** (`clap_complete`, `clap_mangen`). |
| 25 | **Official GitHub Action** | `uses: kjanat/actionlint@v1` is a **Node 24 launcher** that downloads the binary (no Docker; Linux/macOS/Windows). Inputs include `version`, `version-file` (`.tool-versions`), `install-only`, `config` overlay, `shellcheck`, `ruff`, `annotations`, `summary`, `sarif` and `review`. Output is a versioned `result-file`. The `v1` tag is pinned to a digest-pinned commit. | `action.yml`, docs/action.md | **v1.** Composite or JS launcher plus binary download with checksum and attestation verification. Compare with 10-github-action.md. |
| 26 | Distribution | npm `@kjanat/actionlint` (11 platform packages plus launcher, OIDC trusted publishing); PyPI wheels `actionlint-kjanat`; Homebrew cask in `kjanat/tap` with completions; AUR (`-bin`, source, `-git`); Scoop; WinGet (manifests validated against the official schema); aqua registry; mise (`github:`/`aqua:`/`npm:`/`go:` backends); **Nix flake** (packages, checks, devShell, completions, man page, config schema); GHCR and Docker Hub images with SBOM, provenance and `gh attestation verify`. Chocolatey is an open draft (#99). | docs/install.md, `flake.nix`, `distribution/` | **v1:** GitHub releases with attestations, cargo-dist, Homebrew, npm, PyPI (maturin `bin`), aqua/mise (free once release assets are regular), Docker. **Later:** AUR, Scoop, WinGet, Nix flake, Chocolatey. |
| 27 | **Data upkeep automation** | A weekly `Upkeep` workflow runs `go generate` (popular actions, webhooks, availability, runtimes, Ruff selectors) and opens a PR per drift. A **GitHub changelog monitor** opens one issue per `github.blog/changelog` Actions entry; each is evaluated and closed with evidence (#103; issues #125–#220). The popular-actions catalogue is now versioned JSON with a schema, plus 117 action paths (#268, #280). Every generator HTTP call has a deadline. | `.github/workflows/`, `scripts/generate-*`, `internal/actioncatalog` | **v1.** This addresses the root cause behind 05's "stale data" theme. Copy the changelog-monitor idea. |
| 28 | Playground | Rebuilt on Vite with CodeMirror 6 and `@codemirror/lint`; WASM build. | `playground/` | **Later** (Rust → WASM is easier). |
| 29 | Git-native discovery | Draft #275 (+13k lines) ports parts of git's `setup.c`, `config.c`, `dir.c` and `wildmatch.c` into `internal/gitrepo` so that no `git` process is needed. A provenance review is pending (#285/#286). Ideas: lint git trees or bundles without a checkout (#282–#284). | PR #275 | **v1:** use the `ignore` crate (ripgrep) for `.gitignore`-aware discovery. **Later:** `gix` for tree or remote-revision linting. |
| 30 | Diagnostics UX | Suggests `github.repository` for `github_repository` and `job.workflow_sha` for `github.job_workflow_sha` (#206, #224). Suggests `always()` for bare `always` (#226). Suggests `.github` for `.gihub` typos (#191, open). | `expr_sema.go:765` | **v1:** "did you mean" via edit distance on contexts, properties and functions (kjanat #262 is the general version). |

---

## (c) kjanat open PRs, issues and merged PRs

### Open PRs (5)

| PR | Status | What | Signal |
|---|---|---|---|
| [#298](https://github.com/kjanat/actionlint/pull/298) | open, +945/−118 | `${{ insert }}` mapping insertion in `env:` (workflow, job, step, container, service), `with:` and matrix. Includes one-level splicing of expression-produced matrix arrays. `case()` predicates must be Boolean. Evaluation-dependent findings are suppressed in unreachable `case()`, `&&` and `\|\|` branches. Structural job/step templates stay rejected. | Accept `${{ insert }}` keys in mappings (runner `InsertExpressionToken.cs`; upstream #349). Reachability-aware suppression in short-circuit branches is a good precision idea. |
| [#275](https://github.com/kjanat/actionlint/pull/275) | draft, +13,248 | `internal/gitrepo`: a clean-room port of git discovery, config, ignore and index reading, tested against the `git` binary. `executable-bit` reads modes through it. | The pull-in vs. reimplement trade-off. In Rust, `gix` already covers this. |
| [#264](https://github.com/kjanat/actionlint/pull/264) | draft, merge after 2026-11-02 | Retire the `macos-14*` labels. | Label data needs **dated lifecycle states** (available → deprecated → retired on date X), not just add/remove. |
| [#209](https://github.com/kjanat/actionlint/pull/209) | open | Docs: drop the Trunk warning (trunk-io/plugins#1181 switches Trunk's plugin to the fork). | Meta-linters (Trunk, MegaLinter, super-linter) are switching to maintained forks. Plan plugin PRs for workflint. |
| [#99](https://github.com/kjanat/actionlint/pull/99) | draft, +4,217 | Chocolatey package (a Windows job; embedded binary with VERIFICATION.txt). The `actionlint` id there is stale at 1.6.25. | Chocolatey is underserved. Later. |

### Issues (118 total: 41 open, 77 closed)

There are 28 bot-opened "Changelog:" issues from the GitHub changelog monitor; each one is evaluated against the code. The rest group as follows.

**Open rule and feature ideas worth tracking for workflint:**

- [#296](https://github.com/kjanat/actionlint/issues/296): flag *any* `${{ }}` interpolated into `run:`, including `env.*`/`secrets.*`, as a distinct configurable diagnostic separate from "known untrusted". Matches upstream #332/#345/#96. → workflint: an opt-in `script-interpolation` rule.
- [#295](https://github.com/kjanat/actionlint/issues/295): `github/gh-actions-lock`, GitHub's new action lockfile tool. → Investigate. A lockfile-aware pinning policy may replace "require SHA".
- [#190](https://github.com/kjanat/actionlint/issues/190): `env.FILE` declared only in an earlier **step's** `env:` is used by a later step and silently evaluates to empty. → **v1 correctness rule** (step-scoped env tracking; allow `GITHUB_ENV` exports conservatively).
- [#84](https://github.com/kjanat/actionlint/issues/84): unquoted `python-version: 3.10` becomes float `3.1`. Proposed: flag `with:` scalars tagged `!!float` whose source has a trailing zero. → **v1** (cheap, high value).
- [#159](https://github.com/kjanat/actionlint/issues/159): `uses: ./ }` and trailing commas after flow→block conversion are legal YAML plain scalars but are almost certainly typos. → **later**, as a suspicious rule.
- [#160](https://github.com/kjanat/actionlint/issues/160): OS inference from prefixed custom labels (`self-32vcpu-windows-2022`) fails, so PowerShell gets linted as bash (seen in zed-industries/zed). → **v1**: OS inference via a configurable label→OS map plus substring heuristics; never ShellCheck a script when the OS is unknown and no shell is set.
- [#78](https://github.com/kjanat/actionlint/issues/78): values interpolated into `::warning::` and similar workflow commands without `%0A` encoding allow forged annotations. [#79](https://github.com/kjanat/actionlint/issues/79): a workspace-owning `actions/checkout` running in a `parallel` group alongside other steps (a race, measured). [#88](https://github.com/kjanat/actionlint/issues/88): cache writes on untrusted triggers are silently dropped (since 2026-06-26). → later; #88 overlaps the cache policies.
- [#262](https://github.com/kjanat/actionlint/issues/262): spelling suggestions. [#260](https://github.com/kjanat/actionlint/issues/260): **split the monolithic `expression` rule into sub-rules** (syntax, undefined, type, availability, injection) that can be configured independently. → **v1 design decision**: give workflint fine-grained rule IDs, with `expression` as a group alias for actionlint compatibility.
- [#189](https://github.com/kjanat/actionlint/issues/189): a richer JSON report (file inventory, job/step structure, effective shell). [#23](https://github.com/kjanat/actionlint/issues/23): a generic **external script-linter bridge** (PSScriptAnalyzer for pwsh, eslint/biome for `actions/github-script`, …). → later; design the trait now.
- [#33](https://github.com/kjanat/actionlint/issues/33): generate permission scopes and levels from `languageservices/workflow-parser/src/workflow-v1.0.json`. → **v1** (generate, don't hand-maintain).
- [#157](https://github.com/kjanat/actionlint/issues/157), [#158](https://github.com/kjanat/actionlint/issues/158): popular-actions selection should use **dependency-graph dependents counts**. New majors should be discovered with `git ls-remote --tags` instead of HEAD-probing a guessed `next` tag; HEAD-probing has 4 blind spots and 6 actions are never checked. → **v1 data pipeline design**.
- [#269](https://github.com/kjanat/actionlint/issues/269): **Dockerfile-only actions**. The runner falls back to `Dockerfile` when there is no `action.yml` (`ActionManager.cs#L875-L912`). → v1: don't report a missing manifest when a `Dockerfile` exists.
- [#3](https://github.com/kjanat/actionlint/issues/3), [#74](https://github.com/kjanat/actionlint/issues/74): go-yaml's `Node` has no raw text, end marks or decode map, which is why kjanat wrote `yamlsource`. In the issue thread, kjanat states the principle: "GitHub.com's hosted workflow parser is the final authority … conformance to the [YAML] specification is not an independent goal."
- [#69](https://github.com/kjanat/actionlint/issues/69): a Rust analysis core (see §b). [#21](https://github.com/kjanat/actionlint/issues/21): split the Go root package into `ast`, `expr` and `rule`.
- [#172](https://github.com/kjanat/actionlint/issues/172): an action `ignore:` input regex was misunderstood (the quotes became part of the regex). → UX: prefer rule-ID/selector ignores over message regexes.
- [#173](https://github.com/kjanat/actionlint/issues/173), [#174](https://github.com/kjanat/actionlint/issues/174), [#175](https://github.com/kjanat/actionlint/issues/175), [#177](https://github.com/kjanat/actionlint/issues/177): verification findings. `snapshot.if` accepts `github, vars, inputs, needs, strategy, matrix, steps, job, runner, env` but not `secrets`, and is missing from the docs table. `github.job_workflow_sha` was removed from the docs and evaluates to null; the replacement is `job.workflow_sha`. Numeric literal parsing still needs reconciling with measured behaviour. Expressions in `uses:` are unmeasured.

**Closed issues that are useful evidence:** [#24](https://github.com/kjanat/actionlint/issues/24) (`$/` semantics from runner `PipelineConstants.cs#L40-L76`: `TryParseSelfRepository` trims leading `/`, and an empty path is invalid); [#58](https://github.com/kjanat/actionlint/issues/58) (the runner refused a composite with a `parallel` key that actionlint passed); [#47](https://github.com/kjanat/actionlint/issues/47) (a register of docs/schema/runtime disagreements); [#163](https://github.com/kjanat/actionlint/issues/163) (cache-mode table); [#178](https://github.com/kjanat/actionlint/issues/178) (workflow execution protections are a repository setting, not YAML syntax); [#90](https://github.com/kjanat/actionlint/issues/90) and [#199](https://github.com/kjanat/actionlint/issues/199) (zizmor's `self-repository` audit pushes users to `$/`, and old actionlint rejects it. **zizmor compatibility drives linter requirements.**)

### Merged PRs (136) at a high level

- **Wk 1 (Aug 19–23):** fork bootstrap; upstream PR backlog ported (parallel steps, `$/`, queue, permissions, `job.workflow_*`, labels, popular actions, deadlock fix); exact ShellCheck positions (#2); prebuilt Docker action (#5); policies (#40–#44); completion (#45); matrix tag typing (#38); expression conformance probe (#35).
- **Wk 2 (Aug 28–Sep 5):** composite step validation (#70); digest-pinned action image (#85); distribution to npm, Scoop, AUR and WinGet (#95–#98); changelog monitor (#103); Upkeep drift workflow (#108); config JSON schema (#120); Homebrew tap (#121).
- **Wk 3 (Sep 7–13):** composite context checks (#124, #131); action rules generated from the runner schema (#133); runtime lifecycle (#134); upstream conformance tests (#147); Nix flake (#151); alias diagnostics (#154); cache-mode and cache policies (#164, #165).
- **Wk 4–8 (Sep 20–Oct 15):** CLI redesign (#155); executable bits (#183); composite scripts (#184); Node 24 action launcher (#185); unified JSON results (#201); typed document export (#222); grouped lint rules, overrides and suppressions (#231–#233); Ruff (#234); unsound-ternary and workflow-run-names (#235); reusable ignores (#250); precise metadata expression spans (#252); PyPI wheels (#270); rshellcheck pre-commit hook (#271); versioned action catalogue (#280); gitignore-aware discovery (#289); ShellCheck WASM in the playground (#294).

### Related: astral-sh/actionlint

Astral maintains a fork too (pushed 2026-10-13). Its 22 merged PRs, all from August 2026, cover Ruff (#22), Pyflakes removal (#25), `issues.field_added/field_removed` (#10), `$/` (#7, credited to kjanat), parallel steps, queue and permissions. Open PRs cover a release-hardening series: CycloneDX SBOM, signed GHCR, protected environments, hash-pinned build dependencies. Signals: a **third** maintained fork, and Astral is a likely workflint user if it is fast and Ruff-native.

---

## (d) Consolidated checklist: open-PR findings workflint should include

This list merges 05-issues.md §3 (upstream `rhysd/actionlint` open PRs, written as `rhysd#N`) with the zizmor (`zz#N`), kjanat (`kj#N`, issues `kj-i#N`) and Astral (`astral#N`) findings above. jactionlint and zizmor-action PRs are in 09 and 10; append them here when those land. **P** = priority: v1, later, or ✗ (do not include).

### Schema, syntax and data

| Item | Sources | P |
|---|---|---|
| `cache-mode` (`read\|write\|write-only\|none`) at workflow, job and reusable-call job; effective mode inherited job → workflow | rhysd#745, kj#164, zz#2418 | v1 |
| Parallel steps: `background`/`wait`/`wait-all`/`cancel`/`parallel`, plus a reference rule | rhysd#695/#694, kj v1.9.0, astral#8 | v1 |
| `concurrency.queue` (`single`/`max`; `max` + `cancel-in-progress: true` is an error) | rhysd#654, kj v1.8.0 | v1 |
| `$/` self-repo `uses:` (actions and reusable workflows; root `$/` valid; bare-slash invalid; no `@ref`) as a typed `Uses` enum | rhysd#732, kj#200/#30, kj-i#24, astral#7 | v1 |
| `${{ insert }}` mapping keys in `env:`, `with:` and matrix (accept; validate the value's object shape) | kj#298, rhysd issue #349 | v1 |
| Newer schema fields: root `description`, `cancel-timeout-minutes`, `snapshot`/`snapshot.if`, service `entrypoint`/`command`, `services.x.image: ''`, empty choice options, UTC tz aliases | kj v1.17.0 audit, rhysd#645 | v1 |
| Events: `merge_group.destroyed`, `pull_request(_target).stacked`, `issues.field_added/field_removed`, `image_version` (+ `names`/`versions` filters, `created/ready/deleted`) | rhysd#740, kj schema audit, astral#10 | v1 |
| Permissions generated from `workflow-v1.0.json`, with per-scope allowed levels: `copilot-requests`, `code-quality`, `vulnerability-alerts` (read/none), `drives`, `models` | rhysd#689/#674/#658/#666, kj#28, kj-i#33 | v1 |
| Contexts: `job.workflow_ref/sha/repository/file_path`, `jobs.<id>.result` in `workflow_call` outputs, `github.artifacts(_list)`; `github.retention_days` is a string | rhysd#707/#696/#724, kj v1.9/v1.14 | v1 |
| `github.job_workflow_sha`: **do not add to the typed context.** Emit "did you mean `job.workflow_sha`". kjanat measured it as OIDC-only and null in expressions; this **revises 05's "Include"**. | rhysd#671, kj-i#174, kj UPSTREAM.md | v1 (as a hint) |
| Runner labels: `ubuntu-26.04(-arm)`, `xcode-27(-xlarge)`, `windows-11-vs2026-arm`, `macos-26*`; dated lifecycle (`macos-14*` retire 2026-11-02); moving aliases (`ubuntu-latest` ∈ {24.04, 26.04} during migration); generate from the docs article API | rhysd#743/#744/#723/#683/#710, kj#264, zz#2140 | v1 |
| JS action runtimes and deprecation/removal dates generated from runner C# sources; Node 20 removed | kj#134 | v1 |
| Action metadata rules generated from runner `action_yaml.json`; composite step schema; context availability for composite steps and input defaults | rhysd#366, kj#70/#124/#131/#133 | v1 |
| Popular-actions dataset: versioned JSON, refreshed automatically, majors discovered via `git ls-remote`, selection by dependents count, keyed by major **and** exact version, alerting on failure; include AI-agent actions | rhysd#718/#668/#652, kj-i#157/#158, kj#280, zz#1655 | v1 |
| Callee-side `on.workflow_call.secrets: inherit`: **verify** against both schemas before accepting | zz#2159 | verify |
| `runs.env` in composite actions | rhysd#513 (kj: Docker-only) | ✗ |
| `if:` under event triggers (`on.<event>.if`) | rhysd#357 (kj: invalid) | ✗ |
| Forgejo/Gitea URL `uses:` only behind an explicit platform mode | zz#2318 comments, rhysd issue #698 | later |

### YAML, positions and parsing

| Item | Sources | P |
|---|---|---|
| Decoded-scalar ↔ source span map for all styles (`''`, `\` escapes, folding, chomping, CRLF, tags, anchors). Used by expressions, ShellCheck/Ruff remaps, suppressions and autofix; fall back to the key span when unmappable | zz#2423, kj#2/#252, rhysd#556, kj-i#3 | v1 |
| Autofix re-encodes replacement text for the target scalar style | zz#2423 (`RewriteScalarFragment`) | v1 |
| Tag-aware scalar typing under the YAML 1.2 core schema (`!!str`, quoted and block scalars are strings; `0b10`/`1_000` are strings; `True` is true) | rhysd#727, kj#38/#57 | v1 |
| Anchors: reject `+` in names; report alias type errors at the use site with the anchor location; dedupe identical diagnostics centrally | kj#114/#154, zz#1584 | v1 |
| Error containment: a malformed subtree produces one diagnostic and the rest of the file is still linted (no all-or-nothing deserialisation) | zz#2159, rhysd#709 (by design) | v1 |
| Integer literals > int32; `+`/leading zero in the exponent; reconcile with measured numeric behaviour | rhysd#733/#708, kj#29, kj-i#175 | v1 |
| Unquoted version-like floats in `with:` (`3.10` → `3.1`) | kj-i#84 | v1 |
| Stray `}`/`,` in plain scalars after flow→block edits | kj-i#159 | later |

### Rules and semantics

| Item | Sources | P |
|---|---|---|
| Lint `action.yml` as a first-class document (discovery, unreferenced actions, composite scripts with their own shell and working directory, errors reported in `action.yml`) | rhysd#366, kj#70/#184/#243/#267 | v1 |
| `missing-local-action` (default-on); Dockerfile-only actions are valid | kj#250, kj-i#196/#269 | v1 |
| Caller/callee permission check for local reusable workflows + `assume-default-permissions` | rhysd#670, kj#41 | v1 |
| `secrets` type stays open in mixed-trigger workflows | rhysd#703, kj v1.9.0 | v1 |
| `workflow_run.head_branch`, `head_repository.owner.login` (and `**` keys) as untrusted inputs | rhysd#737, kj#37 | v1 |
| `cache-operation` (cache action disabled by the effective mode, followed through local reusable workflows) | kj#165 | v1 |
| `cache-write-untrusted` / `cache-call-unrestricted` with job-`if` event guards (opt-in in workflint) | kj#165/#266, zz#2426 | v1 (opt-in) |
| `unsound-ternary`, `workflow-run-names` | kj#235 | v1 |
| Step-scoped `env` used by a later step | kj-i#190 | v1 |
| Evaluate `runs-on` to a set of possible values; tri-state hosted/self-hosted; OS inference from prefixed labels | zz#2140/#2405, kj-i#160, rhysd issue #656 | v1 |
| Suspicious group: string conditions, mixed-type comparisons, case-insensitive conditions, mixed-type matrix filters | kj docs/config.md | later (opt-in) |
| Reachability-aware suppression in short-circuit and `case()` branches | kj#298 | later |
| `executable-bit` (git index modes + bash AST) | kj#183 | later |
| Generic script interpolation (`env.*`/`secrets.*` in `run:`) as a separate opt-in rule | rhysd#332, kj-i#296 | later (opt-in) |
| Workflow-command injection (`::error::` without encoding); checkout race in `parallel` groups | kj-i#78/#79 | later |
| Unpinned images in `docker`/`podman` commands in `run:` (proper CLI models) | zz#1677/#1711 | later |
| Agentic-action configuration audit, secrets in reusable calls, concurrency for reusable-only workflows | zz#1655/#1792/#1621 | ✗ (zizmor's domain) |

### Policies (opt-in)

| Item | Sources | P |
|---|---|---|
| `require-commit-hash` (40/64 hex; `docker://` digest; skip `./`, `$/` and expressions) | rhysd#436, kj#40 | v1 |
| `require-permissions` (`scope: workflow\|job`) | kj#143, rhysd issue #525 | v1 |
| `require-job-timeout` with `min`/`max`; optional fix that inserts a default | rhysd#581, kj#44/#143, zz#2166 | later |
| `required-actions` (globs; `*` does not cross `/`) | rhysd#474, kj#43 | later |
| Glob patterns with exact-beats-glob specificity in all `uses` policy lists | zz#1559 | v1 (the shared matcher) |
| `known-secrets` / `config-secrets` allow-list | rhysd#649, kj#42 | v1 |
| `disallow-suppressions` | kj#165 | later |
| Lockfile-aware pinning (`github/gh-actions-lock`) | kj-i#295 | investigate |

### Config, suppression, CLI and output

| Item | Sources | P |
|---|---|---|
| Inline suppressions: `# workflint:ignore RULE -- reason` and `ignore-next-line`, with `actionlint:` accepted as an alias; a block-header directive covers the body; malformed directives are diagnosed | rhysd#375, kj#233 | v1 |
| Rule registry: fine-grained IDs (split `expression`), groups, presets, levels, per-file overrides, **ID aliases** for renames and actionlint compatibility | kj#231/#232, kj-i#260, zz#2233 | v1 |
| One layered settings resolver (defaults → config → env → CLI); `--select`/`--ignore`; selected overrides disabled | zz#2081, kj#155 | v1 |
| Config files: read actionlint YAML unchanged; empty or comment-only config is valid; empty env var means unset; XDG user-global fallback; JSON Schema published | zz#2108/#1010, rhysd#706, kj#120 | v1 |
| `shellcheck` rc/inline config and dialect precedence | rhysd#755, kj#182 | v1 |
| Selector-based `ignores:` (rule + file/uses/job/step + reason) instead of message regexes | kj#250, kj-i#172 | v1 |
| TOML config / `extends` | zz#1666, kj#232 | later |
| Subcommands `check`/`rules`/`doctor`/`completion`; JSON (versioned schema)/JSONL/SARIF/GitHub output; Ruff-style `-q`/`--silent`; completions; man page | rhysd#716, kj#45/#155/#201, zz#1696 | v1 |
| "Did you mean" for contexts, properties, functions, `always`, `.gihub` | kj#224/#226, kj-i#191/#262 | v1 |
| Semantics-preserving autofix only (safe/unsafe split) | zz#1927 | v1 (policy) |
| JSON document-model export | kj#222, kj-i#189 | later |

### External tools

| Item | Sources | P |
|---|---|---|
| Ruff for Python scripts (isolated, JSON, mapped positions); no Pyflakes | kj#234/#230, astral#22/#25 | v1 |
| ShellCheck subprocess pool with a correct stdin feed and cancellation. kjanat's fix equals upstream #651, yet 00-baseline still hangs on upstream main, so **investigate further** | rhysd#651, kj v1.8.0, 00-baseline.md | v1 |
| Recommend or ship a hermetic ShellCheck (rshellcheck is GPL: subprocess only; or a WASM build) | rhysd#482, kj#89/#271/#294 | later |
| Generic external script-linter interface (pwsh, github-script JS) | kj-i#23 | later |

### Data freshness, testing and distribution

| Item | Sources | P |
|---|---|---|
| Scheduled generators with PR-per-drift, request deadlines and failure alerts; a GitHub-changelog monitor that opens an issue per Actions entry | rhysd#718, kj#103/#108, zz#2438 | v1 |
| Conformance corpus: languageservices, runner, SchemaStore, yaml-test-suite; pinned SHA-256; exact difference baseline | kj#147 | v1 |
| Expression behaviour vectors from kjanat's hosted probe | kj#35/#171 | v1 |
| Release targets include Windows ARM64; attestations, SBOM, `cargo-deny`/`cargo-audit`, Dependabot for cargo | zz#2424/#2420, rhysd#731/#685/#684, kj#80 | v1 |
| Official action (JS/composite launcher, version pinning, SARIF/summary/annotations) | kj#185, see 10-github-action.md | v1 |
| npm + PyPI wheels + Homebrew + aqua/mise + Docker; later AUR/Scoop/WinGet/Nix/Chocolatey; plugin PRs to Trunk/MegaLinter/super-linter | kj#95–#98/#151/#270/#99/#209, rhysd#725/#722 | v1 / later |
| Jujutsu `.jj` root detection; gitignore-aware discovery | rhysd#752, kj#289 | v1 |
| Online mode: per-host `auth:`, rate-limit error, narrow retries, RFC 9111 HTTP cache | zz#2318/#1609/#2364 | later |
| ProofX release proof | rhysd#729 | ✗ |
