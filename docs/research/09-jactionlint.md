# 09 — jactionlint (jdx's fork): what it adds and what workflint should take

Snapshot: `.reference/jactionlint` at `1bcb2641` (2026-10-14, v2.2.0). Upstream baseline: `.reference/actionlint` at `011a6d1`.
All `file:line` references are relative to the jactionlint checkout unless prefixed with `upstream:`. Raw GitHub dumps:
`research/raw/jactionlint-issues.json` (28 issues) and `research/raw/jactionlint-prs.json` (254 PRs).
This note diffs against upstream as described in `03-rules.md` / `05-issues.md` and does not re-describe upstream.

---

## 0. TL;DR

- **Fork point is exactly upstream `011a6d1`** (`git merge-base HEAD 011a6d1` = `011a6d1`). 291 commits on top, 263 by jdx,
  **all between 2026-10-04 and 2026-10-14** (87 on 2026-10-13 alone). Diff vs upstream: 2,122 files, +210k/−12k lines.
  Root package: 591 `.go` files (310 tests), ~90.6k non-test + ~69.7k test lines (upstream: 30.7k total). `docs/checks.md` alone is 10.4k lines.
- **It is AI-built at speed**: 237 of 254 PR bodies end with "Generated with Claude Code", 230 carry a Cursor Bugbot summary;
  most issues are written by the maintainer's "rollout agents" (#113–#119). Quality is nonetheless *defended* by many invariant
  and compatibility tests (see §9), but the code is sprawling and prose-heavy.
- **What it is**: actionlint + zizmor merged into one Go binary, with a rule registry (161 rule IDs from 105 implementations),
  three nested profiles (`correctness` ⊂ `default` ⊂ `pedantic`, plus `online` and `opt-in` buckets), per-rule levels/options,
  inline + zizmor-compatible ignores, structural config ignores with expiry, line-free baseline, `--changed-since`, a byte-offset
  autofix engine with tree-diff verification, an opt-in online mode (GitHub REST/GraphQL + registries, cached), action.yml and
  dependabot.yml linting, parallel-steps / `cache-mode` / `queue` syntax, facts export and JSON Schemas for config/output/facts.
- **Correctness profile ≈ upstream superset.** On the 1,503-file corpus it reproduces upstream findings except deliberate
  divergences (larger-runner labels, null webhook filters) and *fixes* upstream's block-scalar line positions.
- **Default profile is loud**: 24,299 findings (all level `error`) vs upstream's 6,836 on the corpus; 92 % of the extra is five
  hardening/policy rules (`unpinned-uses` 7,133, `missing-permissions` 2,828, `missing-timeout` 2,586, `artipacked` 1,934,
  `concurrency-limits` 1,277). Sampled security findings were mostly true positives.
- **Performance**: ~2.4× upstream CPU without external tools (1.0 s vs 0.44 s wall, 2.5 s vs 0.47 s user), but the default
  `github-script-syntax` rule shells out to `node --check` per script: with node on PATH a run took **16–19 s** on this machine.
- **Recommendation**: copy the *architecture* (registry-as-single-source-of-truth, profiles, level/option config, ignore/baseline
  design, fix-as-data + verification, vendored runner schema conformance test, offline-first online mode with cache), adopt a
  curated subset of rules in v1, and avoid the sprawl (hand-curated dated tables, external node, text-indentation heuristics, 10k-line docs).

---

## 1. Provenance, scale, dependencies

