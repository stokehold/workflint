# 05 — Upstream actionlint issues & PRs analysis (for workflint)

Snapshot taken 2026-10-15 from `rhysd/actionlint`: **154 open issues** and **55 open PRs** (all read), plus the 9 issues closed as "not planned" and the 100 most-reacted closed issues. Raw dumps are in `.reference/research/raw/`:

| File | Contents |
|---|---|
| `open-issues.json` / `open-issues.txt` / `open-issues-comments.txt` | all open issues: bodies, comments, reactions |
| `open-prs.json` / `open-prs.txt` | all open PRs with changed files |
| `closed-not-planned.json`, `closed-np-<N>.json` | issues closed as not planned, with comments |
| `closed-top-reacted.json`, `closed-<N>.json` | most-reacted closed issues and selected maintainer replies |

Code citations point to the reference checkout at upstream `main` commit `011a6d1`. That is 3 commits after the last release, v1.7.12 (2026-03-30). The release-blocking deadlock fix (#651) is on `main` but has never shipped. **R** is the total reaction count on the issue.

Context: upstream has had no commits since 2026-04-19 and no release since 2026-03-30 (#563, #699, #719). Two forks are active: `kjanat/actionlint` (v1.14, adds composite-action checks, exact shellcheck positions, completions and an official action) and `jdx/jactionlint` (adds security/policy checks to replace zizmor). Many of the most-reacted issues below are not hard problems. They are simply **data that stopped being refreshed**, with fixes sitting in unmerged PRs.

Handling legend: **By design** = the Rust architecture removes the problem class; **Fix** = a specific behaviour to get right; **Feature** = something to add; **Data** = refresh tables/datasets; **OOS** = out of scope (reason given); **No-op** = upstream behaviour is correct, keep it.

---

## 1–2. Every open issue, by category

### (a) False positives (valid workflows rejected)

| # | Title | Problem | R | workflint handling |
|---|---|---|---|---|
| 701 | `workflow_call` secrets type applied to other triggers | When a workflow has both `workflow_call` and e.g. `workflow_dispatch`, `secrets` is closed to the declared `workflow_call` secrets for every job. | 0 | **Fix**. The `secrets` context is closed only when `workflow_call` is the sole trigger; otherwise it is an open map. Root cause: `rule_expression.go:152-158` sets `rule.secretsTy` whenever `e.Secrets != nil`. PR #703 does this. |
| 148 | Mix inherited with declared secrets | `secrets.declared || secrets.ORG_SECRET` is rejected; a reusable workflow can be called either with `secrets: inherit` or with explicit secrets. | 1 | **Fix** (same root cause as #701). Undeclared secrets in a `workflow_call` workflow get a warning-level "may be inherited" diagnostic, not an error. Also make it configurable via `known-secrets` (#609). |
| 407 | needs outputs / matrix from `fromJSON` / secrets in reusable workflows | A grab-bag: needs outputs (user error), dynamic matrix properties, and the #701 secrets bug. | 0 | **Fix** the #701 part. For dynamic matrices (`matrix: ${{ fromJSON(...) }}`) the matrix type is `any`. Add inline suppressions (#237) for the rest. |
| 343 | `jobs.<id>.result` not defined in `workflow_call` outputs | `jobs` context only has `outputs`. | 5 | **Fix**. Type it as `{outputs, result}`. Root cause: `rule_expression.go:1018-1020`. PR #724. |
| 225 | Quoted `'true'` passed to a string input of a reusable workflow is typed bool | The type is deduced from the decoded scalar, ignoring quoting and tags. | 4 | **Fix**. Type `with:` values from the YAML tag and quote style (`!!str`, `"..."` → string). Root cause: `rule_expression.go:551-563` ignores `String.Quoted`, which exists at `ast.go:39`. |
| 250 | Numeric string considered a number in property access | `"3.10"` in a matrix becomes `number`. | 0 | **Fix**. Same tag-aware typing. Root cause: `rule_expression.go:1053-1073` (`checkRawYAMLString` re-parses the decoded value). PR #727. |
| 517 | `if: |` multiline condition reported as always true | A block scalar's trailing newline counts as "extra characters around `${{ }}`". | 0 | **Fix**. Trim trailing whitespace/newlines before checking the placeholder bounds. Root cause: `rule_if_cond.go:41-51`. |
| 367 | `${{ success() }} \|\| ${{ failure() }}` reported as always true | The reporter thinks it is wrong. | 4 | **No-op**. The diagnostic is correct: the runner's template engine formats multiple segments into a non-empty string. Improve the message to suggest `${{ success() \|\| failure() }}`. |
| 443 | `github.head_ref` in `if:` flagged as untrusted | Maintainer could not reproduce. | 0 | **No-op/test**. Only report untrusted inputs in script sinks (`run:`, `github-script` `script:`), never in `if:`. Add a regression test. |
| 499 | Complex `runs-on` labels (`runs-on=${{ github.run_id }}`, `runner=64cpu-linux-x64`) | Third-party runner services (RunsOn, Blacksmith, WarpBuild) use `key=value` labels. | 0 | **Fix/Feature**. Skip labels that contain expressions. Document glob support in config labels (already `path.Match`, `rule_runner_label.go:210`). Ship presets for well-known third-party runner providers (#478 comment). |
| 509 | download-artifact v3 "too old" false positives on GHES | GHES ships `v3-node20` tags; v4 is unsupported on GHES. | 1 | **Feature**. Add a `target: github.com \| ghes-<ver> \| ghec \| forgejo \| gitea` profile that adjusts outdated-action checks, contexts and permissions. Root cause: unconditional `OutdatedPopularActionSpecs` (`popular_actions.go:5068`). PR-less upstream (#510 closed). |
| 518 | Local JS action `main: index.js` doesn't exist because it is built in a prior step | Lint-time file existence check. | 1 | **Fix**. Downgrade to a warning, or skip when an earlier step in the same job builds it. Make the severity configurable. Root cause: `rule_action.go:430`. |
| 522 | Dynamic outputs of a general-purpose local action | The action writes arbitrary outputs to `GITHUB_OUTPUT`, but strict output typing rejects them. | 0 | **Feature**. Per-action config `dynamic-outputs: true` (also needed for `google-github-actions/get-secretmanager-secrets`, closed #404). Verify GitHub semantics for undeclared outputs of node/docker actions. Consider an open type for those. |
| 530 | "Any program on `$PATH` can be a shell" | `shell: nix develop` rejected. | 0 | **No-op** + better message. GitHub requires a builtin name or a `{0}` format string, and `{0}` is already accepted (`rule_shell_name.go:74`). The message should mention the `{0}` form. |
| 511 | `runs.env` in composite actions | User claims it is valid. | 0 | **No-op**. Docs say `runs.env` is Docker-only, and the maintainer agreed (`rule_action.go:455`). Do **not** merge PR #513. |

### (b) False negatives / missing checks

| # | Title | Problem | R | workflint handling |
|---|---|---|---|---|
| 734 | `workflow_run.head_branch` not untrusted | Missing from the untrusted-input list. | 0 | **Data**. Add `workflow_run.head_branch`, `head_repository.*`, `head_commit.*` (PR #737). Make the untrusted list data-driven and user-extendable (#345). Root cause: `expr_insecure.go:127-139`. |
| 748 | Reusable-workflow call jobs accept `services:` | `stepsOnlyKey` is not set for `services`. | 0 | **Fix**. Root cause: `parse.go:1396-1397`. Generate the allowed-keys matrix from the official schema rather than hand-written switches. |
| 656 | Runner-label checking with expressions is fragile | Only an exact `${{ matrix.x }}` is resolved. | 0 | **Fix**. Do a general partial evaluation of `runs-on` strings over matrix values (string interpolation, `format()`). Root cause: `rule_runner_label.go:235` (`tryToGetLabelsInMatrix`). |
| 634 | `exclude:` values are checked against `include:` values too | `include` is processed after `exclude`, so exclude must match `matrix:` only. | 0 | **Fix**. Root cause: `rule_matrix.go:157-172` merges include values into the rows used for the exclude check. |
| 630 | Invalid value types in `matrix.include/exclude` | Scalar-vs-array mismatches (refined in comments). | 0 | **Feature**. Element-type compatibility check between matrix rows and include/exclude entries (warning). |
| 622 | Outdated `actions/cache` pinned by SHA not flagged | The outdated check only works on `@vN` refs. | 0 | **Feature**. Read the trailing `# vX.Y.Z` comment (Dependabot/Renovate convention) to resolve SHA pins. Optional online resolution (theme 5). |
| 586 | Findings should be a superset of CodeQL's Actions queries | Parity request. | 0 | **Feature (research)**. Map CodeQL `actions/ql/src` queries and zizmor audits to an opt-in "security" rule pack (theme 8). |
| 552 | Check permissions granted to local reusable workflows | The caller must grant every scope the callee's jobs request, otherwise the run fails at startup. | 7 | **Feature**. Cross-file permission check for `./` and `$/` calls, with a configurable default token policy (PR #670 `assume-default-permissions`). |
| 538 | Detect concurrency-group deadlocks between caller and callee | Same `concurrency.group` expression in a caller and a local callee. | 0 | **Feature**. Workflow-graph check for identical group expressions (warning). |
| 514 / 221 | Local action used before checkout | Common mistake. | 3 / 1 | **Feature** (declined upstream, closed #192). Opt-in heuristic warning: no `actions/checkout` (or configured checkout actions) before a `./` action in the job. Not needed for `$/`. |
| 441 | Enforce `\|` before multiline `run` | A plain multi-line scalar is folded into one line, so commands silently join. | 1 | **Feature**. Warn when a `run:` plain/folded scalar spans multiple source lines (needs scalar-style info from the parser). |
| 417 | Detect `$ {{ x }}` (space after `$`) | Malformed placeholder. | 0 | **Feature**. Warn on `$ {{`, `${ {`, and `{{ ... }}` without `$`. |
| 169 | Unclosed or unopened expressions (`$ secrets.X }}`, `secrets.X }}`) | GitHub fails on a stray `}}`. | 2 | **Fix**. Report `}}` with no opening `${{` in expression-capable strings. Mirror runner behaviour. |
| 385 / 213 / 96 | Treat `inputs.*` / `workflow_dispatch` inputs / `env.*` as untrusted | Script injection via inputs. | 1 / 1 / 4 | **Feature**. Opt-in "strict injection" mode that flags any `${{ }}` in `run:` except an allow-list (`matrix`, `steps.*.outputs` configurable). Default stays as today. |
| 345 | `${{ env.FOO }}` in run can be injection | Taint through `env`. | 2 | **Feature**. Taint tracking: an env var assigned from an untrusted source and then interpolated via `${{ env.X }}` in a script is flagged. |
| 365 | Misspelled `steps.cache.outputs.primary-key` not caught | `actions/cache/restore@v3` is not in the dataset (only v4/v5), so outputs are untyped. | 0 | **Data/by design**. Cover all majors still in use, plus optional fetch-and-cache of action metadata (theme 5). |
| 362 | "too many workflows referenced (limit 20)" | Nested reusable-workflow limit. | 1 | **Feature**. Count reusable-workflow fan-out through local and resolvable remote calls; error above GitHub's documented limits (verify the current limits, which have changed). |
| 356 | Max expression length 21000 exceeded | Runtime-only failure. | 0 | **Feature**. Check expression/template length limits. |
| 336 | Inputs passed to a local reusable workflow that only `workflow_dispatch` declares | Inputs declared on the wrong trigger. | 1 | **Fix/verify**. Local reusable-workflow input checking exists (`reusable_workflow.go`); make sure it uses only `workflow_call.inputs`, and add a hint when the input exists under `workflow_dispatch`. |
| 383 | Validate `with:` for remote reusable workflows | Needs the remote file. | 0 | **Feature**. Optional remote reusable-workflow resolution with an on-disk cache keyed by `owner/repo/path@sha` (theme 9). |
| 277 | `timeout-minutes` not supported in composite action steps | Part of composite linting. | 0 | **Feature** (part of action metadata linting, theme 1). |
| 265 | Check that local actions exist | Removed upstream because of submodule/cloned-at-runtime cases (`action_metadata.go:254-262`). | 0 | **Feature** (opt-in). Off by default; `check-local-action-exists: true`, or warning severity. |
| 246 | `gh` used without `GITHUB_TOKEN`/`GH_TOKEN` | Needs script parsing. | 3 | **Feature (later)**. Needs a shell tokenizer (see theme 6); low priority. |
| 83 | `workflow_run.workflows` names must exist | Cross-file name check. | 5 | **Feature**. Project-level index of workflow `name:` values; warn on unknown names (only when all workflows are in the lint set). |
| 74 | `env` can't reference a sibling key at the same level | `env.X` used inside the same `env:` mapping is always empty. | 0 | **Feature**. Scope-aware `env` typing: keys defined at the current level are not visible to their siblings. |
| 39 | Validate `github.event.*` properties | `github.event` is an empty open object (`expr_sema.go:275`). | 0 | **Feature**. Generate per-event payload types from `@octokit/webhooks` schemas, scoped by the workflow's `on:` triggers. Warn rather than error on unknown properties at first. |

### (c) Stale data (labels, events, permissions, context properties, keys, action inputs)

| # | Title | Problem | R | workflint handling |
|---|---|---|---|---|
| 693 | Parallel steps (`background`, `wait`, `wait-all`, `cancel`, `parallel`) | New step keys (2026-06-25). | 93 | **Data+Feature**. Model these step kinds in the AST, with reference checks (`wait`/`cancel` must target an earlier `background` step) per PRs #694/#695. Step parsing lives at `parse.go:1202`. |
| 711 / 760 | `$/` self-repository `uses:` | New syntax (2026-07-30) rejected for both actions and reusable workflows. | 82 / 0 | **Fix**. Make `uses:` a typed enum (`Local{./}`, `SelfRepo{$/}`, `Repo`, `Docker`, `Url` for Forgejo); resolve `$/` like `./`. Root causes: `rule_action.go:331`, `action_metadata.go:244`, `reusable_workflow.go:208`, `rule_workflow_call.go:64,151`. PR #732. |
| 682 / 761 | `ubuntu-26.04`, `ubuntu-26.04-arm` | Missing labels. | 41 / 1 | **Data**. Root cause: `rule_runner_label.go:30-110` hand-maintained tables. PRs #683/#723/#743. |
| 739 | `xcode-27`, `xcode-27-xlarge` | Missing labels. | 0 | **Data**. PR #744. |
| 688 | `windows-2025-vs2026` | Already in v1.7.12 (`rule_runner_label.go:36,106`). The user likely ran an older version. | 0 | **Data** (already handled). Also add `windows-11-vs2026-arm` (PR #710). |
| 337 | Larger runners (`ubuntu-latest-4-cores`) | Partly fixed upstream; custom-named GitHub-hosted larger runners exist. | 3 | **Data+Config**. Rename the config concept from `self-hosted-runner.labels` to `runner-labels` (keep the old key as an alias). |
| 555 | ARM labels (and the arm pre-commit hook) | Label part already fixed; the hook part is distribution. | 1 | **Data** + see (h). |
| 657 / 680 / 687 | `concurrency.queue` | New key; validate `single\|max` and reject `max` together with `cancel-in-progress: true`. | 21 / 0 / 0 | **Data+Fix**. Root cause: `parse.go:821-824`. PR #654 (R=50). |
| 757 / 705 / 647 | `job.workflow_ref/sha/repository/file_path` | Missing from the strict `job` context. | 0 / 1 / 5 | **Data**. Root cause: `expr_sema.go:312-326`. PRs #696/#707. Mark GHES availability in the target profile. |
| 713 | `vulnerability-alerts` permission | Missing scope (read/none only). | 2 | **Data**. Root cause: `rule_permissions.go:5-20`. PR #666. |
| 672 / 720 | `code-quality` permission | Missing scope. | 6 / 1 | **Data**. PRs #658/#674. |
| 686 | `copilot-requests` permission | Missing scope (from gh-aw). | 11 | **Data**. PR #689. |
| 726 | `merge_group` `types: [destroyed]` | The generator scrapes the trigger docs page, not the webhook docs. | 0 | **Data**. Generate activity types from the webhook schema plus docs. Root cause: `all_webhooks.go:26`. PR #740. |
| 669 / 648 | `actions/create-github-app-token@v3` (`client-id`) | Dataset froze at v3.0.0 (`popular_actions.go:847`). | 34 / 30 | **Data/by design**. Key the dataset by major *and* refresh it continuously; honour `deprecationMessage` (warn on `app-id`). PRs #652/#668/#718. |
| 537 | download-artifact v4 `artifact-ids` | Already fixed on main (`popular_actions.go:1014-1029`). | 4 | **Data** (handled). |
| 416 | `dorny/paths-filter@v3` `predicate-quantifier` | Fixed on main (`popular_actions.go:3389`). | 0 | **Data** (handled). |
| 255 | download-artifact `download-path` output | Fixed upstream in the action's manifest. | 1 | **Data** (handled). |
| 346 | `actions/labeler@v4` `all-labels` output | Old major, now in the outdated set. | 1 | **Data**. Verify the v4 outputs; keep metadata for old majors too. |

Not in an issue but in PRs: `cache-mode` workflow/job key (PR #745), `github.job_workflow_sha` (PR #671), service container `entrypoint`/`command` (merged on main, unreleased).

### (d) YAML / parsing problems

| # | Title | Problem | R | workflint handling |
|---|---|---|---|---|
| 753 | `.nan` float panics | `err.Error()` on a nil error when ParseFloat succeeds with NaN. | 0 | **By design** (no panics; `Result` everywhere). Root cause: `parse.go:305-306`. |
| 747 | Container `volumes` parsed into `Ports` | Copy-paste bug drops one field. | 0 | **Fix**. Root cause: `parse.go:1065-1066`. Typed AST tests catch this. |
| 519 | Cryptic "did not find expected key" with wrong line | go-yaml error message; the line is also shifted. | 0 | **By design**. Use a YAML parser with precise spans and good recovery, plus friendly re-phrasing of common errors (mis-indented list item, `name: [Foo] bar`). Root cause: `go.yaml.in/yaml/v4 v4.0.0-rc.3` (`go.mod:14`); PRs #673/#730/#735 bump it. |
| 330 | Wrong column for expression errors in multiline `run:` | Block-scalar indentation is lost. | 0 | **By design**. The parser keeps a per-line source map for block scalars (theme 7). |
| 225 | (also d) quoting/tag semantics | see (a). | — | YAML layer must expose tag, quote style and block style per scalar. |

Closed but relevant: anchors/aliases (closed #133) and merge keys `<<` must be supported from day one.

### (e) Expression engine / type issues

| # | Title | Problem | R | workflint handling |
|---|---|---|---|---|
| 440 | Ternary with falsy "true" branch (`cond && '' \|\| 'x'`) | Always returns the `\|\|` branch. | 0 | **Feature**. Flag `a && <falsy literal> \|\| b`. Simple AST pattern on the LogicalOp node. |
| 373 | Mis-terminated `if:` spanning lines | `${{ a }}\n \|\| b }}`. | 0 | **Fix/verify**. Probably already caught by the extra-characters check (`rule_if_cond.go:51`). Add a test. Also report the stray `}}` (#169). |
| 110 | Double-quote check missed when `${{ }}` is mixed into `if:` | `${{ github.repository }} == "a/a"`. | 0 | **Fix/verify**. Now caught as "always true" by `rule_if_cond.go:51`. Add a test. |
| 349 | `${{ insert }}` | Undocumented runner directive used as a mapping key. | 0 | **Feature**. Accept `${{ insert }}` as a mapping key in `env`/`with`/matrix (runner template-engine parity). |
| 434 | Redundant branch checks in `if:` | `on.push.branches: [main]` plus `if: github.ref == 'refs/heads/main'`. | 0 | **Feature** (opt-in, warning). Needs a union/literal type for `github.ref`/`event_name` derived from `on:`. |
| 69 / 68 | Redundant or always-false `github.event_name` comparisons | Same machinery. | 0 / 0 | **Feature**. Literal-union types for `github.event_name` from `on:` triggers; constant-fold comparisons. |
| 172 | Property case sensitivity | Outputs with camelCase; maintainer could not reproduce. | 0 | **By design**. Case-insensitive property lookup everywhere, with tests for camelCase outputs. |
| 39 | (also e) `github.event` typing | see (b). | — | — |

PRs fixing engine bugs: #733 (integer literals > int32, `expr_parser.go:142` uses `ParseInt(...,32)`), #708 (`1e+5` rejected, `expr_lexer.go:322` only accepts `-`). Both are **Fix** items: number literals follow JSON grammar and use f64 semantics like the runner.

### (f) Feature requests (config, output, rule control, IDE, policy)

| # | Title | Problem | R | workflint handling |
|---|---|---|---|---|
| 46 / 401 / 350 / 323 | Lint action metadata (`action.yml`), composite actions | Not supported; running on `action.yml` gives nonsense "missing on/jobs". | 168 / 98 / 4 / 0 | **Feature (core)**. First-class `ActionMetadata` document kind, auto-detected by file name or the top-level `runs:` key. Composite steps reuse the workflow step checker (expressions, shellcheck, `uses`, `shell` required), with action-specific rules (`runs.using`, branding, inputs are always strings, no `timeout-minutes` in composite steps #277). PR #366 (R=58) is the reference design. |
| 237 | Inline ignore comments | Only regex `-ignore` / per-path ignore exist. | 72 | **Feature (core)**. `# workflint-disable-next-line <rule>[: reason]`, `# workflint-disable <rule>` … `enable`, file-level `# workflint-disable-file`. Report unused suppressions. Optionally honour `# actionlint-ignore` for migration. PR #375 (R=25). |
| 516 | Warnings vs errors | No severity at all (`error.go:26-37`). | 2 | **Feature (core)**. Each rule has a default severity (`error\|warning\|info`), overridable per rule and per path in config; `--fail-on`/`--max-warnings`. |
| 75 / 71 | Unique error IDs and doc links | `Kind` is too coarse (`syntax-check` covers many checks). | 3 / 3 | **Feature (core)**. Stable hierarchical rule codes (e.g. `WL1001 syntax/unexpected-key`), a `help_uri` per code, used in SARIF/JSON and in suppressions. |
| 49 | Require `timeout-minutes` | Maintainer: opinionated. | 21 | **Feature (opt-in rule)**. PR #581. |
| 358 | Autofix | No fixer infrastructure. | 12 | **Feature**. Diagnostics carry optional `Fix { edits: Vec<TextEdit>, applicability }`; `--fix`/`--fix-unsafe`; shellcheck `-f diff` integration later. |
| 229 / 73 | LSP server / VS Code extension | None upstream. | 2 / 2 | **Feature**. `workflint lsp` (tower-lsp). Diagnostics plus code actions from fixes and hover for contexts. VS Code extension as a thin client. |
| 198 | Security best-practice options (min permissions, SHA pinning) | | 9 | **Feature**. Opt-in `security` rule group (theme 8). |
| 435 / 524 | Detect `@main`/`@master`; require exact versions | Pinning policy. | 3 / 3 | **Feature**. `pinning: {require: sha \| semver \| any, allow: [actions/*, myorg/*]}`. PR #436 (R=15). |
| 738 | Extend SHA pinning to composite actions' nested `uses:` | Transitive pinning. | 0 | **Feature**. Apply the pinning rule to local composite actions (and fetched remote ones). |
| 728 / 525 | Warn on missing `permissions:`; prefer job-level least privilege | | 0 / 1 | **Feature** (opt-in security rules). |
| 469 | Require that a specific action is used | Org baseline policy. | 0 | **Feature (policy)**. `required-actions` config (PR #474). Low priority. |
| 450 | Naming convention for inputs/outputs | Style. | 6 | **Feature (opt-in style rule)**, kebab-case by default when enabled. |
| 501 | Always wrap `if:` in `${{ }}` | Style. | 0 | **Feature (opt-in style)**. |
| 374 | Warn when `shell:` isn't explicit | The default differs (`sh` vs `bash -e`). | 1 | **Feature (opt-in)**. |
| 564 | Suggest pinning `*-latest` runners / flag outdated OS | Opinionated, contested in the thread. | 3 | **Feature (opt-in, info)**. Flag deprecated/retired images (data-driven) at warning level by default; `*-latest` advice off by default. |
| 609 | `secrets` allow-list like `config-variables` | | 0 | **Feature**. `known-secrets: [...]`; PR #649. |
| 655 / 478 | Allow only configured self-hosted labels / disallow `self-hosted` and GitHub-hosted labels | ARC setups. | 1 / 6 | **Feature**. `runner-labels: {allow-github-hosted: bool, allow-self-hosted-preset: bool, labels: [...]}` (the `self-hosted` preset is hard-coded at `rule_runner_label.go:73`). |
| 676 | Merge multiple config files | Org base + repo overrides. | 0 | **Feature**. `extends: [path \| url]` with deterministic merge. |
| 675 | Global config from `$XDG_CONFIG_HOME` | | 0 | **Feature**. Discovery order: `--config` > repo `.github/workflint.yaml` > `workflint.toml` > `pyproject.toml [tool.workflint]` > XDG user config. PR #706. |
| 623 | `[tool.actionlint]` in `pyproject.toml` | | 0 | **Feature**. Support `[tool.workflint]` (ruff-style precedence: a dedicated file wins). |
| 667 | Ignore generated `*.lock.yml` (gh-aw) | Needs a file exclude; today it's `ignore: ['.*']`. | 1 | **Feature**. `exclude: [globs]` plus `.gitignore`-style handling, and built-in awareness of gh-aw `.lock.yml` (lint the source `.md`? out of scope). |
| 481 | Ignore via `paths` "not working" | Not reproducible; the user's glob didn't match `.yml` vs `.yaml`. | 4 | **Feature (UX)**. `workflint config explain <file>` shows the config sources and matched path rules; warn when a path glob matches no file. |
| 554 | Org secrets "invalid" with an invented config schema | Unknown config keys (`secrets:`, `rules:`) are silently ignored. | 0 | **By design**. Strict config schema with errors on unknown keys and a published JSON Schema. Root cause: non-strict `yaml.Unmarshal` at `config.go:93`. |
| 691 | `--summary` aggregate line | | 0 | **Feature**. Summary footer by default in TTY output, `--summary` elsewhere. |
| 352 | Custom rules | Upstream supports this only via the Go library. | 0 | **Feature (later)**. Phase 1: declarative policy rules in config (required/forbidden actions, keys, labels). Phase 2: WASM or Rhai plugins. Library API in the meantime. |
| 256 | Expose parsed `uses:` type in the API | Library consumers (scorecard-like tools). | 0 | **By design**. The typed `Uses` enum is part of a public crate API. |
| 300 | Shell completion | | 0 | **By design**. `clap_complete` for bash/zsh/fish/pwsh, plus a man page via `clap_mangen`. PR #716. |
| 698 | Forgejo `uses: https://...` URLs | Alternative platforms. | 4 | **Feature**. `Uses::Url` variant, enabled by `target: forgejo\|gitea` or auto-detected from a `.forgejo/workflows` path. |

### (g) Performance / concurrency

| # | Title | Problem | R | workflint handling |
|---|---|---|---|---|
| 184 | `panic: WaitGroup is reused` | Race in the process pool (reported fixed in 1.6.27). | 4 | **By design**. Structured concurrency (rayon/scoped threads plus a bounded subprocess semaphore); no shared WaitGroup reuse. |
| 702 / 704 / 712 | shellcheck integration deadlocks (see (i)) | Hangs for hours in CI; ignores SIGTERM. | 0 / 0 / 0 | **By design**. See (i). Also a global `--timeout` per external process and graceful SIGTERM handling. |

No user reports slow linting itself. The perf issues are all hangs. Benchmarks should still cover large monorepos (hundreds of workflows) and huge `run:` blocks (>64 KiB).

### (h) Distribution / install

| # | Title | Problem | R | workflint handling |
|---|---|---|---|---|
| 117 | Official GitHub Action | Users rely on the curl script or third-party actions (reviewdog). Declined as #262. | 28 | **Feature**. Ship `workflint/action` (separate repo, own tags) that downloads a pinned, checksum-verified release binary and emits annotations natively (`--format github`). |
| 477 | pre-commit hook doesn't install shellcheck | Different results locally vs CI. | 10 | **By design**. pre-commit hooks: `workflint` (system shellcheck) and `workflint-bundled` (embedded shellcheck, see (i)). PR #482 uses wasilibs go-shellcheck. |
| 555 | Hook/action incompatible with ARM runners | Docker image was amd64-only. | 1 | **By design**. Multi-arch binaries (x86_64/aarch64 for linux-gnu/musl, macOS, Windows incl. arm64), multi-arch OCI images, PyPI/npm wheels (`pip install workflint`, à la ruff). |
| 153 | Docker shellcheck crash on arm64 host | Platform mismatch. | 0 | **By design** (multi-arch image). |
| 493 | Checksum of the download script | Supply-chain pinning. | 0 | **Feature**. Signed checksums, SLSA provenance attestations, and a versioned installer pinned per release. |
| 520 | Problem-matcher JSON must be copied into repos | | 0 | **By design**. `--format github` emits workflow commands directly; no matcher needed. The official action handles it too. |

### (i) shellcheck / pyflakes / external-linter integration

| # | Title | Problem | R | workflint handling |
|---|---|---|---|---|
| 702 | Deadlock: stdin written to `StdinPipe` before `Start` | Blocks once the script exceeds the pipe buffer (8 KiB under pipe pressure). | 0 | **By design**. Spawn first, write stdin from a dedicated thread while reading stdout/stderr concurrently (or pass the script as a temp file). Upstream fix is on main but unreleased (`process.go:27-31`, commit fd33e9f). |
| 712 | Deadlock for `run:` > 64 KiB | Same root cause. | 0 | **By design** (as above). Regression test with a 200 KiB script. |
| 704 | Intermittent deadlock with both pipe ends held by actionlint | Race even for small scripts. | 0 | **By design**. Never keep the child's write end open in the parent (close-on-exec, drop handles after spawn); per-process timeout. |
| 189 / 347 | shellcheck/pyflakes missing → silently skipped | Only visible with `-verbose` (`linter.go:574-592`). | 9 / 0 | **Fix**. Warn (stderr, once) when an integration is unavailable. Hard error when it was requested explicitly via flag/config. `--require-shellcheck` for CI. |
| 545 | Missing `fi` not reported | Probably shellcheck unavailable or parse errors dropped. | 0 | **Fix/test**. Always surface shellcheck SC1xxx parse errors; covered by #189 warnings. |
| 486 | `.shellcheckrc` not used | `--norc` is hard-coded (`rule_shellcheck.go:193`), deliberately. | 3 | **Feature**. `shellcheck.rcfile: path` in config / `--shellcheck-rcfile` (PR #755), and `shellcheck.args`. Keep `--norc` by default for reproducibility. |
| 360 / 88 | Point shellcheck errors at the exact script line/column | Errors point at the `run:` key (`rule_shellcheck.go:220-232`). | 13 / 12 | **By design** (theme 7). Block-scalar source maps; literal `\|` maps exactly, folded `>` best effort. PR #556. |
| 364 | Playground doesn't run shellcheck | WASM build can't spawn processes. | 3 | **Feature**. Bundle shellcheck compiled to WASM (wasilibs) for both the playground and the `--bundled-shellcheck` mode. |
| 439 | Use ruff instead of pyflakes | | 5 | **Feature**. Pluggable Python linter: `python-linter: pyflakes \| ruff` (`ruff check --stdin-filename -`). |
| 488 / 389 / 244 | Lint `actions/github-script` JS/TS (jslint/tsc/eslint) | No JS checking (also declined as #55). | 0 / 2 / 0 | **Feature (later)**. Syntax-only check of `script:` via an embedded JS parser (oxc/swc) with `github`, `context`, `core`, `glob`, `io`, `exec` declared as globals; skip scripts containing `${{ }}`, or substitute placeholders. |
| 461 | Promote the integration upstream to shellcheck | Not ours. | 0 | **OOS**. |

### (j) Other (governance, upstream infra, project discovery, spam, questions)

| # | Title | Problem | R | workflint handling |
|---|---|---|---|---|
| 719 / 563 / 699 | Maintainer inactive / any forks / new release? | Release cadence is the #1 meta-pain. | 23 / 17 / 7 | **Process**. Automated data refresh PRs plus frequent releases; more than one maintainer with release rights. |
| 750 | `Project.Knows` accepts sibling dirs with a shared prefix | `strings.HasPrefix` path check. | 0 | **By design**. `Path::starts_with` (component-wise). Root causes: `project.go:66`, `reusable_workflow.go:244`. |
| 749 | Non-scalar `ignore` entries compile to an empty regex matching everything | | 0 | **By design** (strict typed config). Root cause: `config.go:35-36`. |
| 242 | Workflows not detected without `.git` | Project root requires `.git` (`project.go:28`). | 0 | **Fix**. A root is the nearest dir with `.github/workflows` plus any VCS marker (`.git`, `.jj` PR #752, `.hg`), falling back to the `.github` parent. Explicit `--root`. |
| 310 | Can a list of files be passed? | Question; docs unclear about directories. | 0 | **Feature (docs/CLI)**. Accept files, dirs and globs (expanded internally, for Windows/Docker). |
| 754 / 758 / 759 / 515 | Upstream generator/test/vuln/toolchain issues | Upstream infra. | 0 | **OOS** (upstream). Lesson: data generators must handle network quirks and run in CI with alerting, not silently fail for months (see PR #718). |
| 369 | Validate apt package names | Needs the runner image package index. | 0 | **OOS** (environment-dependent, network-heavy). |
| 677 / 626 | Vendor pitches (canary tokens, CI cost reports) | Spam. | 0 | **OOS**. |

**Count check (script-verified):** (a) 15, (b) 29, (c) 26, (d) 5, (e) 9, (f) 37, (g) 4, (h) 6, (i) 15, (j) 14 — 160 entries covering all 154 open issues. Six issues are cross-listed: #225 (a/d), #39 (b/e), #555 (c/h), and #702/#704/#712 (g/i).

---

## 3. Open PRs and what they signal

| PR | R | What it does | Signal for workflint |
|---|---|---|---|
| #755 | 0 | `-shellcheck-rcfile` passes `--rcfile` instead of `--norc` | Include as config `shellcheck.rcfile` (#486). |
| #752 | 0 | Detect Jujutsu `.jj` as a project root | Include (VCS-agnostic root detection, #242). |
| #745 | 2 | Support `cache-mode` at workflow/job level | Include (new schema key). Generate keys from the schema. |
| #744 | 1 | `xcode-27` labels | Data. |
| #743 | 10 | `ubuntu-26.04(-arm)`, x/net bump, CI fixes | Data. CI fragility is a lesson. |
| #740 | 0 | `merge_group` `destroyed` | Data. |
| #737 | 0 | `workflow_run.head_branch` / `head_repository.owner.login` untrusted | Include; extend the list. |
| #735 / #730 / #673 | 1 / 0 / 0 | Bump go-yaml v4 rc.4–rc.6 and fix error-position APIs | Shows upstream's YAML dependency is a pre-release with an unstable API. Choose the Rust YAML stack deliberately (spans, recovery, stable API). |
| #733 | 0 | Integer literals > int32 | Include (JSON-number semantics). |
| #732 | 17 | `$/` self-repo `uses:` via a `canonLocalUsesSpec` normaliser | Include. A typed `Uses` enum replaces the scattered `HasPrefix("./")` checks. |
| #731 / #685 / #684 / #279 / #700 | 0 | Dependency/tool bumps, dependabot for Go modules | Infra only. Enable `cargo-deny`/`cargo-audit` + Dependabot for cargo from day one. |
| #729 | 0 | "ProofX" release proof workflow | OOS (vendor). Use standard SLSA attestations. |
| #727 | 0 | Matrix scalar tagged `!!str` is a string (#250) | Include (tag-aware typing). |
| #725 / #722 | 0 | Docs: Debian APT archive; MegaLinter mention | Distribution signal: package managers and meta-linters (super-linter, MegaLinter, trunk) are major channels. Keep CLI/output stable for them. |
| #724 | 0 | `jobs.<id>.result` in `workflow_call` outputs | Include. |
| #723 / #683 | 1 / 30 | `ubuntu-26.04` labels | Data. |
| #718 | 0 | Regenerate popular actions; explains that the weekly generator has failed every run since March | Strong signal. Popular-action data must refresh automatically, with alerting, and must not block on new majors. |
| #716 | 0 | Rewrite the CLI with cobra for completions | Use clap + clap_complete. |
| #710 | 0 | `windows-11-vs2026-arm` | Data. |
| #709 | 0 | Guard odd-length mapping content in the action metadata parser (panic) | By design (no index panics). |
| #708 | 0 | Accept `+` in the exponent | Include. |
| #707 / #696 | 1 / 0 | `job.workflow_*` properties | Data. |
| #706 | 0 | XDG user-global config | Include in config discovery. |
| #703 | 1 | Open `secrets` type in mixed-trigger workflows (#701) | Include. |
| #695 / #694 | 7 / 34 | Parallel steps parsing plus `parallel-steps` reference rule | Include both (model and rule). |
| #689 / #674 / #658 / #666 | 1 / 3 / 6 / 7 | `copilot-requests`, `code-quality`, `vulnerability-alerts` permissions (with per-scope allowed levels) | Data. Note #666 models per-scope allowed access levels (`read`/`none` only), which our permissions table should do too. |
| #671 | 0 (draft) | `github.job_workflow_sha` (only in OIDC docs, used in 54+ public workflows) | Include. Undocumented-but-real properties need a policy: accept, with a doc note. |
| #670 | 2 | Caller/callee permission mismatch for local reusable workflows + `assume-default-permissions` config | Include (#552). |
| #668 / #652 | 14 / 0 | Fix `create-github-app-token@v3` metadata; #652 also keeps a `@v3.0.0` exact entry | Data. Dataset keyed by major and by exact versions. |
| #654 | 50 | `concurrency.queue` with validation | Include. |
| #649 | 0 | `config-secrets` allow-list | Include (`known-secrets`). |
| #581 | 1 | Opt-in configurable `timeout-minutes` rule | Include as an opt-in rule. |
| #556 | 1 (draft) | Map shellcheck errors to the source line for literal block scalars | Include (by design via source maps). |
| #513 | 0 | Allow `runs.env` in composite actions | **Do not include** (contradicts the docs; see #511). |
| #482 | 1 | pre-commit hook installs shellcheck via wasilibs go-shellcheck | Signal for bundled shellcheck (WASM). |
| #474 | 0 | `required-actions` rule | Optional policy feature. |
| #436 | 15 | Opt-in `require-commit-hash` | Include in the pinning policy rule. |
| #375 | 25 | Inline ignore comments | Include (core). |
| #366 | 58 | Lint action metadata files (parser, rules, playground) | Include (core). Reference design for the action document model. |
| #357 | 0 | Allow `if:` under event triggers | **Verify first**. `on.<event>.if` is not in GitHub's schema; probably invalid. |
| #332 | 0 | Treat `env.*` interpolation in scripts as untrusted | Include only in opt-in strict-injection mode (#345). |

---

## 4. Top themes, ranked by user impact

Impact is the sum of reactions across the issue cluster plus linked PR reactions and duplicates.

1. **Action metadata / composite action linting** (#46 168, #401 98, PR #366 58, #350, #323, #277, #738, #518, #511; about 330 reactions). The single most-wanted feature for 5 years.
   → **Decision:** two document kinds (`Workflow`, `ActionMetadata`) sharing one `Step`/`Expr`/`Uses` model and one rule engine. Auto-detect `action.y(a)ml` anywhere in the repo, with configurable discovery globs. Composite-specific semantics: inputs always string, `shell` required, no `timeout-minutes`/`continue-on-error` expressions where unsupported.

2. **Stale platform data and release cadence** (#693 93, #711 82, #682 41, #669 34, #648 30, #719 23, #657 21, #563 17, #686 11, #699 7, plus permission/label/context issues, and PRs #654 50, #694 34, #683 30 sitting unmerged; about 500+ reactions). Users are blocked by valid new syntax for months.
   → **Decision:** every platform fact lives in **generated data files** (runner labels + OS compat, permission scopes + allowed levels, webhook activity types, context property types, workflow/action schema keys). Generate them from authoritative machine-readable sources where possible (GitHub's `actions/languageservices` workflow schema, `@octokit/webhooks` schemas, runner-images README). A scheduled CI job opens PRs and **alerts on failure**. Releases are automated. Users can extend the data in config (`extra-permissions`, `runner-labels`, `extra-keys`) so a stale binary never hard-blocks them. Unknown keys could be downgraded via severity config.

3. **Inline suppression comments** (#237 72, PR #375 25, #407 "noqa" request, #594 `if: false`, workaround comments in #711/#657/#648). Today the only escape hatch is regex-on-message.
   → **Decision:** a comment-aware parser (comments attached to nodes/lines) with `# workflint-disable-next-line <code>`, block `disable`/`enable`, file-level disable, required reasons optional, and "unused suppression" warnings. Suppressions key on stable rule codes, not message text.

4. **Rule identity, severity and opt-in rules** (#49 21, #516, #75, #71, #594, #564, #450, #374, #501, and closed #390/#160). The maintainer declined many checks only because rules couldn't be disabled ("actionlint does not have functionality to disable each rule").
   → **Decision:** a rule registry with stable codes, default severity, default enabled state, categories (`correctness`, `security`, `style`, `pedantic`), and doc URLs. Config has `rules: { <code|category>: off|info|warning|error }` per path. Exit code based on `--fail-on`. This unlocks every opinionated request without hurting the "no false positives by default" promise.

5. **Popular-action dataset staleness and action I/O typing** (#669, #648, #537, #416, #255, #346, #365, #622, #509, #522, closed #90/#404). A frozen bundled dataset produces false positives (#669: 34 reactions) and false negatives (#365).
   → **Decision:** a three-tier action metadata resolver: (1) bundled snapshot covering *all* majors still in use, refreshed by automation; (2) local `action.yml` for `./` and `$/`; (3) **opt-in online fetch** (`--online` / `network: true`) via `raw.githubusercontent.com`/API with token support and an on-disk cache keyed by resolved SHA. Resolve SHA pins via `# vX.Y.Z` comments. Honour `deprecationMessage`. Per-action overrides in config (`dynamic-outputs`, extra inputs).

6. **External linter robustness (shellcheck/pyflakes)** (#702/#704/#712: hours-long CI hangs; #189 9, #477 10, #347, #545, #364, #486, #439). Silent skipping makes results differ between machines.
   → **Decision:** a robust subprocess layer (spawn, then concurrent stdin writer and stdout/stderr readers, per-call timeout, bounded parallelism, no inherited fds). Loud warning when a tool is missing. Optional **embedded shellcheck (WASM via wasmtime)** for hermetic results (pre-commit, playground). Configurable args/rcfile. Pluggable Python linter (ruff).

7. **Precise diagnostics positions in block scalars** (#360 13, #88 12, #330, PR #556). Errors inside `run:` scripts point at the `run:` key.
   → **Decision:** the YAML layer must keep exact source spans for every scalar, its style (`|`, `>`, chomping, indentation indicator) and a line map from decoded content back to source. This is the main reason to own or carefully pick the YAML parser. All diagnostics carry start/end spans (for SARIF regions, LSP ranges and underlines).

8. **Opt-in security and policy rules** (#198 9, PR #436 15, #435, #524, #738, #728, #525, #96, #213, #385, #345, #734, #586). Users want actionlint to cover zizmor/CodeQL-style hardening; the jdx fork exists for this.
   → **Decision:** a `security` rule category, off or warning by default, covering: pinning policy (SHA/semver, allow-lists, transitive through composites), missing/over-broad `permissions`, a data-driven untrusted-input list with taint through `env`, and a strict injection mode. The policy config is a typed schema.

9. **Cross-file / reusable-workflow analysis** (#552 7, #701, #148, #343, #336, #383, #407, #362, #538, #83, PR #670). Bugs and gaps at workflow boundaries.
   → **Decision:** a project-level analysis phase that builds a **workflow graph** (callers → callees, `workflow_run` names, `$/`/`./` resolution) after per-file parsing. It runs permission propagation, input/secret/output typing, nesting limits and concurrency deadlock checks. Remote reusable workflows resolve through the same opt-in online resolver and cache as theme 5.

10. **Autofix** (#358 12; maintainer: "actionlint does not provide fixers").
    → **Decision:** diagnostics may carry `Fix` edits with applicability (safe/unsafe). The CST/span model must support lossless edits that preserve comments and formatting. First fixes: quoting `$GITHUB_OUTPUT`, `if:` cleanups, deprecated `set-output` → `GITHUB_OUTPUT`, adding `|` to multi-line `run:`.

11. **Distribution and an official GitHub Action** (#117 28, closed #262 8, #477 10, #555, #153, #493, #520).
    → **Decision:** an official action repo, `--format github` native annotations, multi-arch binaries and images, PyPI/npm/cargo/Homebrew/winget packaging, signed checksums + SLSA attestations, pre-commit hooks with and without bundled shellcheck.

12. **Config ergonomics and strictness** (#676, #675, #623, #667, #481, #554, #749, #609, #478, #655).
    → **Decision:** a strictly-validated config (unknown keys are errors, JSON Schema published), `extends` layering, XDG/user config, `pyproject.toml` `[tool.workflint]`, file `exclude` globs, `workflint config explain`, and a compatibility reader for `.github/actionlint.yaml`.

13. **Expression engine fidelity** (#733, #708, #250, #225, #517, #440, #349, #373, #169, #110, #367, #39).
    → **Decision:** port the expression lexer/parser/type checker with the runner's own semantics: JSON numbers as f64, `${{ insert }}`, multi-segment template formatting, case-insensitive properties. Typing of YAML values must be tag- and quote-aware. Add literal-union types (`event_name`, `ref`) for constant folding (#68/#69/#434).

14. **Editor integration (LSP)** (#229, #73; the efm-langserver workaround). Cheap once the core is a library.
    → **Decision:** a `workflint lsp` subcommand on the same incremental project model (re-lint on change, cross-file invalidation). Code actions come from fixes, plus hover/completion for contexts later.

15. **Alternative platforms** (#698 Forgejo, #509 GHES, the GHES note in #647).
    → **Decision:** a `target` profile (`github.com`, `ghes-3.x`, `ghec`, `forgejo`, `gitea`) that selects data tables (contexts, permissions, runner labels, allowed `uses:` forms, outdated-action rules). Auto-detect from the `.forgejo/` / `.gitea/` workflow directory.

Cross-cutting robustness (by design in Rust): no panics on malformed input (#753, #709), component-wise path handling (#750), typed AST without copy-paste field bugs (#747, #748), structured concurrency (#184).

---

## 5. Declined / not-planned features worth reconsidering

Upstream's stated philosophy was "minimal config, no opinionated checks, no network, no source modification". Theme 4 (per-rule enable/severity) and opt-in online mode remove the reasons for most refusals.

| Upstream item | Upstream reason | Reconsider as |
|---|---|---|
| #262 (not planned, R=8) / #117 (R=28) Official GitHub Action | Tag conflicts with the CLI repo; download script exists | Official action in a separate repo (theme 11). |
| #594 (not planned) Constant `if: false` is annoying | "Just remove the job"; but required status checks need the job to exist | Keep the rule, allow severity downgrade / inline suppression with a reason. |
| #390 (not planned) Require job `name:` | No demand | Opt-in `style` rule. |
| #430 (not planned) `on.*.paths` globs must match a file | Not pursued (action-validator does it) | Opt-in rule (needs a file index; respect `.gitignore`; warning). |
| #265 / closed #25, #40 Local action existence | Submodules/runtime checkouts | Opt-in rule or warning severity. |
| #192 / #221 / #514 Local action before checkout | False positives with custom checkouts | Opt-in heuristic warning with configurable checkout actions. |
| #49 `timeout-minutes` required (R=21) | Opinionated; no per-rule disable | Opt-in rule (PR #581 design). |
| #160 `workflow_dispatch` input count limit | "Not opinionated / threshold unknown" | It is a hard GitHub limit, so make it a correctness rule (verify the current limit; GitHub has raised it). |
| #395 / #254 / #486 `.shellcheckrc` | Reproducibility (`--norc`) | Explicit `shellcheck.rcfile` option; default unchanged. |
| #82 shfmt / #358 autofix | "actionlint doesn't modify sources" | Fix infrastructure (theme 10); shfmt is optional later. |
| #55 eslint / #389 tsc / #244 TS for `github-script` | eslint not globally installed | Embedded JS syntax check (no external tool). |
| #90 / #404 Fetch action metadata via network | Speed; no network by design | Opt-in online mode with persistent cache (theme 5). |
| #147 Strict boolean-coercion type checks | "Not opinionated" | Opt-in `pedantic` rule (comparisons across types). |
| #450 Input/output naming convention | Out of scope (style) | Opt-in `style` rule. |
| #564 Pin `*-latest` runners | Contested | Opt-in info-level rule; deprecated/retired images flagged by default. |
| #622 Outdated action versions | "Dependabot is better" | Keep runtime-deprecation checks (node12/16/20 sunset) via SHA→version resolution; no "newer version exists" nagging by default. |
| #352 Custom rules (only via the Go library) | — | Declarative policy rules in config, plugins later. |

Not worth reconsidering: #596 (action `inputs.<id>.type` is not in the metadata schema; upstream is right), #397 (`>-` works), #511/PR #513 (`runs.env` is Docker-only), #367 (the diagnostic is correct).