| Item | Value |
|---|---|
| Fork point | upstream `011a6d1` (merge-base); first fork commits 2026-10-05 merge open upstream PRs (#654 `queue`, #666 `vulnerability-alerts`, #668, #703, #707) |
| Commits since fork | 291 (jdx 263; github-actions[bot] 8; 9 upstream PR authors via merged PRs) |
| Active days | 2026-10-04…10-14: 39, 23, 5, 47, 28, 24, 87, 11 commits/day |
| Releases | 1.8.0 (10-05) → 2.0.0 (10-09) → 2.2.0 (10-14) (`CHANGELOG.md:3-235`) |
| Go | module `github.com/jdx/jactionlint/v2`, go 1.26, toolchain 1.27.2 (`go.mod`) |
| New deps vs upstream | `mvdan.cc/sh/v3` (bash parser), `go.yaml.in/yaml/v4` **rc.6** (pre-release), `bmatcuk/doublestar`, `yuin/goldmark` (docs embed), `golang.org/x/net`, `spf13/pflag` (GNU-style flags) |
| Binary | 23 MB (upstream 8 MB) |
| Layout | one giant root package (540 files; `v3.md:9-14` plans to split it), plus `internal/runscript` (shell model) and `internal/exampleconfig` |

`v3.md` is a *breaking-change parking lot*, not a roadmap: split the root package (≈1,570 exported funcs / 324 types,
`v3.md:16-20`), drop legacy config keys and `strict`/`all` profile aliases, remove pre-commit in favour of hk (`v3.md:51-56`),
promote pedantic rules to default "from measured numbers" (`v3.md:58-66`), and react to GitHub turning `pull_request_target`
off by default on 2026-11-02 (`v3.md:68-72`).

---

## 2. Differences vs upstream (summary)

| Area | Upstream (`03-rules.md`) | jactionlint |
|---|---|---|
| Rule identity | 15 rule structs; `Kind` = rule name; only regex-on-message ignores | 161 stable kebab-case IDs (`Error.ID`) on top of upstream `Kind` (`error.go:44-49`); frozen by snapshot test (`testdata/rule_ids.d/*.txt`, `rule_registry_test.go:93`) |
| Rule control | none (only `OnRulesCreated` in the Go API) | profiles + per-rule `off/info/warn/error` + typed options (`rule_registry.go:27-141`, `config_rules.go:17-42`) |
| Security | `expression` untrusted-input check only | ~54 security rules, zizmor-parity (`docs/zizmor-parity.md`) |
| Policy | none | ~22 policy rules (pinning, permissions, timeout, concurrency, …) |
| Files | workflows | + composite/docker/node `action.yml` (`action_file.go`), `dependabot.yml` (`dependabot*.go`), `.github/workflows/actions.lock` (`actions_lock.go`) |
| Suppression | `-ignore` regex, `paths.*.ignore` | inline `# jactionlint ignore=`, `# zizmor: ignore[...]`, config `ignores:` with `reason`/`expires`, baseline, `unused-ignore` |
| Fixes | none | 27 fixable rules, `--fix[=unsafe]`, `--diff`, `--fix-rules` |
| Network | never | `--online` (10 online-only rules + enrichment) |
| Scripts | shellcheck/pyflakes subprocess | + in-process bash parse (mvdan/sh) for syntax, taint, pipefail, GNU flags; `node --check` for github-script; `--ruff` |
| Output | text, JSON via template, SARIF via template | text, oneline, json, jsonl, sarif (native), gcc, github, summary, Go template; `--show-fix-status`, `--finding-context`, schemas |
| Syntax | as of April 2026 | + parallel steps, `cache-mode`, `concurrency.queue`, `cancel-timeout-minutes`, root `description`, `$/` uses, `${{ insert }}`, more whole-section expressions, new permissions/activity types (`parse.go` +615/−57) |
| Positions | block-scalar errors reported at wrong line | mapped to the real line (fix #28 in `CHANGELOG.md:270`; confirmed on corpus, §10) |

---

## 3. Rule registry, profiles, severity, config

**Two registries filled from `init()`**:
- `registerRules(RuleInfo{...})` (`rule_registry.go:163-176`) — metadata per diagnostic ID: `ID, Group (correctness|security|policy|style),
  Summary, DefaultLevel, Profile, Online, OptIn, Fixable, DocsAnchor, Options []RuleOption` (`rule_registry.go:115-141`).
  `RuleOption{Name, Kind (int|number|bool|strings|string-map), Default, Summary, Min, Validate}`.
- `registerRuleFactory(name, func(*RuleEnv) []Rule)` (`rule_registry.go:356-370`) — one constructor per implementation; one
  implementation reports several IDs via `ReportID(id, pos, msg)` (`rule.go:59-75`). E.g. upstream's `action` rule now emits 7 IDs
  incl. `unpinned-uses`, `missing-action-input` (`rule_action.go:829-845`). 161 IDs / 105 factories.
- Retired IDs: `renamedRules` (`rule_registry.go:193-198`) map old → new ID + option (e.g. `template-injection-trusted` →
  `template-injection` with `pedantic: true`); still honoured in ignore comments, refused elsewhere with a hint (`config_rules.go:526-541`).
- `legacyRuleOrder` (`rule_registry.go:334-339`) freezes upstream's rule order for stable output.

**Profiles** (`rule_registry.go:27-74`): nested ranks `correctness (74 IDs) < default (47) < pedantic (27)`; `Profile` is a single
field per ID and inclusion is a rank comparison. Outside the ladder: **online** (10 rules; run only with `--online`, ignore profile)
and **opt-in** (3: `forbidden-uses`, `required-actions`, `timeout-too-long`; on only when configured). There is no "security"
profile — security is a *group*. `strict`/`all` are deprecated aliases for pedantic. Some rules widen with a per-rule `pedantic: bool`
option that defaults to `profile == pedantic` (`audit_pedantic.go:10-26`).

**Level resolution** (`config_rules.go:17-42`): explicit `rules[id]` → special `required-actions` → opt-in=off → online=default level →
profile includes ? default level : off. **Gating happens after the fact**: `annotateErrors` stamps `Severity` and drops `off`
findings (`linter.go:1327-1346`); many factories still construct and run disabled rules (`rule_artipacked.go:514`). `--select`
does short-circuit factories (measured: running one rule costs ~0.4–0.9 s vs 1.0 s for all).

**Severity** (`severity.go`, 79 lines): `off|info|warn|error` (`warning` alias). Only `error` fails (exit 1) unless `--strict-exit`;
`--min-severity` hides. Note: of 161 IDs, 130 default to `error` — on the corpus *every* default-profile finding was `error`.

**Config** (`config.go`, 778 lines): `.github/jactionlint.y{a,}ml` → `.github/actionlint.y{a,}ml` → XDG global; first match wins,
no cascade (`config.go:611-691`). Top-level keys (`config_rules.go:357-363`): `profile, extends, rules, online, online-options, baseline,
self-hosted-runner, config-variables, config-secrets, paths, ignores, required-actions, assume-default-permissions, generated-files,
platform (github|forgejo|gitea), fix.rules, shellcheck`. `extends` up to 10 levels with cycle detection; `rules`/`paths` merge by key,
`ignores` concatenate (`config.go:485-609`). Unknown keys/IDs/options rejected with Damerau-Levenshtein "did you mean" (`config_rules.go:557-605`).

```yaml
profile: default
rules:
  missing-timeout: warn                    # level; true = default level, false = off
  max-run-lines: {level: warn, max: 80}
  timeout-too-long: {max: 60}              # options only => enabled at default level
  unpinned-uses: {policies: {actions/*: ref-pin, my-org/*: any}}
  template-injection: {pedantic: true}
```

**Config schema** (`config_schema.go`, 445 lines) is *generated from the registry* (draft-07; each rule ID → `oneOf [level, {level+options}]`,
`config_schema.go:106,319-372`), committed as `schema/jactionlint-config.json` (4,820 lines), checked by `TestSchemasAreUpToDate`,
printed by `--print-config-schema`. Not used at runtime; parity between Go parser and schema is checked with a hand-rolled mini
JSON-Schema validator in tests (`config_schema_test.go:196-214`). Issue #294 asks to publish it to SchemaStore.

**`--list-rules`** (`select.go:58-116`; text or JSON with `doc_url, online, fixable`) and **`--explain RULE`** (`explain.go`, 154 lines)
are derived from the registry; `--explain` appends the matching section of the `go:embed`-ed `docs/checks.md` (`explain.go:14`) located via
`DocsAnchor`. `docs/rules.md` is generated (`scripts/generate-rules-doc`, `TestRulesDocIsUpToDate`). Shell completion also completes rule IDs.

**Migration** (`config_migrate.go`, 236 lines): `--migrate-config` rewrites legacy boolean keys (`require-commit-hash`, `timeout-minutes`,
`require-permissions`, …; table `config_rules.go:241-329`) into `rules:` by editing the yaml.Node tree and re-encoding with indent 2
(re-formats the file), then re-parses to verify (`config_migrate.go:127-141`). Legacy keys still load with warnings.

---

## 4. Ignores, baseline, `--changed-since`

**Inline** (`inline_ignore.go`, 446 lines): `# jactionlint ignore=<id|regex>[,…]` (`inline_ignore.go:16`), own-line (covers next node and
its subtree) or trailing (`uses: x@sha # v6 # jactionlint ignore=artipacked`); `# actionlint ignore=` also accepted. A pattern is an exact
rule ID or an RE2 regex over the *message* (`config.go:33-45`). Scope from **text indentation** (`ignoreTargetEnd`, `inline_ignore.go:228-251`),
not the AST. No file-level/block markers, no reason field. Comment discovery is a hand-written line scanner aware of block/quoted scalars
(`comments.go`, 257 lines, with a documented known limit at `:32-33`).

**zizmor comments are honoured** (`zizmor_ignore.go`, 542 lines; `zizmor_scope.go`, 216 lines): regex
`# zizmor: ignore\[([^\]]*)\](?:\s(.*))?$`; names map 1:1 to jactionlint IDs plus an alias table (`zizmor_ignore.go:36-41`, e.g.
`excessive-permissions` also covers `missing-permissions`); scope copies zizmor's (step/job/`on:` per audit, `zizmor_scope.go:15-29`).
`--migrate-ignores` rewrites them into jactionlint comments, refusing scope widening.

**Config `ignores:`** (`config_ignores.go`, 836 lines) — structural, durable suppressions:
```yaml
ignores:
  - rule: [unpinned-uses, artipacked]
    uses: actions/checkout          # name pattern | glob | /regex/
    file: .github/workflows/*.yaml
    job: release
    step: checkout
    reason: pinned by an org ruleset
    expires: "2030-12-31"           # after it: `expired-ignore` (info in the last 14 days)
```
Unused entries are judged only if every file they could match was linted (safe for pre-commit subsets).

**`unused-ignore`** (pedantic, fixable, `inline_ignore.go:302-354`) skips patterns for rules that could not have run (off, online while
offline, missing shellcheck). Its fix rewrites/deletes the comment and is verified by re-parse + DeepEqual with bisection (`:389-432`).

**Filter order** (`linter.go:1067-1175`): rules → unreachable-code drop → levels → config ignores (matched) → inline/zizmor → unused-ignore →
`paths.*.ignore`/`--ignore` → generated-file drop (`generated_files.go`: "DO NOT EDIT" header ⇒ drop policy/style) → sort/dedupe →
baseline → `--min-severity`. `--no-ignores` disables all but `--ignore`.

**Baseline** (`baseline.go` 481 + `linter_baseline.go` 487 lines; default `.github/jactionlint-baseline.json`):
```json
{"version":1,"entries":[{"file":".github/workflows/ci.yaml","rule":"missing-timeout",
  "fingerprint":"7f915da24540610a","context":"2fcd66cb1402c448","occurrence":0,"message":"..."}]}
```
`fingerprint` = sha256/16 hex of (rule, structural scope `jobs/build`, whitespace-collapsed source line, message with line/col/root normalised)
(`baseline.go:164-175,307-328`); `context` = same without message (survives rewording); `occurrence` disambiguates duplicates. Two-pass match
(exact, then context, with a hand-curated deny-list, `:378-446`). Line moves/reindent/CRLF don't break it; editing the line or renaming the
job does. `--baseline-write[=FILE]` (exit 0; partial rewrite for given files), `--baseline-check` → `unused-baseline-entry`; SARIF emits
baselined findings as `suppressions`. `first_run_hint.go` suggests `--baseline-write` at ≥20 findings.

**`--changed-since REV`** (`changed_since.go`, 361 lines) is a **display filter only**: every file is linted, then findings are kept if on
lines changed since `merge-base(REV, HEAD)` (`git diff -U0 --relative --no-renames <base>` incl. staged/unstaged; untracked = all lines,
`:259-289`); `--changed-scope files` widens to whole files. Shallow clone / unknown rev is a hard error. jj support is only `.jj` root
detection (`project.go:47-49`).

---

## 5. Autofix engine

- **Fix = data on the finding**: `Fix{Description, Unsafe, Edits []TextEdit{Start, End, NewText}}` over byte offsets of the original
  source (`error.go:81-90,133-141`), or `NoFix{Code, Reason, Option}` explaining why not (`option-required`, `online-required`,
  `lookup-failed`, `unsupported-shape`, `needs-judgment`, `error.go:92-131`). JSON output carries both (seen on corpus:
  `"no_fix":{"code":"option-required",…,"option":"missing-timeout.default-minutes"}`).
- **No re-serialisation**: because yaml v4 gives no end positions, fixers locate text with line-based helpers (`srcDoc` in
  `fix_source.go:10-375`; `dependabotSource` in `dependabot_fix.go:9-183`) that bail out (`unsupported-shape`) on flow style, tabs,
  anchors, lone CR. Scalar rendering is context-aware (plain/single/double/block, `fix_scalar.go:24-107`) and quotes anything that
  would not round-trip or that matches YAML 1.1 specials (`fix_scalar.go:183-215`).
- **Planner** (`fix.go:78-190`): deterministic order (safe before unsafe → `fixPriority` map → rule → offset); greedy accept into a
  sorted `editSet` (binary-search conflict check); overlapping or same-offset inserts conflict; a dropped fix is retried next pass.
- **Fixpoint** (`fix.go:435-530`): up to `maxFixPasses = 10` (`fix.go:29`) of plan → apply → verify → full re-lint in memory; SHA-256
  of each text detects cycles → rollback + exit 3. Baselined findings *are* fixed; ignored ones are not.
- **Verification** (`fix_verify.go`): new text must be UTF-8 and parse to the same number of documents; old/new node trees are
  diffed (maps by key, sequences by LCS, scalars by tag+value) and **every structural difference must overlap an edit's range**
  (`fix_verify.go:181-440`). On failure, `applyVerified` re-applies fixes one by one and bans the offender for the file (`fix.go:243-263`).
  It guarantees "syntax intact and nothing changed away from the edits", not semantic correctness.
- **Write**: atomic temp+fsync+rename, preserves mode, follows symlinks, refuses if the file changed since read (`fix.go:609-658`).
  `--diff` uses an in-house Myers diff (`fix_diff.go`, `git apply`-compatible). SARIF carries safe fixes only.
- **27 fixable rules**: template-injection, insecure-commands, artipacked, bot-conditions, unpinned-uses (online for SHA lookup),
  self-repository, obfuscation, missing-permissions (via the `actionPermissionTable` of ~25 popular actions with source URLs,
  `action_permissions.go:164-455`), missing-timeout / dependabot-cooldown / mutable-runner-label (only if the user configures the value —
  "jactionlint never invents a timeout"), anonymous-definition, unused-ignore, concurrency-limits, concurrency-cancels-release,
  pipeline-without-pipefail, environment-file-redirect, malformed-expression-delimiters, multiline-plain-run, invisible-characters,
  fail-fast-action-inputs, unlocked-install, missing-version-comment, ref-version-mismatch, unpinned-images, dependabot-execution,
  dependabot-missing-actions-update.
- **Invariant tests**: every finding of a fixable rule has `Fix` or `NoFix` (`no_fix_invariant_test.go:15-46`); linting never writes and
  fixing preserves comments/`uses:` lines/line endings for every fixture (`source_write_invariants_test.go:43,115`).
- Weak spots: node extents guessed "up to the next node" (`fix_verify.go:31-36`); `fixPriority` covers 11 of 27 rules; re-lint per pass.

---

## 6. Online mode

- **Off by default**; CLI > `JACTIONLINT_ONLINE` > config; `--no-online` wins (`online.go:30-47`). Modes `cache` (no network, any age)
  and `strict` (exit 3 on any skipped lookup). `--online-report FILE` writes coverage JSON. First file's config decides for the run (`online.go:11-14`).
- **Fetches** (REST paths in `github_http.go:1138-1361`): `/repos/{o}/{r}` (archived, id/full_name for repojacking), tags/refs/annotated
  tags (pins, version comments, ref-confusion), branches+compare or a GraphQL branch scan (impostor commits, `github_http.go:1450,1576`),
  `/contents` (remote action.yml, remote reusable workflows), `/advisories?ecosystem=actions` (bulk, 5 pages, per-package fallback),
  `/users/{login}` (repojacking), releases + commit dates (immutable releases, cooldown), `/environments/{name}` (reviewer gates),
  workflow runs/jobs (`--suggest-timeouts`), Docker Hub / ghcr.io manifest HEADs for image digests (`registry_http.go:19-126`).
- **Auth** (`github_auth.go:68-210`): `--online-token-env` → `--online-token-file` → `GITHUB_TOKEN` → `GH_TOKEN` → `gh auth token`.
  401 ⇒ drop token, continue anonymously; config-supplied API URLs never get the token (`online.go:85`); cross-origin redirects refused.
- **Cache** (`github_cache.go`): `$XDG_CACHE_HOME/jactionlint`, one JSON per `sha256(scope|url)`, 32 MiB LRU, ETag revalidation after
  TTL (default 1 h), SHA-addressed answers immutable, 404s cached; per-token scope plus a "shared public" slot so rotating CI tokens still hit.
  Per-run memo with `sync.Once` per key (`github_session.go:42-176`).
- **GHES**: `--online-api-url`; opt-in `--online-github-com-fallback` asks GHES first and api.github.com only on 404 (GitHub Connect actions),
  separate `GITHUB_COM_TOKEN`, never sends the GHES token to github.com (`github_fallback.go:11-70`).
- **Resilience**: 20 s timeout; retries with jittered backoff honouring `Retry-After`; waits for primary rate limit up to `--online-max-wait`
  (30 s) else serves stale cache; session gives up after 3 consecutive timeouts/5xx, a 401, DNS failure (`github_session.go:15-18,453-515`);
  failed lookups ⇒ no finding + `lookup-failed` NoFix. Tests replay fixtures (`github_fixture.go`, `testdata/online/github.json`).
- **Online-only rules (10)**: action-release-cooldown (also opt-in), actions-lock-drift, archived-uses, impostor-commit,
  known-vulnerable-actions, ref-confusion, ref-version-mismatch, repojacking, stale-action-refs, unpinnable-action. Online also enriches
  offline rules (remote action inputs, reusable workflow inputs/secrets, pin fixes, environment-reviewer gating).
- Size: ~7.5k source + ~10k test lines of infrastructure (`github_http.go` alone 1.7k) — most of it defensive plumbing.

---

## 7. Script analysis, JS, Python

- **Bash is parsed in-process with `mvdan.cc/sh/v3`**, wrapped by `internal/runscript` (5.4k lines incl. tests) into a semantic model:
  commands with wrappers/flags/positionals/redirects/pipeline stage, guards and if/case totality, package installs for
  pip/uv/pipx/npm/cargo/go/gem/apt/brew (`internal/runscript/install.go`, 727 lines), `curl | sh`. `${{ }}` is replaced by a same-length
  placeholder `X0___` so offsets map back (`internal/runscript/script.go:8-22`). Only bash/sh.
- `shell-syntax` (`rule_shell_syntax.go`, 213 lines) reports only structural bash errors and stays silent when shellcheck runs, on Windows,
  unknown shells, or CR in script — a narrow "shellcheck-lite" (1 finding on the corpus).
- **Taint** (`script_taint.go` 404, `rule_github_env_flow.go` 251, `rule_command_injection.go` 411, `rule_argument_injection.go` 500 lines):
  sources = untrusted context table + env indirection (`template_injection.go:169,633-708`) + command outputs (`gh pr view --json title`,
  `git log` after PR checkout), sinks = `$GITHUB_ENV`/`$GITHUB_PATH`, `eval`/`bash -c`/`python -c`, option injection into
  sed/awk/find/curl/git, container options, a curated `codeExecTable` of action inputs, AI-agent prompts (`injection_sinks.go:9-60`).
- A second, separate hand-written shell quote scanner (`shell_scan.go`, 250 lines) decides when the template-injection fix can rewrite
  `${{ x }}` → `"$ENV"`; `macos_script_aware.go` (349) calls mvdan directly for `macos-gnu-flags`. Three expression-sanitisers coexist.
- **github-script**: no JS parser in Go. `github-script-syntax` pipes each script into **`node --check -`** wrapped as an async function
  (`rule_github_script_syntax.go:160-273`), requires node ≥ 20 (`:320-329`), skipped with a note otherwise. Separately,
  `command_injection_javascript.go` (1,013 lines) is a hand-written byte scanner for `child_process` sinks — the most heuristic code in the repo.
- **Python**: `--ruff=CMD` *replaces* pyflakes with `ruff check --select F --output-format json -`, still reported as `pyflakes`
  (`rule_pyflakes.go:69-71,193-209`). `external_tools.go` tracks ran/missing/disabled per tool and surfaces it in notes and SARIF.

---

## 8. New file kinds, syntax and data

- **action.yml** (`action_file.go`, 349 lines): `ParseAction` returns a `Workflow` with `.Action`; composite steps go into a synthetic
  job so all step rules apply (`action_file.go:13-24,102-167`); checks keys per `runs.using`, required `main/image/steps`, composite
  `run` needs `shell`, no `timeout-minutes` (`:184-317`). `action_graph.go` (628 lines) builds a repo-wide caller graph (workflows →
  local actions / `$/` / reusable workflows, BFS) so composite steps are judged with their most dangerous calling trigger (`:47-306`).
- **Actions lock file**: `.github/workflows/actions.lock`, described as a GitHub technical preview written by `gh actions-lock`
  (format repo `github/actions-lockfile`, versions v0.0.2/0.0.3 only; `actions_lock.go:18-44`, `docs/checks.md:4194-4330`). *I could not
  verify this GitHub feature independently.* `actions-lock` (offline, `rule_actions_lock.go`, 600 lines) checks drift/format/staleness;
  `actions-lock-drift` (online) checks moved tags, repo-id changes, impostor commits; `unpinned-uses` accepts lock-pinned tags.
- **dependabot.yml**: own AST/parser (`dependabot_parse.go` 642 lines) with hand-kept enums from GitHub docs (`:17-37`), no JSON schema;
  rules `dependabot-syntax`, `dependabot-cooldown` (fixable), `dependabot-execution`, `dependabot-missing-actions-update`, `hardcoded-registry-credentials`.
- **New workflow syntax** (`git diff 011a6d1 -- parse.go ast.go`: +615/−57): parallel steps `background/wait/wait-all/cancel/parallel`
  (`ast.go:542-583,876-880`, `parse.go:1429-1566`; rules `invalid-parallel-step`, `background-step-not-waited`), `cache-mode`
  (`parse.go:869,1704,1875`), `concurrency.queue` (`parse.go:983-1003`), `cancel-timeout-minutes`, root `description`, `${{ insert }}`,
  `$/path` uses (`uses.go:10-15`), permissions `code-quality`, `copilot-requests`, `drives`, `vulnerability-alerts` (`rule_permissions.go:95-116`).
- **Event payloads** (`event_payloads.go`, generated from octokit/webhooks @aa80c24 + rest-api-description 1.1.4, 3 levels deep, stored as a
  string DSL parsed at runtime): `undefined-property` checks `github.event.*` names — any-event union by default, per-trigger with
  `strict-event-payloads`/pedantic (`event_payload.go:180-250`).
- **Runner labels**: third-party providers table (blacksmith, depot, namespace, runs-on, ubicloud, warpbuild; `runner_label_providers.go`),
  larger-runner suffix regex (`rule_runner_label.go:212-230`), and a **dated retirement table evaluated against the wall clock**
  (`runner_label_retirement.go:24-27,76-81`) — non-hermetic results.
- **AI agent actions**: `agent_actions.go` (666 lines) curates ~14 agent actions (claude-code-action, run-gemini-cli, codex-action, …) with
  prompt inputs (injection sinks), write gates and unsafe settings; `agentic-actions` flags outsider-triggerable agents.
- **Schema conformance** (`schema_conformance_test.go`, 828 lines): compares parser tables against **actions/runner's own schemas**
  (`src/Sdk/WorkflowParser/workflow-v1.0.json`, `src/Runner.Worker/action_yaml.json`) vendored in `testdata/schema/runner/` at a pinned
  commit; every difference must be in a reasoned allowlist (`:772+`, e.g. `newerThanPin`), stale entries fail. Weekly
  `.github/workflows/schema-pin.yaml` + `scripts/update-schema-pin.bash` bump the pin via PR. `schema_drift_test.go` (102 lines) is just
  hand-written assertions for recent additions. Not SchemaStore, and `actions/languageservices` only as a cited secondary source.

---

## 9. Rule catalogue (161 IDs)

Authoritative source: the registry (`registerRules` in each rule file's `init()`, types `rule_registry.go:11-141`); dump with
`jactionlint --list-rules --format json`. Totals: groups correctness 81 / security 54 / policy 22 / style 4; profiles correctness 74 /
default 47 / pedantic 27 / online 10 / opt-in 3; default level error 130 / warn 24 / info 7; 27 fixable.

**56 IDs are upstream checks split into finer IDs** (all in `correctness`; mapping in `docs/actionlint.md`):
syntax-check → workflow-syntax, duplicate-key, yaml-syntax, merge-key, recursive-alias, unused-anchor (`parse.go:1985-1990`);
expression → expression-syntax, expression-type, undefined-property, undefined-function, invalid-function-call, context-availability,
template-injection, workflow-input-type; if-cond → constant-condition, if-always-true; action → invalid-uses, missing/unknown/deprecated-action-input,
outdated-action-runner, invalid-local-action (`rule_action.go:832-838`); events → unknown-event, invalid-activity-type, invalid-event-filter,
invalid-event-config, invalid-workflow-dispatch-input, invalid-workflow-call-input, invalid-cron, cron-too-frequent, invalid-timezone
(`rule_events.go:336-344`); workflow-call → invalid-workflow-call, missing/unknown-workflow-input, missing/unknown-workflow-secret,
invalid-local-workflow; job-needs → undefined/cyclic/duplicate-job-needs, duplicate-job-id; others → matrix-duplicate-value,
matrix-invalid-exclude, unknown-runner-label, conflicting-runner-labels, invalid-label-pattern, invalid-id, duplicate-step-id,
invalid-permissions, invalid-glob, hardcoded-container-credentials, invalid-env-var-name, invalid-shell-name, deprecated-commands,
shellcheck, pyflakes. Behaviour changes to these: rule ID printed instead of kind, `template-injection` has more sinks and (default profile)
more sources, `undefined-property` types `github.event`, shellcheck positions point at the token, warn/info don't fail, `.github/workflows`
subdirectories not scanned, strict config (unknown key → exit 3).

**105 new rules.** Legend: C/D/P = correctness/default/pedantic profile, On = online-only, Opt = opt-in; F = autofix; Z = mirrors a zizmor
audit (`docs/zizmor-parity.md` claims a rule for every zizmor 1.30.1 audit); level error unless (w)/(i). Corpus = hits on the 1,503-file
corpus in that rule's own profile (default run unless P).

| Category | Rule (profile, flags) — one line | Corpus |
|---|---|--:|
| Injection / untrusted input | `github-env` (D, Z) outsider input written to `$GITHUB_ENV`/`$GITHUB_PATH` | 7 |
| | `command-injection` (P w) env-laundered input reaching `eval`/`bash -c`/`python -c` | 0 |
| | `argument-injection` (P w) outsider text as options of sed/awk/find/curl/git | 0 |
| | `output-clobbering` (P w) newline injection into `$GITHUB_OUTPUT` | 0 |
| | `comment-trigger-authorization` (P w) comment-triggered job never checks commenter permission | 13 |
| | `agentic-actions` (D) AI agent action outsiders can steer / safeguards off | 0 |
| | `untrusted-checkout` (D) privileged trigger runs PR code (incl. TOCTOU) | 0 (4 in P) |
| | `untrusted-artifact` (D) artifact of triggering run used unvalidated | 2 |
| | `dangerous-triggers` (D, Z) `pull_request_target` / `workflow_run` / `issue_comment` with privileges | 126 |
| | `bot-conditions` (D, F, Z) spoofable `github.actor == 'x[bot]'` | 4 |
| | `unsound-contains` (D, Z) `contains('a,b', x)` substring check | 2 |
| | `unsound-prefix-match` (D) startsWith/endsWith on account/repo names | 0 |
| | `obfuscation` (D, F, Z) computed indexes / obfuscated expressions | 7 |
| | `invisible-characters` (D, F) bidi/invisible Unicode | 0 |
| | `cache-poisoning` (D, Z) cache restored in release/tag/privileged workflows | 116 |
| Tokens, secrets, runners | `excessive-permissions` (D, Z) token gets broader write access than needed | 1 (1,502 in P) |
| | `artipacked` (D, F, Z) checkout persists credentials | 1,934 |
| | `checkout-static-credentials` (D) checkout given an SSH key or non-expiring token | 0 (19 in P) |
| | `overprovisioned-secrets` (D, Z) `secrets[...]`/`toJSON(secrets)` | 6 |
| | `unredacted-secrets` (D, Z) secret parsed with `fromJSON()` so its fields are not redacted | 0 |
| | `secrets-inherit` (D, Z) `secrets: inherit` | 0 |
| | `secrets-outside-env` (P w, Z) job uses a secret without an `environment:` | 1,307 |
| | `workflow-secret-scope` (P w) secret exposed at workflow-level env | 20 |
| | `github-app` (D, Z) GitHub App token issued with more access/longer life than needed | 2 |
| | `insecure-commands` (D, F, Z) `ACTIONS_ALLOW_UNSECURE_COMMANDS` | 1 |
| | `misfeature` (D, Z) setup-python `pip-install` input or `shell: cmd` | 67 |
| | `self-hosted-runner` (P i, Z) job runs on a self-hosted runner/group | 31 |
| | `use-trusted-publishing` (D, Z) token-based publish to npm/PyPI/crates | 41 |
| Supply chain (offline) | `unpinned-uses` (D policy, F online, Z) action/workflow/image not SHA-pinned; policies per pattern | 7,133 |
| | `unpinned-images` (D, F, Z) container/service image not digest-pinned | 194 |
| | `unpinned-tools` (D, Z) action installs newest tool version (no version / `latest`) | 0 (600 in P) |
| | `adhoc-packages` (D, Z) package installed by name with npm/yarn/pnpm/bun/gem outside a lock file | 134 |
| | `unlocked-install` (D, F) `cargo install` without `--locked`, npm/yarn/pnpm install not freezing an existing lock file | 4 (1,662 in P) |
| | `unverified-download` (D) `curl \| sh` / unverified binaries | 98 |
| | `insecure-url-scheme` (D, Z) http:// fetches | 24 |
| | `insecure-ssh-keyscan` (D) `ssh-keyscan` trust-on-first-use | 0 |
| | `typosquat-uses` (D, Z) `uses:` resembling popular actions | 0 |
| | `superfluous-actions` (D, Z) action duplicates a runner-image tool (e.g. `gh release create`) | 65 |
| | `actions-lock` (D) `.github/workflows/actions.lock` drift/format | 0 |
| | `self-repository` (P i, F, Z) local action/workflow referenced as `./path` instead of `$/path` | 411 |
| | `missing-version-comment` (P w, F) SHA pin without `# vX.Y.Z` | 93 |
| | `forbidden-uses` (Opt, Z) / `required-actions` (Opt) allow/deny/require lists | — |
| Online-only | `action-release-cooldown` (On w, also opt-in) version newer than cooldown | — |
| | `actions-lock-drift` (On) lock pin no longer matches GitHub | — |
| | `archived-uses` (On w, Z) action repo archived | — |
| | `impostor-commit` (On, Z) pinned SHA not in repo history (fork network) | — |
| | `known-vulnerable-actions` (On, Z) GHSA advisory for the action version | — |
| | `ref-confusion` (On w, Z) ref is both branch and tag | — |
| | `ref-version-mismatch` (On w, F, Z) `# v1.2.3` comment ≠ SHA | — |
| | `repojacking` (On w) owner renamed/deleted, name re-registrable | — |
| | `stale-action-refs` (On i, Z) SHA not on any tag | — |
| | `unpinnable-action` (On w, pedantic option) SHA-pinned action pulls in actions/images by tag, so the pin doesn't freeze it | — |
| Dependabot | `dependabot-syntax` (C) schema of dependabot.yml | — |
| | `dependabot-cooldown` (D, F, Z) missing/short cooldown | — |
| | `dependabot-execution` (D, F, Z) `insecure-external-code-execution: allow` | — |
| | `hardcoded-registry-credentials` (D) literal registry creds | — |
| | `dependabot-missing-actions-update` (P w, F) no github-actions ecosystem / misses composite dirs | — |
| Action metadata | `action-syntax` (C) action.yml schema per `runs.using` | — |
| New correctness | `background-step-not-waited`, `invalid-parallel-step` (C) parallel-steps semantics | 0 |
| | `concurrency-deadlock` (C) caller and local callee share a concurrency group | 0 |
| | `conflicting-action-inputs` (C) mutually exclusive inputs of known actions | 0 |
| | `environment-file-redirect` (C, F) write to `GITHUB_OUTPUT` without `$`, or truncating `>` after writing | 0 |
| | `gh-without-token` (C) `gh` used without `GH_TOKEN` | 0 |
| | `github-script-syntax` (C) JS syntax error in actions/github-script (via node) | 0 |
| | `local-action-checkout` (C) local action before checkout | 1 |
| | `malformed-expression-delimiters` (C, F) `{{ x }}` / `${ x }` that are not expressions | 28 |
| | `multiline-plain-run` (C, F) multi-line plain scalar `run:` folding lines | 0 |
| | `unsound-ternary` (C, Z) `a && b \|\| c` with falsy b | 0 |
| | `workflow-call-permissions` (C) callee needs more permissions than caller grants | 0 |
| | `workflow-run-names` (C) `workflow_run.workflows` name not found | 0 |
| | `shell-syntax` (D) bash syntax error (mvdan/sh) when shellcheck absent | 1 |
| | `pipeline-without-pipefail` (D, F) failure hidden in a pipeline under default `bash -e` | 95 |
| | `concurrency-cancels-prs` (D) shared group + cancel cancels unrelated PRs | 3 |
| | `concurrency-cancels-release` (D, F) cancel-in-progress on release/deploy | 1 |
| | `gate-job-skipped-on-failure` (D) gate job reading `needs` results is skipped when a need fails (skipped required check passes) | 0 |
| | `insufficient-permissions` (D) job permissions lack what a known action needs | 0 |
| | `ubuntu-slim-limits` (D) container/services/Docker action on single-CPU `ubuntu-slim` | 0 |
| | `event-name-never-received` (P w) `github.event_name == 'x'` for an untriggered event | 7 |
| | `fail-fast-action-inputs` (P w, F) action that passes when nothing was produced is not told to fail (e.g. upload-artifact `if-no-files-found`) | 528 |
| | `macos-gnu-flags` (P w) GNU-only flags of sed/date/grep on macOS runners | 0 |
| | `unmatched-paths-filter` (P w) `paths:` glob matches no file | 0 |
| Policy / style | `missing-permissions` (D, F) no `permissions:` | 2,828 |
| | `missing-timeout` (D, F needs option) no `timeout-minutes` | 2,586 |
| | `concurrency-limits` (D, F, Z) no `concurrency:` | 1,277 |
| | `duplicate-triggers` (D) push+pull_request double runs | 285 |
| | `unused-job-output` (D) job output nobody reads | 17 |
| | `timeout-too-long` (Opt) | — |
| | `anonymous-definition` (P w, F, Z) job/step without `name:` | 1,717 |
| | `continue-on-error` (P i), `deprecated-node-runtime` (P w), `deprecated-runner-label` (P w), `mutable-runner-label` (P w, F) | 149 / 16 / 0 / 2,356 |
| | `undocumented-permissions` (P i, Z), `unused-workflow-input` (P w), `unused-workflow-secret` (P w), `unused-needs` (P i) | 17 / 5 / 0 / 30 |
| | `max-run-lines`, `require-expression-wrapping`, `require-shell` (P) | 6 / 1,880 / 6,782 |
| Ignore hygiene | `invalid-ignore-comment` (C), `expired-ignore` (C), `unused-baseline-entry` (C i), `unused-ignore` (P, F) | — |

The full generated list with summaries is in the appendix.

---

## 9a. Output formats, facts, schemas, distribution

- **Formats** (`format.go:20-34`, `docs/output.md`): text (with snippet; rule ID at end), oneline, gcc, github (`::error file=…,title=<rule-id>::`),
  native SARIF 2.1.0, json, jsonl, summary (`format_summary.go`), Go template (upstream-compatible). No checkstyle/GitLab/JUnit.
  JSON finding: `message, filepath, line, column, end_line, end_column, kind (legacy), id, severity, doc_url, snippet, fix{description,unsafe,edits[{start,end,new_text}]}, no_fix{code,reason,option}`.
  SARIF: rule tags = profile+group; `partialFingerprints.primaryLocationLineHash` reuses the baseline context hash; only safe fixes; baselined → `suppressions`;
  run properties `externalTools`, `onlineCoverage`. `--finding-context` adds workflow events and the job's effective permissions (`finding_context.go`).
- **Schemas** (draft-07, committed under `schema/`, printed by `--print-*-schema`, drift-tested): output (175 lines, additive-only per major), config
  (4,820 lines, generated from registry), facts (885 lines).
- **Facts export** (`facts.go`, `FactsVersion = 1` at `facts.go:27`): `--export-facts` dumps the analysed repo model — workflows (events, filters, `called_by`,
  permissions with `source` job/workflow/repository-setting, concurrency, jobs, steps), actions with callers, local call graph — with positions and explicit
  unknowns (`dynamic: true`, `known: false`), for OPA/Conftest. No linting.
- **Distribution**: release-please → GoReleaser; daily auto-merge of the release PR once the last tag is ≥7 days old; weekly `go generate` data refresh into
  the release PR; 13 OS/arch targets, CGO off; macOS codesign + notarisation (`release.yaml:43-138`); sha256 + sigstore "packslip" bundle + `attest-build-provenance`
  (`release.yaml:142-152`), no cosign; PyPI wheels via `uvx go-to-wheel` with trusted publishing (`release.yaml:191-243`; wheels not attested); Homebrew cask in-repo
  (depends on shellcheck); Docker (Alpine + shellcheck + pyflakes); root composite **`action.yml`** downloading the exact pinned version, with `annotations`
  (github format) or `advanced-security` (SARIF upload) modes, `online` + `token` inputs; 4 pre-commit hooks (incl. wasi-built shellcheck; slated for removal);
  hk (`hk.pkl`) dogfooding via SARIF diff; mise (`mise.toml`, ~30 tasks); WASM playground (`playground/main.go`, VitePress site); man page; completions incl. rule IDs.

---

## 9b. Quality signals

| Metric | jactionlint | upstream |
|---|--:|--:|
| Root non-test LOC | 90,630 (281 files) | 18,203 |
| Root test LOC | 69,710 (310 files) | 12,511 |
| `func Test` | 1,500 (+136 elsewhere) | 169 |
| testdata | 1,619 files, 8.1 MB | — |
| docs | ~21.8k lines (`docs/checks.md` 10,388) | — |
| `go test ./...` | passes; root package ~279 s | — |

Largest non-generated files: parse.go 1,992, github_http.go 1,711, rule_expression.go 1,548, linter.go 1,420, template_injection.go 1,414,
rule_cache_poisoning.go 1,376, expr_sema.go 1,269, command_injection_javascript.go 1,013, rule_untrusted_checkout.go 960.

**Validation that works and is worth copying**
- `actionlint_compat_test.go` (`TestCorrectnessProfileKeepsEveryErrorOfActionlint`): all 147 upstream fixtures under `profile: correctness`
  must still produce every expected upstream error; exceptions map is empty (some `.out` files were reworded though).
- `schema_conformance_test.go` against vendored actions/runner schemas with a reasoned allowlist + weekly pin bump.
- `TestRuleIDsAreStable`, `TestSchemasAreUpToDate`, `TestRulesDocIsUpToDate` — generated artefacts can't drift.
- Fix invariants (`no_fix_invariant_test.go`, `source_write_invariants_test.go`), `scripts/fix-corpus` (fixes are valid, idempotent, never increase findings).
- `perf_equiv_test.go`: optimised structures (edit-set, line index) checked against naive reference implementations with random inputs.
- `scripts/zizmor-diff` differential harness with a dated measurement ledger.

**Sprawl / fragility signals**
- Built in ~10 days by agents: 229 commits with "Generated with Claude Code", 209 `Co-authored-by: Claude Sonnet 5.5`, 34 Opus; 222 PRs reviewed by
  Cursor Bugbot, 73 by CodeRabbit; single PRs of +6–16k lines (e.g. `be4bf706`: 194 files, +6,123).
- Files named after process, not domain: `rule_review211_test.go` … `rule_review211f_test.go`, `rule_review229b_test.go`,
  `rule_batch_a_util.go` (445 lines, production), `rule_batch_h_helpers.go` (607), `config_ignores_round3_test.go`,
  `template_injection_r3_test.go`, `rule_cache_poisoning_ramsey_test.go`.
- 39 test files each define their own `lint*` helper (71 distinct helper names).
- One 540-file root package with ~1,570 exported functions (`v3.md:16-20`).
- Many dated, hand-curated tables (agent actions, action permissions, input dependencies, code-exec inputs, runner providers, retirement dates,
  dependabot enums) with "verified 2026-10-13" stamps — high upkeep.
- Wall-clock-dependent results (runner retirement), external node, pre-release YAML library, text-indentation heuristics in ignores/baseline/fixers,
  two shell analysers + three expression sanitisers, prose-heavy 10k-line docs as the de-facto spec.
- Measurement ledger admits the 1,109-repo figures are not reproducible (`docs/zizmor-measurements.md`).

---

## 10. Performance and noise on the 1,503-file corpus

Machine: Apple M1 Pro. Corpus copied to a scratch directory (outside git). Both with shellcheck/pyflakes disabled
(`-shellcheck= -pyflakes=` / `--shellcheck= --pyflakes=`). `jactionlint` built into a scratch directory from `./cmd/jactionlint` (86 s cold build).

| Run | Wall | User | Max RSS | Findings |
|---|--:|--:|--:|--:|
| upstream actionlint | 0.44–1.07 s | 0.41–0.47 s | 40–46 MB | 6,836 (1,296 files) |
| jactionlint `--profile correctness`, node not on PATH | 0.67 s | 0.88 s | — | 7,180 |
| jactionlint default profile, node not on PATH | 0.82–1.07 s | 2.5 s | 67–85 MB | 24,299 (all 1,503 files) |
| jactionlint `--profile pedantic`, node not on PATH | 1.31 s | 2.85 s | — | 45,433 |
| jactionlint default/correctness **with node 26 on PATH** | **16.4–19.4 s** | 4.5–5.2 s | — | same (+0 `github-script-syntax`) |

- The 16 s comes from 69 `actions/github-script` steps each spawning `node --check`; on this machine one node start costs ~0.24 s and
  starts don't parallelise (8 parallel `node --check` = 2.0 s), so this is partly environmental, but it shows the cost of an external
  per-script process for a rule that found **zero** problems here. An in-process JS parser (oxc/swc in Rust) avoids it entirely.
- Without node: ~2.4× upstream wall, ~5× user CPU, ~2× RSS. No single default rule dominates (each alone 0.4–0.9 s incl. startup);
  cost is spread over ~150 rules, mvdan/sh parses and text-heuristic helpers.

**Correctness profile vs upstream** (position-level diff): 147 upstream findings without a jactionlint finding at the same
position — 119 `runner-label` (deliberate: larger-runner names like `ubuntu-latest-xl`, `windows-latest-xl` accepted by
`largerRunnerSizeRe`; GPU/custom labels next to `self-hosted`), 26 `expression` that jactionlint reports **1–2 lines lower, at the
correct line inside block scalars** (upstream bug, `05-issues.md` theme 7), 2 `create: tags:` null-filter findings (deliberately
accepted, `parse.go:592`). Extra in correctness: 432 `outdated-action-runner` for tags like `@v2.3.4` (checked as `@v2`, upstream misses
them), 28 `malformed-expression-delimiters` (`{{ github.branch }}` without `$` — true positives), 10 `expression-type` incl.
`github.repository.default_branch` in nodejs/node (real bug upstream misses), 1 `local-action-checkout` (true positive in next.js).

**Default profile, per rule** (top): unpinned-uses 7,133 · outdated-action-runner 6,000 · missing-permissions 2,828 · missing-timeout 2,586 ·
artipacked 1,934 · concurrency-limits 1,277 · unknown-runner-label 705 · duplicate-triggers 285 · deprecated-commands 276 · unpinned-images 194 ·
adhoc-packages 134 · dangerous-triggers 126 · cache-poisoning 116 · unverified-download 98 · pipeline-without-pipefail 95 · undefined-property 76 ·
template-injection 68 · misfeature 67 · superfluous-actions 65 · use-trusted-publishing 41 · malformed-expression-delimiters 28 ·
insecure-url-scheme 24 · workflow-syntax 23 · expression-type 22 · unused-job-output 17 · constant-condition 15 · (≤7 each) invalid-event-filter,
github-env, obfuscation, unknown-action-input, overprovisioned-secrets, unlocked-install, invalid-glob, bot-conditions, concurrency-cancels-prs,
cron-too-frequent, unsound-contains, untrusted-artifact, if-always-true, github-app, expression-syntax, invalid-activity-type, shell-syntax,
insecure-commands, excessive-permissions, local-action-checkout, concurrency-cancels-release. Pedantic adds require-shell 6,782,
mutable-runner-label 2,356, require-expression-wrapping 1,880, template-injection +1,729, anonymous-definition 1,717, unlocked-install 1,662,
excessive-permissions 1,502, secrets-outside-env 1,307, …

**Sample of 30 new-rule findings** (manual judgement; corpus is real public workflows of ~2020–2022 vintage):

| Rule | File:line | Verdict |
|---|---|---|
| duplicate-triggers | openssl cross-compiles.yml:3 `on: [pull_request, push]` | TP (policy: double runs) |
| adhoc-packages | drawio electron-builder.yml:42 `sudo npm install -g yarn` | Policy nit; arguably noise |
| dangerous-triggers | eShopOnContainers payment-api-deploy.yml:8 `workflow_run` w/ secrets | TP (zizmor-equivalent) |
| dangerous-triggers | PowerToys spelling.yml:3 `pull_request_target` | TP |
| dangerous-triggers | slate comment.yml:5 `issue_comment` w/ NPM_TOKEN | TP |
| cache-poisoning | obs-studio main.yml:649 cache on tag-push workflow | TP (zizmor-equivalent) |
| unverified-download | Sentinel ci.yml:28 `bash <(curl codecov.io/bash)` | TP (message advertises mise) |
| pipeline-without-pipefail | Rocket.Chat build_and_test.yml:347 `curl … \| sed \| sh` | TP |
| misfeature | folly getdeps_windows.yml:20 `shell: cmd` | Policy, debatable |
| superfluous-actions | black upload_binary.yml:47 `actions/upload-release-asset` | TP (archived action) |
| use-trusted-publishing | vuetify nightly.yml:56 `npm publish` with token | TP (advice) |
| insecure-url-scheme | parcel nightly-release.yml:145 `curl -O http://musl.cc/…` | TP |
| unused-job-output | tauri covector…yml:18 `commandRan` | Likely TP (cannot see external callers) |
| constant-condition | eShopOnContainers …deploy.yml:21 `if: false` | TP but intentional (upstream #594 complaint) |
| github-env | airflow build-images.yml:166 value from grep of repo file in `pull_request_target` | Borderline/likely FP (base checkout) |
| obfuscation / overprovisioned-secrets | rust-lang ci.yml:135/507 `secrets[format(…)]` | TP (zizmor-equivalent) |
| bot-conditions | swagger-ui dependabot-merge.yml:9 `github.actor == 'dependabot[bot]'` | TP |
| concurrency-cancels-prs | Alamofire ci.yml:21 `group: ci` + cancel | TP (real bug) |
| untrusted-artifact | superset docker-ephemeral-env.yml:17 | TP |
| unsound-contains | Kong perf.yml:22 `contains('["OWNER",…]', …)` | TP |
| github-app | iptv clean.yml:193 no `permissions` input | TP (policy) |
| artipacked | Rocket.Chat build_and_test.yml:287 | TP per zizmor; high-volume policy |
| concurrency-limits | vim codeql-analysis.yml:8 | Policy; noisy (1,277 hits) |
| template-injection ×4 | TypeScript new-release-branch.yaml:24/37, set-version.yaml:31 (`client_payload`), netdata dashboard-pr.yml:26 (`inputs.*`) | TP in mechanics; `workflow_dispatch` inputs / `repository_dispatch` require write access → low-severity by zizmor's own standards |
| unpinned-images | osquery build_x86.yml:44 tag not digest | Policy |
| unknown-action-input ×2 | nodejs auto-start-ci.yml:50 `octokit/graphql-action` `owner:` | FP inherited from upstream (action passes unknown inputs as GraphQL variables) |

Net: the security rules are precise in this sample (≈1 borderline of ~20); the *volume* problem is policy rules that default to
`error` in the default profile (unpinned-uses, missing-permissions, missing-timeout, artipacked, concurrency-limits = 15.8k of 17.1k extras).

---

## 11. Issues and PRs

- **Issues (28, all closed)**. Two clusters: (a) *external users* — notarize macOS binary / cask quarantine (#67), mise registry (#81), PyPI via
  `go-to-wheel` (#276), `zizmor_parity_audits` toggle to disable checks that overlap zizmor (#278), `language: python` pre-commit (#280),
  SchemaStore config schema (#294); (b) *maintainer/agent tracking issues* filed while "rolling v2 out to ~24 repositories" — precision gaps
  in heuristics (cache-poisoning vs zizmor #99/#118, pipefail PIPESTATUS #100/#116, untrusted-checkout #101, trusted-publishing #102,
  bot-conditions #115, missing-permissions with multiple callers #117, concurrency-limits advice cancelling callees #113), fix gaps (#104, #119),
  perf on gh-aw (373 workflows, 54 MB YAML: 0.8 GB RSS / 3.7 s, #103), parallel-steps accept/reject corrections against GitHub's actual
  behaviour (#124–#127), and two big "ideas from actionlint/zizmor/ghalint/platform changes" trackers (#121, #167).
- **PRs (254; 247 merged)**: by conventional type feat 131, fix 57, chore 20, docs 16, ci 13, test 10, perf 5. Median body 4.2k chars
  (max 25k) — long design write-ups, typical of agent-authored PRs; 237 say "Generated with Claude Code", 230 have a Cursor Bugbot review.
  The first ~10 merge open upstream PRs (queue, vulnerability-alerts, code-quality, copilot-requests, create-github-app-token@v3).
- **Themes**: (1) zizmor parity as an explicit goal with a differential harness (`scripts/zizmor-diff`, `docs/zizmor-measurements.md`:
  95.7 % of zizmor's 998 findings on 23 repos also reported); (2) rollout-driven precision work (agents apply jactionlint to jdx's own
  ~24 repos and file issues); (3) distribution breadth; (4) tracking of new GitHub platform features within days (parallel steps, `$/`,
  `cache-mode`, lock file).

---

## 12. Feature-by-feature recommendation for workflint

Legend: **v1** = adopt in first release, **later** = after v1 / opt-in, **skip** = don't build.

| Feature (jactionlint ref) | Rec | Reasoning |
|---|---|---|
| Registry as single source of truth: ID, group, profile, level, options, fixable, docs anchor (`rule_registry.go:115-141`) | **v1** | Everything (list-rules, explain, config schema, docs, completion, did-you-mean) derives from it. Do it as a `static` Rust table / derive macro; gate rules *before* running (jactionlint filters after, `linter.go:1327`). |
| Stable kebab-case IDs + snapshot test + retired-ID aliases (`rule_registry.go:193`, `rule_ids.d`) | **v1** | Cheap, prevents churn. Consider reusing jactionlint/zizmor IDs where semantics match so ignore comments port over. |
| Nested profiles correctness ⊂ default ⊂ pedantic + online/opt-in buckets | **v1** (shape) | Good model. But workflint's default should be close to *correctness + high-precision security*; put pinning/permissions/timeout/concurrency policy behind a `hardened`/pedantic profile or at `warn`. jactionlint's default fails every real repo (24k errors on 1,503 files). |
| `off/info/warn/error` levels; only error fails; `--min-severity`, `--strict-exit` | **v1** | Standard. |
| Typed per-rule options validated against registry; strict config with did-you-mean | **v1** | Fixes upstream config complaints (`05-issues.md` theme 12). Use serde + generated JSON Schema (schemars) so parser and schema are one thing, not two kept in sync by tests (`config_schema_test.go:196`). |
| Read `.github/actionlint.yaml`; `extends`; XDG global config | **v1** (compat read) / **later** (`extends`) | Compatibility matters for migration; `extends` adds merge complexity. |
| `--list-rules`, `--explain` with embedded docs | **v1** | Low cost; keep per-rule docs short (not a 10k-line checks.md). |
| `--migrate-config` | **skip** | Only needed for jactionlint's own v1 keys. Support actionlint keys natively instead. |
| Inline `# … ignore=` comments; regex-on-message patterns | **v1** (IDs only) | Use AST-attached comments from our CST instead of indentation heuristics (`inline_ignore.go:228-251`). Add file-level + reason (jactionlint lacks both). Accept `# actionlint ignore=` and `# jactionlint ignore=` for interop. |
| Honour `# zizmor: ignore[...]` with zizmor scopes (`zizmor_ignore.go`, `zizmor_scope.go`) | **later** | Valuable for zizmor migrants only if we ship zizmor-equivalent rules; scoping semantics are subtle (542+216 lines). |
| `unused-ignore` (+ fix) | **v1** | High value, small. Must know which rules ran (jactionlint's skip logic, `inline_ignore.go:302-354`). |
| Structural config `ignores:` with `reason` and `expires` (`config_ignores.go`) | **later** | Nice for orgs; baseline + inline cover v1. |
| Line-free baseline (fingerprint + context + occurrence) (`baseline.go:164-446`) | **v1** | Essential for adopting stricter profiles; fingerprint off CST paths (job/step IDs) not text indentation. Reuse hash for SARIF `partialFingerprints`. |
| `--changed-since` as display filter (`changed_since.go`) | **later** | Easy with `git diff -U0`; baseline covers the main use. Use gix or shell out. |
| Generated-file detection (`generated_files.go`) | **later** | Small, niche. |
| Fix-as-data with byte edits, safe/unsafe, `NoFix` reasons (`error.go:81-131`) | **v1** (data model) | Design diagnostics this way from day one; LSP code actions fall out. |
| Fix engine: planner, fixpoint ≤10 passes with cycle hash, tree-diff verification, atomic write (`fix.go`, `fix_verify.go`) | **v1** core / fixes incrementally | ~1.1k lines in Go; with real CST spans verification becomes exact and `fix_source.go` heuristics vanish. Start with a few safe fixes (deprecated commands, `$GITHUB_ENV` quoting, multiline-plain-run, malformed delimiters, artipacked, template-injection→env). |
| `--diff`, `--fix-rules`, `--show-fix-status` | **v1** (`--diff`) / later | Cheap given the model. |
| `actionPermissionTable` for missing-permissions fix (`action_permissions.go:164-455`) | **later** | Useful but a hand-curated, decaying table. |
| Online mode: off by default, cached, ETag, token discovery, `cache`/`strict` modes, coverage report (`online*.go`, `github_*.go`) | **later** (design v1) | Big (~7.5k lines). Design the resolver trait now (popular-actions snapshot → local → online), ship metadata fetch + SHA pin/version-comment first; impostor/repojacking/advisories after. Never wall-clock/network-dependent by default. |
| GHES + github.com fallback, GraphQL branch scan, `--suggest-timeouts`, registry digests | **skip / much later** | Low ROI per line. |
| In-process bash parse for syntax/taint/pipefail (`internal/runscript`, mvdan/sh) | **v1** (parser) / later (taint) | Rust has no mvdan equivalent; evaluate tree-sitter-bash / brush-parser / yash-syntax. A parser enables precise shell-aware template-injection fixes and `pipeline-without-pipefail`. Keep shellcheck optional. |
| github-script JS syntax via `node --check` (`rule_github_script_syntax.go`) | **v1 but in-process** | Use oxc_parser (fast, pure Rust). The node approach made runs 16–19 s here and found 0 issues. |
| Hand-written JS `child_process` sink scanner (`command_injection_javascript.go`) | **later** (on oxc AST) | Do it on a real AST or not at all. |
| `--ruff` instead of pyflakes | **v1** | Trivial and ruff is the de-facto tool; consider ruff as default if present. |
| action.yml linting with synthetic composite job + caller graph (`action_file.go`, `action_graph.go`) | **v1** | #1 upstream request (`05-issues.md` theme 1). Caller graph (trigger context for composites) is later. |
| dependabot.yml linting (`dependabot_parse.go`) | **later** | Out of actionlint scope; SchemaStore has a dependabot schema we could validate against cheaply. |
| Actions lock file (`actions_lock.go`) | **later / watch** | Targets a pre-1.0 GitHub preview format; verify the feature is real and stable before investing. |
| New syntax: parallel steps, `cache-mode`, `concurrency.queue`, `cancel-timeout-minutes`, `$/` uses, new permissions/activity types | **v1** | Parity with current GitHub is table stakes; jactionlint is the reference for these semantics (incl. corrections #124–#127). |
| Larger-runner label heuristic + third-party runner providers (`rule_runner_label.go:212`, `runner_label_providers.go`) | **v1** | Removes a real FP class (119 corpus findings). |
| Runner retirement by wall-clock date (`runner_label_retirement.go:76-81`) | **skip** (as designed) | Non-hermetic. Key retirement to the data snapshot date / release, or emit as info with date in message. |
| Event payload property checking (`event_payloads.go`) | **v1** (typo mode) | Generated from octokit/webhooks; catches real bugs. Generate typed Rust tables at build time, not a runtime DSL. |
| Vendored actions/runner schema conformance test + weekly pin bump (`schema_conformance_test.go`) | **v1** | Best idea in the repo for fighting stale data (`05-issues.md` theme 2). |
| Upstream-fixture compat test (`actionlint_compat_test.go`) | **v1** | Same oracle we need for parity; extend with position-level corpus diff (§10). |
| zizmor differential harness (`scripts/zizmor-diff`) | **later** | Useful once security rules exist; record corpus + commit for reproducibility (theirs doesn't). |
| Security rules matching zizmor (dangerous-triggers, cache-poisoning, artipacked, bot-conditions, unsound-contains, overprovisioned-secrets, obfuscation, github-env, untrusted-checkout/artifact, insecure-commands) | **v1 subset** | Precise in sample. Start with the high-precision, low-volume ones; artipacked at warn. |
| Agentic-actions table (`agent_actions.go`) | **later** | Fast-moving curated list; high upkeep. |
| Supply-chain heuristics (adhoc-packages, unverified-download, insecure-url-scheme, use-trusted-publishing, superfluous-actions, typosquat) | **later**, pedantic | Mostly advice; some messages advertise mise (`unverified-download`). |
| Policy rules (unpinned-uses with per-pattern policies, missing-permissions, missing-timeout, concurrency-limits, duplicate-triggers) | **v1 opt-in** | Users ask for them (`05-issues.md` theme 8) but they must not be default errors. |
| Output: text, json/jsonl, native SARIF, github, gcc, summary, templates; JSON Schema for output | **v1** | SARIF with fixes/suppressions/fingerprints is valuable for code scanning. |
| `--finding-context`, `--export-facts` + facts schema | **later** | Interesting for OPA users; cheap once the project model exists. |
| Distribution: release-please, signed/notarised macOS, attestations, PyPI wheels, Homebrew, Docker, root `action.yml` (pinned version), WASM playground, completions, man page | **v1**: GitHub releases + attestations, cargo-dist (Homebrew, installers), PyPI via maturin, action.yml, completions; **later**: playground, Docker, notarisation | Rust/cargo-dist gives most of this for free. |
| hk/mise integration | **later** | Easy wins for the mise/hk audience (#81). |

---

## 13. Lessons

**Copy**
1. *One registry, many derived artefacts*, each with an "is up to date" test (rule IDs, docs, config schema, output schema).
2. *Findings carry fixes or reasons for no fix*; the fix loop is plan → apply → verify (structural tree diff limited to edit ranges) → re-lint → fixpoint with cycle detection; writes are atomic and refuse concurrent modification.
3. *Baseline without line numbers* + SARIF fingerprints from the same hash; `unused-ignore` / `unused-baseline-entry` that only judge what actually ran.
4. *Conformance against GitHub's own runner schema*, vendored at a pin, every divergence justified in an allowlist, bumped weekly by automation.
5. *Parity oracle*: keep every upstream fixture passing under a `correctness` profile; diff positions on a real corpus.
6. *Offline-first online mode*: explicit opt-in, cache-only mode, strict mode, coverage report, no finding on failed lookup, never send tokens to config-chosen hosts.
7. Track new GitHub syntax within days (parallel steps, `cache-mode`, `queue`, `$/`), using jactionlint's corrections (#124–#127) as test cases.

**Avoid**
1. *Loud defaults*: 130/161 rules default to `error`, the default profile reports ~16 errors per workflow on real repos, 92 % of them hardening policy. Keep "no false positives, nothing opinionated by default" (upstream's promise) and make hardening a named profile.
2. *External processes per script* (node for github-script) — 16–19 s vs 1 s; embed parsers (oxc, a bash parser) instead.
3. *Text heuristics where a CST should answer* (ignore scope, baseline scope, fix locations, node extents) — a parser with exact spans is the main structural advantage workflint can have.
4. *Wall-clock-dependent results* and dated hand-curated tables without generators; prefer generated data with provenance, and put curated tables in data files with tests.
5. *Disabled rules still running*; gate before execution.
6. *Sprawl*: one 540-file package, process-named files (`rule_review211f_test.go`, `rule_batch_h_helpers.go`), 39 duplicated test helpers, a 10k-line checks doc as spec. Use crates per layer (parser, expr, rules, fix, online, cli), one test harness (fixture dirs with expected output, insta snapshots), and short per-rule docs generated from the registry.
7. *Breadth before precision*: ~105 rules in 10 days; follow-up issues (#99–#118) are mostly precision fixes for heuristics. Ship fewer rules, each measured on the corpus (FP sample) before entering `default`.
8. Pre-release dependencies in the core path (yaml v4 rc.6).

**Positioning implication**: jactionlint already occupies "actionlint + zizmor in one Go binary" and moves fast. workflint's edge should be speed (≤ upstream, i.e. ~10× faster than jactionlint's default run), hermetic results (no node/shellcheck needed for core checks), exact spans/CST-based fixes, LSP, a quiet default profile, and interop (read actionlint config, accept jactionlint/zizmor ignore comments, compatible rule IDs, import jactionlint baselines if cheap).

---

## Appendix A. Full rule list (generated from `--list-rules --format json`; corpus hits = default-profile run)

#### correctness (81)

| Rule | Profile | Level | Fix | Corpus hits | Summary |
|---|---|---|---|--:|---|
| `action-syntax` | correctness | error |  |  | The action metadata does not follow the syntax of action.yml. |
| `background-step-not-waited` | correctness | error |  |  | Outputs or results of a background step are read before a wait step covers it. |
| `concurrency-deadlock` | correctness | error |  |  | The concurrency group of a workflow is the same as the group of a local reusable workflow it calls, so GitHub detects a deadlock and cancels the run. |
| `conflicting-action-inputs` | correctness | error |  |  | Two inputs of a well-known action exclude each other. |
| `conflicting-runner-labels` | correctness | error |  |  | The runner labels of a job conflict with each other. |
| `constant-condition` | correctness | error |  | 15 | An if: condition is a constant expression. |
| `context-availability` | correctness | error |  |  | A context or special function is used where it is not available. |
| `cron-too-frequent` | correctness | error |  | 3 | A scheduled job runs more often than once every 5 minutes. |
| `cyclic-job-needs` | correctness | error |  |  | Jobs depend on each other in a cycle. |
| `dependabot-syntax` | correctness | error |  |  | The Dependabot configuration does not follow the syntax of dependabot.yml. |
| `deprecated-action-input` | correctness | error |  |  | A deprecated input of an action is used. |
| `deprecated-commands` | correctness | error |  | 276 | A deprecated workflow command such as ::set-output is used. |
| `duplicate-job-id` | correctness | error |  |  | A job ID is defined more than once. |
| `duplicate-job-needs` | correctness | error |  |  | A job ID is listed more than once in needs. |
| `duplicate-key` | correctness | error |  |  | A key is defined more than once in a mapping. |
| `duplicate-step-id` | correctness | error |  |  | A step ID is not unique within its job. |
| `environment-file-redirect` | correctness | error | yes |  | A script writes to GITHUB_OUTPUT or another environment file of the runner without the $, or empties it with > after it wrote to it. |
| `expression-syntax` | correctness | error |  | 1 | A ${{ }} expression has a syntax error. |
| `expression-type` | correctness | error |  | 22 | A ${{ }} expression has a type error. |
| `gh-without-token` | correctness | error |  |  | A script runs the gh command although no GH_TOKEN or GITHUB_TOKEN reaches the step, so gh stops. |
| `github-script-syntax` | correctness | error |  |  | The script of actions/github-script has a JavaScript syntax error. |
| `if-always-true` | correctness | error |  | 2 | An if: condition is always true because of the characters around ${{ }}. |
| `invalid-activity-type` | correctness | error |  | 1 | An activity type is not available for the Webhook event. |
| `invalid-cron` | correctness | error |  |  | A cron schedule has an invalid format. |
| `invalid-env-var-name` | correctness | error |  |  | An environment variable name contains characters which are not allowed. |
| `invalid-event-config` | correctness | error |  |  | An event is configured with options it does not support. |
| `invalid-event-filter` | correctness | error |  | 7 | An event filter is not available for the event or conflicts with another filter. |
| `invalid-function-call` | correctness | error |  |  | A built-in function is called with wrong arguments. |
| `invalid-glob` | correctness | error |  | 4 | A glob filter pattern is invalid. |
| `invalid-id` | correctness | error |  |  | A job or step ID does not follow the naming convention. |
| `invalid-ignore-comment` | correctness | error |  |  | An inline ignore comment is invalid. |
| `invalid-label-pattern` | correctness | error |  |  | A runner label pattern in the configuration is not a valid glob. |
| `invalid-local-action` | correctness | error |  |  | A local action cannot be loaded or its metadata is invalid. |
| `invalid-local-workflow` | correctness | error |  |  | A local reusable workflow cannot be loaded or is invalid. |
| `invalid-parallel-step` | correctness | error |  |  | A step is not allowed inside a parallel group or refers to a wrong step. |
| `invalid-permissions` | correctness | error |  |  | A permission scope or its value is invalid. |
| `invalid-shell-name` | correctness | error |  |  | A shell name is not available on the runner. |
| `invalid-timezone` | correctness | error |  |  | A timezone of a schedule is not a valid IANA timezone name. |
| `invalid-uses` | correctness | error |  |  | A uses: value does not follow the format of an action or a Docker image. |
| `invalid-workflow-call` | correctness | error |  |  | A reusable workflow call does not follow the format of a reusable workflow. |
| `invalid-workflow-call-input` | correctness | error |  |  | An input of the workflow_call event is invalid. |
| `invalid-workflow-dispatch-input` | correctness | error |  |  | An input of the workflow_dispatch event is invalid. |
| `local-action-checkout` | correctness | error |  | 1 | A local action is used before any step checks out the repository. |
| `malformed-expression-delimiters` | correctness | error | yes | 28 | Text such as ${ github.ref }, $ {{ x }} or {{ github.sha }} is meant to be an expression but is not one. With the option pedantic, so is a shell variable in the input of an action. |
| `matrix-duplicate-value` | correctness | error |  |  | A matrix has a duplicate value. |
| `matrix-invalid-exclude` | correctness | error |  |  | An exclude entry of a matrix does not match the matrix. |
| `merge-key` | correctness | error |  |  | The YAML merge key << is used, which GitHub Actions does not support. |
| `missing-action-input` | correctness | error |  |  | A required input of an action is not specified. |
| `missing-workflow-input` | correctness | error |  |  | A required input of a reusable workflow is not specified. |
| `missing-workflow-secret` | correctness | error |  |  | A required secret of a reusable workflow is not passed. |
| `multiline-plain-run` | correctness | error | yes |  | A run: is written over several lines without |, so YAML folds the lines into one command. |
| `outdated-action-runner` | correctness | error |  | 6000 | An action runs on a runtime which GitHub Actions no longer supports. |
| `pyflakes` | correctness | error |  |  | pyflakes reported an issue in a Python script. |
| `recursive-alias` | correctness | error |  |  | A YAML alias refers to itself. |
| `shellcheck` | correctness | error |  |  | shellcheck reported an issue in a shell script. |
| `undefined-function` | correctness | error |  |  | An undefined function is called in an expression. |
| `undefined-job-needs` | correctness | error |  |  | A job needs a job which does not exist. |
| `undefined-property` | correctness | error |  | 76 | An undefined variable or property is accessed in an expression. |
| `unknown-action-input` | correctness | error |  | 6 | An input which the action does not define is specified. |
| `unknown-event` | correctness | error |  |  | An unknown Webhook event is used. |
| `unknown-runner-label` | correctness | error |  | 705 | A runner label is unknown, or is one that GitHub has retired. |
| `unknown-workflow-input` | correctness | error |  |  | An input which the reusable workflow does not define is specified. |
| `unknown-workflow-secret` | correctness | error |  |  | A secret which the reusable workflow does not define is passed. |
| `unsound-ternary` | correctness | error |  |  | The a && b || c idiom has a falsy b so it always evaluates to c. |
| `unused-anchor` | correctness | error |  |  | A YAML anchor is defined but never used. |
| `workflow-call-permissions` | correctness | error |  |  | A caller job grants fewer permissions than a reusable workflow requires. |
| `workflow-input-type` | correctness | error |  |  | The type of a value passed to a reusable workflow does not match its input. |
| `workflow-run-names` | correctness | error |  |  | A workflow_run event refers to a workflow which does not exist in the repository. |
| `workflow-syntax` | correctness | error |  | 23 | The workflow does not follow the syntax of GitHub Actions workflows. |
| `yaml-syntax` | correctness | error |  |  | The file is not valid YAML. |
| `concurrency-cancels-prs` | default | error |  | 3 | A concurrency group that cancels runs is shared by all pull requests, so unrelated pull requests cancel each other. |
| `concurrency-cancels-release` | default | error | yes | 1 | cancel-in-progress can cancel a release or a deployment which is still running. |
| `gate-job-skipped-on-failure` | default | error |  |  | A job that reads the results of the jobs it needs is skipped when one of them fails. |
| `insufficient-permissions` | default | error |  |  | The explicit permissions of a job do not grant what an action or command of the job needs from the GITHUB_TOKEN. |
| `pipeline-without-pipefail` | default | error | yes | 95 | A failing command in a pipeline of a run: script is hidden because the shell does not enable pipefail. |
| `shell-syntax` | default | error |  | 1 | A shell script at "run:" has a syntax error (found without shellcheck). |
| `ubuntu-slim-limits` | default | error |  |  | A job on ubuntu-slim has a container, services or a Docker container action, which the single-CPU runner cannot run. With the option pedantic, a timeout-minutes above its limit of 15 minutes counts too. |
| `event-name-never-received` | pedantic | warn |  |  | github.event_name is compared with an event that the workflow never receives, so the comparison is always false. |
| `fail-fast-action-inputs` | pedantic | warn | yes |  | An action which passes by default when nothing was produced or found is not told to fail. |
| `macos-gnu-flags` | pedantic | warn |  |  | A command at run: has a GNU-only option and the job runs on macOS, which has the BSD command. |
| `unmatched-paths-filter` | pedantic | warn |  |  | An entry of paths or paths-ignore matches no file of the repository. |

#### security (54)

| Rule | Profile | Level | Fix | Corpus hits | Summary |
|---|---|---|---|--:|---|
| `hardcoded-container-credentials` | correctness | error |  |  | A password for a container registry is written directly in the workflow. |
| `template-injection` | correctness | error | yes | 68 | A potentially untrusted input is expanded in a script, a container option or the prompt of an AI agent. With the option pedantic, so is any other expression. |
| `actions-lock` | default | error |  |  | The Actions lock file does not match the workflows: an action is missing from it, an entry is stale or malformed. |
| `adhoc-packages` | default | error |  | 134 | A package is installed by name with npm, yarn, pnpm, bun, aube, gem or bundle add outside of a lock file. |
| `agentic-actions` | default | error |  |  | An AI agent action can be steered by outsiders, runs on code of a pull request, or has its safeguards turned off. |
| `artipacked` | default | error | yes | 1934 | actions/checkout persists the GITHUB_TOKEN credential in the git config. |
| `bot-conditions` | default | error | yes | 4 | A condition trusts a bot by github.actor, which can be spoofed. |
| `cache-poisoning` | default | error |  | 116 | A cache is restored in a release job or written by a privileged trigger. With the option pedantic, a tag that only starts checks and a branch filter that names a release branch among others count as releases too, and a cache that saves a file with credentials is reported. |
| `checkout-static-credentials` | default | error |  |  | actions/checkout is given an SSH key or a token that does not expire. |
| `dangerous-triggers` | default | error |  | 126 | A workflow uses pull_request_target, workflow_run or issue_comment. With the option pedantic, also one that cannot write and uses no secret. |
| `dependabot-cooldown` | default | error | yes |  | An update in dependabot.yml has no cooldown or a cooldown shorter than the minimum. |
| `dependabot-execution` | default | error | yes |  | An update in dependabot.yml allows insecure external code execution. |
| `excessive-permissions` | default | error |  | 1 | The GITHUB_TOKEN gets write access that is broader than needed. |
| `github-app` | default | error |  | 2 | A GitHub App token (actions/create-github-app-token, tibdex/github-app-token, peter-murray/workflow-application-token-action) is issued with more access or a longer life than needed. |
| `github-env` | default | error |  | 7 | Input that an outsider controls, or a value that is not a literal in a workflow triggered by pull_request_target or workflow_run, is written to GITHUB_ENV or GITHUB_PATH. |
| `hardcoded-registry-credentials` | default | error |  |  | A registry in dependabot.yml has a password, token or key written in plain text. |
| `insecure-commands` | default | error | yes | 1 | ACTIONS_ALLOW_UNSECURE_COMMANDS enables the deprecated set-env and add-path commands. |
| `insecure-ssh-keyscan` | default | error |  |  | ssh-keyscan output is trusted as the host key without verification. |
| `insecure-url-scheme` | default | error |  | 24 | A download uses http, ftp or git instead of https. |
| `invisible-characters` | default | error | yes |  | A file contains an invisible or bidirectional control character which hides what the text says. |
| `misfeature` | default | error |  | 67 | A misfeature of GitHub Actions is used: the pip-install input of setup-python or the cmd shell. With the option pedantic, a shell that GitHub does not document too. |
| `obfuscation` | default | error | yes | 7 | A path at uses: or an expression is written in an obfuscated way. |
| `overprovisioned-secrets` | default | error |  | 6 | An expression uses the whole secrets context. |
| `secrets-inherit` | default | error |  |  | A reusable workflow is called with secrets: inherit. |
| `superfluous-actions` | default | error |  | 65 | An action does what a tool of the runner image does as well, such as gh release create. |
| `typosquat-uses` | default | error |  |  | An action is one typo away from a popular action of another owner. |
| `unlocked-install` | default | error | yes | 4 | cargo install runs without --locked, or npm, yarn or pnpm install without freezing a lock file that the repository has. With the option pedantic, also installs without a lock file, npm, Yarn and bun installs that run the lifecycle scripts of the dependencies (and aube installs that allow every build), and installs without a release-age cooldown. |
| `unpinned-images` | default | error | yes | 194 | A container or service image is not pinned by a digest. |
| `unpinned-tools` | default | error |  |  | An action installs the newest version of its tool because no version is set or it is latest. |
| `unredacted-secrets` | default | error |  |  | A secret is parsed with fromJSON(), so the fields of it are not redacted in logs. With the option pedantic, also a value that a script cuts from a secret and prints without masking it. |
| `unsound-contains` | default | error |  | 2 | A condition uses contains() on a string literal, which also matches substrings. |
| `unsound-prefix-match` | default | error |  |  | An account, an owner or a repository is identified by a prefix, a suffix or a part of its name. |
| `untrusted-artifact` | default | error |  | 2 | A workflow_run workflow uses an artifact of the triggering run without validating it. With the option pedantic, also one that downloads every artifact of the run. |
| `untrusted-checkout` | default | error |  |  | A pull_request_target, workflow_run or issue_comment workflow checks out the code of a pull request and runs it, or turns off the protection of actions/checkout against the code of fork pull requests. It also reports a gate that holds at one moment (an environment, a label, the comment of a maintainer) in front of a branch that the author can still push to, and a label that survives a push. With the option pedantic, a checkout that actions/checkout refuses for a fork and a dispatch with the number of a pull request and no commit are reported too, and with the option any-checkout a checkout that nothing visibly runs. |
| `unverified-download` | default | error |  | 98 | A script runs what it downloads without verifying it. |
| `use-trusted-publishing` | default | error |  | 41 | A package is published with a long-lived credential although the registry supports trusted publishing. With the option pedantic, also trusted publishing in a job that builds the package or has no environment, and attestations that are switched off. |
| `argument-injection` | pedantic | warn |  |  | Free text that an outsider writes is expanded into the program text or the options of sed, awk, find, curl, wget and git clone. |
| `command-injection` | pedantic | warn |  |  | Input that an outsider controls is run as code by a script after it was moved to an environment variable: eval, sh -c, a variable as a command, a pipe into a shell, an inline program, child_process in github-script. |
| `comment-trigger-authorization` | pedantic | warn |  |  | A job of a comment-triggered workflow is started by the text of the comment and nothing checks who wrote it. |
| `output-clobbering` | pedantic | warn |  |  | Input that an outsider controls is written to GITHUB_OUTPUT as a record, so a line break in it sets the other outputs of the step. |
| `secrets-outside-env` | pedantic | warn |  |  | A job uses a secret but has no environment, or passes one to a reusable workflow. |
| `self-hosted-runner` | pedantic | info |  |  | A job runs on a self-hosted runner: the label self-hosted, a label of self-hosted-runner.labels or a runner group. |
| `self-repository` | pedantic | info | yes |  | A local action or workflow is referenced as ./path instead of $/path. |
| `workflow-secret-scope` | pedantic | warn |  |  | A secret is assigned to the workflow-level env and reaches multiple jobs. With the option job-level, also one of the env of a job that reaches its steps. |
| `action-release-cooldown` | online | warn |  |  | An action is pinned to a version released less than the cooldown ago. |
| `actions-lock-drift` | online | error |  |  | A pin of the Actions lock file no longer matches GitHub: the repository changed owner or was replaced, a version tag moved, or the commit is not in the history of the repository. |
| `archived-uses` | online | warn |  |  | An action or reusable workflow is in an archived repository. |
| `impostor-commit` | online | error |  |  | A hash-pinned action uses a commit which is not part of the repository's own history (it exists only in a fork). |
| `known-vulnerable-actions` | online | error |  |  | An action version is affected by a published GitHub security advisory. |
| `ref-confusion` | online | warn |  |  | The ref of an action is both a branch and a tag of its repository. |
| `ref-version-mismatch` | online | warn | yes |  | The version comment of a hash-pinned action does not match the pinned commit. |
| `repojacking` | online | warn |  |  | An action or reusable workflow has an owner whose name anyone can register. |
| `stale-action-refs` | online | info |  |  | A hash-pinned action uses a commit which no tag of the repository points to. |
| `unpinnable-action` | online | warn |  |  | A hash-pinned action pulls in an action or a Docker image by a tag or a branch, so its pin does not freeze what runs. Only with the option pedantic. |

#### policy (22)

| Rule | Profile | Level | Fix | Corpus hits | Summary |
|---|---|---|---|--:|---|
| `expired-ignore` | correctness | error |  |  | An entry of "ignores" in the config file has expired or is about to. |
| `unused-baseline-entry` | correctness | info |  |  | A baseline entry matches no finding any more, so the baseline can shrink. |
| `concurrency-limits` | default | error | yes | 1277 | A workflow does not cancel superseded runs with concurrency:. |
| `duplicate-triggers` | default | error |  | 285 | push and pull_request both run the workflow for the same commit. |
| `missing-permissions` | default | error | yes | 2828 | Neither the workflow nor the job sets permissions:. |
| `missing-timeout` | default | error | yes | 2586 | A job does not set timeout-minutes. |
| `unpinned-uses` | default | error | yes | 7133 | An action, reusable workflow or Docker image is not pinned to a commit SHA or digest. |
| `unused-job-output` | default | error |  | 17 | An output of a job is never read by another job or by a workflow_call output. |
| `anonymous-definition` | pedantic | warn | yes |  | A workflow has no top-level name:. |
| `continue-on-error` | pedantic | info |  |  | A job has continue-on-error: true, so its failure does not fail the workflow. |
| `dependabot-missing-actions-update` | pedantic | warn | yes |  | dependabot.yml has no github-actions update although the repository has workflows using actions, or its github-actions updates do not cover the local composite actions that use actions. |
| `deprecated-node-runtime` | pedantic | warn |  |  | A Node.js runtime that GitHub Actions removed is declared, used or selected by an environment variable. |
| `deprecated-runner-label` | pedantic | warn |  |  | A runner label is deprecated and GitHub is going to retire it on a known date, such as macos-14. |
| `missing-version-comment` | pedantic | warn | yes |  | A hash-pinned action has no comment which names its version. |
| `mutable-runner-label` | pedantic | warn | yes |  | A runner label is an alias that GitHub moves to newer images, such as ubuntu-latest. |
| `undocumented-permissions` | pedantic | info |  |  | A permission scope above read has no comment explaining it. |
| `unused-ignore` | pedantic | error | yes |  | An ignore comment or an entry of "ignores" in the config file did not suppress anything. |
| `unused-workflow-input` | pedantic | warn |  |  | An input of workflow_dispatch or workflow_call is never used. |
| `unused-workflow-secret` | pedantic | warn |  |  | A secret of workflow_call is never used. |
| `forbidden-uses` | opt-in | error |  |  | An action or reusable workflow is not allowed or is denied by the configuration. |
| `required-actions` | opt-in | error |  |  | An action listed in required-actions is not used by a workflow. |
| `timeout-too-long` | opt-in | error |  |  | timeout-minutes of a job exceeds the configured maximum. |

#### style (4)

| Rule | Profile | Level | Fix | Corpus hits | Summary |
|---|---|---|---|--:|---|
| `max-run-lines` | pedantic | error |  |  | A run: script has more lines than allowed. |
| `require-expression-wrapping` | pedantic | error |  |  | An if: condition is not wrapped in ${{ }}. |
| `require-shell` | pedantic | error |  |  | A run: step does not set the shell explicitly. |
| `unused-needs` | pedantic | info |  |  | A needs entry is neither read by the job nor needed for the order of jobs. |

