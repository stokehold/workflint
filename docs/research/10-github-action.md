# 10 — An official GitHub Action for workflint

Snapshot 2026-10-15. Sources:

- `zizmorcore/zizmor-action` at `.reference/zizmor-action`, HEAD `38c745a`, latest release v0.7.0 (2026-10-15).
- zizmor's docs and output code at `.reference/zizmor`.
- `jdx/jactionlint` at `.reference/jactionlint`.
- Upstream `rhysd/actionlint` at `.reference/actionlint`.
- Four third-party actionlint actions, read with `gh api`.
- GitHub docs and changelog, linked inline.

Raw dumps are in `.reference/research/raw/`:

- `zizmor-action-issues.json`: all 39 issues, with comments.
- `zizmor-action-prs.json`: all 134 PRs.

Upstream context: actionlint #117 ("Provide a proper GitHub action", 28 reactions, still open) and #262 (closed as not planned). rhysd's reason, from https://github.com/rhysd/actionlint/issues/117:

> "Putting an action in rhysd/actionlint repository is not possible because currently this repository is already using Git tags for releases. But an action also requires Git tags for releases. Versions of actionlint and versions of the action should be separated. So they will have conflicts. I think separate repository is necessary."

---

## 1. How zizmor-action works

### 1.1 Shape

It is a **composite action**: one bash step, plus one optional nested action (`action.yml:107-135`).

```
user job (any runner with bash + python3)
│
├─ actions/checkout                       ← the user must do this (#120 was "forgot checkout")
│
└─ zizmorcore/zizmor-action@<sha>        (composite, action.yml:107)
   │
   ├─ step "Run zizmor"  → action.sh      (action.yml:110-128; inputs passed ONLY via env GHA_ZIZMOR_*)
   │    1. validate `version` ∈ {latest, X.Y.Z, vX.Y.Z}                    action.sh:38-51
   │    2. build argv (persona, format, collect, online, severity, config)  action.sh:53-76
   │    3. lockfile = $GITHUB_ACTION_PATH/support/locks/zizmor-<v>.txt      action.sh:78-80
   │         (unknown version → die "was it released after this action?")
   │    4. pip download --only-binary --no-deps --require-hashes            action.sh:95-102
   │         → picks the wheel for this OS/arch, checks it against the
   │           sha256 set committed in the action repo
   │    5. python -m zipfile extract → chmod +x                             action.sh:109-116
   │    6. cd $GITHUB_WORKSPACE; GH_TOKEN=$token zizmor ARGS -- $INPUTS | tee $RUNNER_TEMP/zizmor
   │                                                                        action.sh:127-134
   │    7. exit 3 (no inputs) → optionally downgraded to warning            action.sh:139-144
   │
   └─ step "Upload SARIF" (if advanced-security == 'true')                  action.yml:130-135
        github/codeql-action/upload-sarif@<sha> category: zizmor
```

### 1.2 Inputs and outputs

Inputs are at `action.yml:13-105`:

| Input | Default | Notes |
|---|---|---|
| `inputs` | `.` | Whitespace-split. It is deliberately left unquoted but placed after `--`, so it can't inject flags (`action.sh:118-133`). |
| `collect` | `default` | Added in #139. |
| `online-audits` | `true` | **Opposite of the CLI default**, because every job has a token. This caused "CI and local results differ" reports #113, #119 and #55, and a README note (#121). |
| `persona`, `min-severity`, `min-confidence` | — | Passed straight through to the CLI. |
| `version` | `latest` | `latest` means "newest version this action release knows about", not newest on PyPI (`action.yml:55-62`). |
| `token` | `${{ github.token }}` | Used only for online audits (#62). |
| `advanced-security` | `true` | SARIF plus upload to code scanning. |
| `annotations` | `false` | `--format=github`. **Mutually exclusive** with `advanced-security`; the action refuses to run with both set (`action.sh:56-59`). |
| `color` | `true` | Added in #36/#37 because a user wanted console output and a problem matcher. |
| `config` | — | Path to the config file (#34, #40, #56). |
| `fail-on-no-inputs` | `true` | Maps exit 3 to a warning (#67). |

There is one output, `output-file`: the SARIF path, set only in SARIF mode (`action.yml:8-11`, added in #86/#87 for DefectDojo-style aggregation).

### 1.3 How the binary is obtained: three generations

The history is the most useful lesson here (`git log`, PR bodies):

| Period | Mechanism | Why it changed |
|---|---|---|
| v0.0.x (May 2025) | `astral-sh/setup-uv` + `action.py` running `uvx zizmor` (first commit `f9f33d6`). | Python start-up cost, an extra nested action, and awkward hashing because uv can fetch any arch wheel (#74). |
| v0.1–v0.6 (Jun 2025–Sep 2026) | `docker run ghcr.io/zizmorcore/zizmor:<v>` with the workspace mounted read-only (#19). Hash-pinned since v0.4 (#68): `support/versions` mapped version to image digest, and an hourly job refreshed it. | Needs a Docker **daemon**. `ubuntu-slim` ships the Docker client without a daemon, so the `command -v docker` guard passed and then `docker pull` failed (#168). No macOS or Windows. Corporate registry mirroring was impossible (#136). Noisy pull logs (#131). Requests to drop Docker: #74, with **13 reactions, the most of any issue**. |
| v0.7.0 (2026-10-15) | **PyPI wheel through the runner's own `python3`** (#169, from the Apache Software Foundation's Jarek Potiuk). One requirements lock per version, holding *all* wheel hashes: 11 wheels for 1.31.0 (`support/locks/zizmor-1.31.0.txt`). `pip download --require-hashes` then picks the right one. No venv, no setup-uv, no Docker. | — |

The maintainer's reasons for *not* using GitHub release binaries (#74 comments; #15 "use release binaries, verify attestations" was closed without being implemented):

- Release binaries were "best-effort".
- He did not want to own OS/arch/musl matching.
- OCI and PyPI both handle multi-arch delegation for him.

The PyPI route is a workaround for zizmor not having first-class release assets. **workflint does not have that constraint.** It controls its own release matrix (`04-cli-output-distribution.md` §8.5).

Verification model: "pin the action, and you pin the binary bytes". Hashes live in the action's git tree, so a SHA-pinned `uses:` transitively pins zizmor's wheel bytes. That answered #52 (`latest` being mutable) and #105 ("let me pin zizmor by SHA"): you don't need to, because pinning the action already does it.

Weaknesses of the PyPI route:

- It still needs Python. Self-hosted runners without it fail with "Cannot run this action without Python" (README:336-344).
- A new zizmor version is unusable until the action releases (#101 "1.24.0 is not available").
- Mirroring now depends on `PIP_INDEX_URL`, a pip detail rather than a documented input.

### 1.4 Version-sync automation

`.github/workflows/sync-zizmor-versions.yml` runs **hourly** (`:3`), guarded to the upstream repo (`:16`, #129). It does the following:

1. Runs `support/sync-zizmor-versions.py`, a PEP 723 script.
2. The script lists stable, non-yanked releases with wheels from the PyPI JSON API, retrying 5 times (`:36-48, 59-70`).
3. For every version it does `uv add --group zizmor-<v>` and `uv pip compile --universal --generate-hashes --no-build` into `support/locks/` (`:72-85`).
4. It sets `zizmor-latest` to the newest version (`:73`).
5. It opens a **draft** PR assigned to the maintainer with `peter-evans/create-pull-request` (`:36-56`).

Merge and release are **manual**. The release notes then paste zizmor's own release notes plus the action's changes (v0.7.0 release body). A separate `make pinact` bumps the SHA pins in the README (`Makefile`, `.github/pinact.yml`).

Maintenance churn: of 134 PRs, 42 are Dependabot (**22 of them just bump `github/codeql-action/upload-sarif`**, the one nested action) and 41 are "Sync zizmor versions" or "bump pins". About 62% of the PR traffic is bookkeeping.

### 1.5 How results are surfaced

| Mode | How | Fails on findings? | Limits and quirks |
|---|---|---|---|
| **Advanced Security (default)** | `--format=sarif` → `upload-sarif`, `category: zizmor`. | **No.** zizmor suppresses exit codes 11–14 in SARIF mode (`zizmor/docs/usage.md:597-605`). The README says to gate with a code-scanning merge-protection ruleset instead (README:55-62). | Needs public repo or paid GHAS (README:48-53). PR checks only show an alert if **all lines of the alert's region are inside the diff** (#43, 11 comments; GitHub confirmed it is by design; README:346-379). Requires `security-events: write`, plus `contents/actions: read` on private repos (README:310-332). GHES without `github/codeql-action` synced breaks even when `advanced-security: false`, because nested actions are resolved up front (#23). |
| **Annotations** | `--format=github` → `::error/::warning/::notice file=,line=,title=::msg` (`zizmor/crates/zizmor/src/output/github.rs:11-22,44-48`). Uses only the start line. High maps to error; Low and Medium map to warning; Informational maps to notice. | Yes (exit 11–14). | **10 errors + 10 warnings (+10 notices) per step, 50 per job**, silently truncated (README:257-265; toolkit problem-matchers doc). Annotations are not rendered in the "new PR review experience" (#36). The console output is replaced by commands, so the logs lose the human format (#36). |
| **Plain** (`advanced-security: false`) | Coloured console output. | Yes. | Not visible on the PR at all. |
| **Checks API** (open PR #172, not merged) | Creates a check run and adds annotations in batches of 50 through the REST API. | Yes. | Needs `checks: write`. **Fork and Dependabot PRs get a read-only token**; the maintainer worries about poor degradation on third-party PRs. Needs `jq` and curl ≥ 7.76. |
| **Job summary** | Not supported. PR #38 (bash-parsed summary) was rejected as too coupled to the output format and moved to zizmor#1117. | — | — |

### 1.6 Permissions, forks, online audits

- `permissions: {}` works for annotations or plain output on public repos. A private repo needs `contents: read` for checkout (README:310-332).
- Online audits use `GH_TOKEN` from `inputs.token` (`action.sh:130`). For private actions in the same org, `github.token` cannot read other repos, so `impostor-commit` fails (#55). The resolution was to explain it in the docs and track a "warn instead of fail" CLI flag in zizmor#1350.
- Forks: annotations and plain output work. GitHub docs say "code scanning always allows the uploading of results when the `pull_request` event triggers the action run" (https://docs.github.com/en/code-security/reference/code-scanning/troubleshoot-analysis-errors/resource-not-accessible), so SARIF works on fork PRs under `pull_request`. Checks API mode would not (#172).

### 1.7 OS support, releases, CI

- **OS**: `action.sh:34` still warns on non-Linux. #171 (open) proposes dropping the warning now that Docker is gone, and a commenter has Windows/macOS-only projects. CI matrix: ubuntu-latest, ubuntu-24.04-arm, ubuntu-slim, ubuntu-22.04, ubuntu-22.04-arm (`selftest.yml:22-29`). **No macOS or Windows.**
- **Tags**: v0.0.1 → v0.7.0 (23 tags), **no floating major tags**. Releases are **immutable** (`gh api …/releases/latest → immutable: true`). The README tells users to SHA-pin (README:97). When v0.4.0 shipped broken (#82), it was marked pre-release and de-listed from Marketplace, because an immutable release cannot be replaced.
- **CI** (`.github/workflows/selftest.yml`): 12 jobs. Each uses `uses: $/`, the 2026-07-30 self-repository syntax (#148). Cases:
  - pass and xfail (`selftest.yml:55-74, 85-103`)
  - version variants including `v` prefix and `9999.0.0` xfail (`:40-74`)
  - mutual-exclusion xfail (`:136-155`)
  - exit 3 handling (`:157-187`)
  - output presence (`:189-206`)
  - an `alls-green` aggregator (`:223-247`)

  It audits `woodruffw/gha-hazmat` as a known-bad remote target.
- **Bug class to avoid**: #82/#73. `action.sh` computed `dirname $GITHUB_ACTION_PATH` to find `support/versions`. That worked for the local `./` self-test but not for the real `_actions/<owner>/<repo>/<ref>/` layout. **Test through a remote-style reference** (`$/`) and never navigate above `GITHUB_ACTION_PATH`.
- Issue templates are copy-pasted from zizmor-vscode (they ask for VS Code extensions). Minor, but it shows a one-maintainer repo.

### 1.8 zizmor-action pain points (from 39 issues and 134 PRs)

| Theme | Issues/PRs | Lesson for workflint |
|---|---|---|
| Docker dependency (ubuntu-slim, macOS/Windows, registry mirrors, log noise) | #74 (13 reactions), #168, #169, #136, #131, #171, #8 | Don't ship a Docker or container action. |
| Tool version lag and pinning semantics | #52, #101, #105, #81, #104 | Pin bytes in the action tree, automate the bump end to end, and offer a verified path for versions newer than the action. |
| Action and CLI defaults differ (online mode) | #113, #119, #55, #121 | Keep the action's defaults identical to the CLI's, or print the effective mode loudly at the start. |
| Annotation limits and visibility | #36, #89, #117, README:257 | The binary should prioritise and cap annotations and always write a job summary; Checks API as an option. |
| SARIF "all lines in diff" quirk | #43, README:346-379 | Emit **tight primary regions** in SARIF (single line), with context in `relatedLocations`. |
| Mutually exclusive output modes | `action.sh:56-59`, #36 | One analysis, several sinks: console + annotations + summary + SARIF together. |
| "Only lint changed files" | #89 | Add a `changed-only` input. |
| GHES/Forgejo | #23, #124 | Avoid nested `uses:` inside the action. Forgejo: document best-effort support. |
| Flag creep ("every CLI flag becomes an input") | #141, #143/#144, maintainer comment on #141 | Keep a small curated input set plus an escape-hatch `args`. Inline config only via file or `vars` (#143). |
| Path-layout bug only seen remotely | #82, #73 | Test through `$/` and stay inside `GITHUB_ACTION_PATH`. |

---

## 2. How actionlint is run in Actions today

### 2.1 Upstream's own guidance (no official action)

- `docs/usage.md:193-254` describes the documented setups:
  - `bash <(curl …/main/scripts/download-actionlint.bash)`, which fetches the script from `main`, then `${{ steps.x.outputs.executable }} -color`.
  - Or `uses: docker://rhysd/actionlint:latest` with a monolithic `args:`.
- `docs/usage.md:310-326`: reviewdog. `docs/usage.md:329-348`: problem matcher, where the user copies `.github/actionlint-matcher.json` into their repo and runs `echo "::add-matcher::…"`.
- `scripts/download-actionlint.bash:113-131`: picks the asset with `$OSTYPE` and `uname -m` and pipes `curl` straight into `tar`. **No checksum or attestation verification**, even though releases ship `checksums.txt` and build-provenance attestations (`04-cli-output-distribution.md:379-384`).
- `.github/actionlint-matcher.json:7`: one regex, ANSI-tolerant, giving file/line/col/message/code.

### 2.2 Comparison table

Stars and dates are from `gh repo view`, 2026-10-15. "Imm." means the latest release is immutable.

| | **zizmor-action** | **reviewdog/action-actionlint** | **raven-actions/actionlint** | **devops-actions/actionlint** | **kjanat/actionlint** (fork, root action) | **jdx/jactionlint** (fork, root action) |
|---|---|---|---|---|---|---|
| Stars | 167 | 102 | 52 | 10 | 47 (whole repo) | 242 (whole repo) |
| Latest | v0.7.0 (2026-10-15) | v1.80.2 (2026-10-14) | v2.2.0 (2026-06-27) | v0.1.13 (2026-08-21) | v1.17.0 (2026-09-13) | v2.2.0 (2026-10-14) |
| Action type | composite (bash) | **Docker** (`image: Dockerfile`, **built on every run**) + `/dockerless` composite variant | composite of 4× `actions/github-script` + `actions/cache` + runtime `npm install @actions/tool-cache` | composite running `docker run` | **node24 JS** (`runs: {using: node24, main: action.mjs}`) | composite (bash) |
| Separate repo? | yes | yes | yes | yes | **no**, shared tags | **no**, shared tags (`version` default bumped by release-please, `action.yml:14`) |
| Binary source | PyPI wheel via `pip download` | the curl script from **kjanat's fork** pinned to a commit (switched from rhysd 2026-09-15) | GitHub release asset; `latest` resolved at runtime via redirect/API | Docker Hub `rhysd/actionlint@sha256:…` (`settings.json`) | GitHub release asset | GitHub release asset (`scripts/download-jactionlint.bash`) |
| Verification | sha256 of every wheel committed in the action | none | none | image digest | **SHA-256 against `SHA256SUMS` committed at the tag**; signed annotated tags; the major-tag mover checks the signature (`major-tag.yml`) | **none** (`curl -fL` + `tar`), although releases have checksums + `attest-build-provenance` |
| Default tool version | newest known to the action | hardcoded in script, bot-bumped (`depup`) | `latest` at runtime (non-reproducible) | hardcoded digest | = action release | = action release |
| OS | Linux (warns otherwise) | Docker variant Linux-only; dockerless any | Linux/macOS/Windows/FreeBSD | Linux + Docker daemon only | Linux/macOS/Windows x64+arm64 | Linux/macOS/Windows (CI matrix `action.yaml:28-29`) |
| Annotations | `--format=github` (opt-in, exclusive) | reviewdog: `github-pr-check`/`github-pr-review`/`github-check` | problem matcher fetched from rhysd raw at tag | bundled problem matcher | native `github` format with source excerpt; `annotations: auto` | `--format=github` (default on) |
| SARIF | default, upload in-action | — | — | — | file only (`sarif: true` → `report-sarif`) | opt-in; upload in-action, **then a separate fail step** so findings are uploaded *and* the job fails (`action.yml:123-139`) |
| Job summary | — | — | — | — | **yes, default on**, with excerpts and links | — |
| PR review comments | — | **yes** (core feature) | — | — | opt-in `review: true` (`pull-requests: write`, advisory) | — |
| Fail semantics | fails except in SARIF mode | `fail_level` (default `none`, so it never fails!) | `fail-on-error: true` | `fail-on-errors: true` | fails on findings; invalid options/fatal always fail | fails on errors; `strict-exit` for warnings |
| Extra tools | — | installs shellcheck + pyflakes | apt/brew/choco shellcheck, pipx pyflakes | inside image | verified standalone shellcheck + ruff | built-in shell checks |
| Outputs | `output-file` | — | version, exit-code, total-errors/files, cache-hit | `results-file` (JSON) | version, exit-code, result, problem-count, output, result-file, report-sarif | executable, sarif-file |
| Floating major tag | none | `v1` + every `v1.N` | `v1`, `v2` (not immutable) | none | `v1`, `v1.N` (moved by a verified workflow) | none |

### 2.3 Pain points in the actionlint actions

From their issue trackers, read with `gh issue list`.

- **reviewdog/action-actionlint**:
  - #173 "Always fails regardless of settings" (open).
  - #124/#104: fails with "fail to parse diff" on PRs touching more than 300 files.
  - #72: Check-run text over 65,535 chars.
  - #169: pre-built images no longer used, which breaks ARC Kubernetes mode.
  - #129: shellcheck directives ignored.
  - #76: how to add runner labels (config discoverability).
  - #95: resolving stale review conversations.

  Permissions are heavy: `checks`, `issues` and `pull-requests: write` (README). A Dockerfile action rebuilds its image (python + shellcheck + pyflakes + actionlint + reviewdog) on every job.
- **raven-actions/actionlint**:
  - #26 (10 reactions): installing actions-cache broke repos not using npm.
  - #56: a floating `@actions/tool-cache@latest` broke existing versions.
  - #60: stray `node_modules` left in the workspace.
  - #51: ESM export error.
  - #71: flag parser splits quoted `-ignore` patterns.
  - #11: "move to JS action".

  **Installing npm packages at action runtime is the anti-pattern here.** `version: latest` resolved at runtime also makes builds non-reproducible.
- **devops-actions/actionlint**: Docker only. #40: `SHELLCHECK_OPTS` not passed into the container. #63: config ignored (the mount and workdir mismatch). It runs actionlint twice on failure, once for JSON and once for the console.
- **Upstream #262** (8 reactions, closed as not planned): the download script means running a script from `main` or hand-bumping a version Dependabot can't see. Docker gives one monolithic `args` string with no typed inputs. The comments asked for discoverability and Dependabot-trackable pins, and suggested a separate repo.

Common thread: **every actionlint action re-implements download + platform detection + output plumbing, and none of the third-party ones verify what they download.** reviewdog has quietly moved to kjanat's fork. A first-party action with verified, reproducible installs and native output modes fills a real gap.

---

## 3. Best practices for a binary-downloading action (2026)

### 3.1 Action type

| | Composite (bash) | JavaScript (node24) | Docker container |
|---|---|---|---|
| OS | All hosted runners: Windows has Git Bash, so `shell: bash` works everywhere. Not bare Alpine containers without bash. | All. The runner bundles Node. Node 20 was **removed 2026-09-23**, so `node24` is required (https://github.blog/changelog/2026-09-23-node-20-is-no-longer-available-in-github-actions). | **Linux only.** Needs a Docker daemon, which `ubuntu-slim` lacks (#168). |
| Start-up | Fastest: no build, only tarball download (zizmor-action ≈ 44 KB). | Fast. Needs a bundled `dist/` committed or built at release (ncc/esbuild); kjanat builds `action.mjs` at release. | Slow. `image: Dockerfile` builds on every job; pre-built images still pull at job set-up for *every* job, even if the step is skipped. |
| Maintenance | Shell portability (macOS bash 3.2, Windows paths). No dependency tree. | npm dependency churn (Dependabot), bundling, Node runtime migrations (node16 → 20 → 24 forced re-releases; raven #21). | Base-image CVEs, digest bumps. |
| Rich API use (Checks API, PR reviews, tool-cache) | curl + jq (zizmor #172 needs curl ≥ 7.76). | Native `@actions/*` toolkit. | Whatever is in the image. |
| Post-step / cleanup | No `post:`. | `post:` supported. | `post-entrypoint`. |

**Conclusion:** a composite action is right when the *binary itself* does the heavy lifting: annotations, summary and SARIF are formatted by workflint and not by the action. A JS action only earns its keep if the action must talk to GitHub APIs (Checks, reviews). That logic can also live in the Rust binary (`workflint github report …`), where it is testable and reusable outside Actions.

### 3.2 Download verification

Stack the following, cheapest first:

1. **Embedded checksums in the action tree.** This is the zizmor-action and kjanat model: a SHA-pinned `uses:` pins the binary bytes. It works offline from GitHub's API and is immune to a compromised release.
2. **Immutable releases** for workflint itself. These have been GA since 2025-10-28 (https://github.blog/changelog/2025-10-28-immutable-releases-are-now-generally-available/). Tags and assets cannot change, and GitHub auto-generates a **release attestation** that `gh release verify` / `gh release verify-asset` (gh ≥ 2.81) can check. Draft first, upload assets, then publish (jactionlint `.goreleaser.yaml:4-11` does exactly this).
3. **Build-provenance attestations** (`actions/attest-build-provenance` over `checksums.txt`, as upstream and jactionlint do). Verify with `gh attestation verify <file> --repo workflint/workflint --signer-workflow workflint/workflint/.github/workflows/release.yml`. `gh` is preinstalled on hosted runners and needs a token. This step proves *which workflow built the bytes*, which step 2 does not.
4. Fail closed. Never fall back to unverified downloads or to `latest`.

### 3.3 Caching

`@actions/tool-cache` writes to `RUNNER_TOOL_CACHE`, which only persists on **self-hosted** runners. On hosted runners it is per-job. A 2–5 MB static binary from GitHub's CDN downloads in well under a second. An `actions/cache` round trip is usually slower and costs cache quota (raven #26, #56). So:

- **Don't use `actions/cache`.**
- Do reuse `$RUNNER_TOOL_CACHE/workflint/<ver>/<arch>/` when it exists (self-hosted).
- Do skip downloading when `executable:` is given.

### 3.4 Immutable actions vs immutable releases vs lockfiles

- **Immutable actions** (publishing actions as OCI packages to GHCR) are **still not usable**. `actions/publish-immutable-action` README: "not ready for public use"; last push 2025-08. Don't plan around them.
- **Immutable releases** *are* usable, and are what zizmor-action, kjanat, jactionlint and reviewdog already use. Floating major tags (`v1`) are just git tags without releases, so they stay movable.
- **Workflow dependency lockfiles** (`.github/workflows/actions.lock` written by `gh actions-lock`, **public preview**: https://docs.github.com/en/actions/concepts/security/workflow-dependency-lockfiles):
  - They lock action refs, including **transitive** action dependencies, to commit SHAs plus owner/repo IDs.
  - They say nothing about binaries an action downloads at runtime.

  Two consequences for us:

  - Users who lock `workflint-action@v1` get a fixed commit, so embedded checksums give them a fully locked chain.
  - Nested `uses:` inside our action become lockfile entries in every consumer's lockfile. Fewer nested actions means less churn. jactionlint already lints the lockfile (`jactionlint/docs/checks.md:4194-4233`); workflint should too.

### 3.5 Same repo or separate repo?

| | Same repo as the tool (kjanat, jactionlint) | Separate repo (zizmor-action, every third-party action) |
|---|---|---|
| Tags | Shared. Every action fix needs a tool release and vice versa. rhysd rejected this (#117). | Independent semver. Action fixes ship without a tool release (#82 → v0.4.1 the next day). |
| Download size | The runner downloads the **whole repo tarball** at the ref every job: jactionlint ≈ 3.8 MB gz, kjanat repo 65 MB with history, workflint with its corpus likely more. | ≈ 50 KB. |
| Marketplace | One action per repo at root. The repo should contain "only the metadata file, code, and files necessary" (https://docs.github.com/en/actions/how-tos/create-and-publish-actions/publish-in-github-marketplace). | Clean fit. |
| Dependabot UX | One bump updates both. | Same, **if** the action's default `version` tracks the bundled release (zizmor-action model). |
| Access to fixtures/testdata in CI | Easy. | Needs a fixtures dir or cross-repo checkout. |

**Pick a separate repo.** Naming: `workflint/workflint-action` follows the ecosystem convention (`zizmorcore/zizmor-action`, `golangci/golangci-lint-action`, `astral-sh/ruff-action`) and reads unambiguously in Dependabot PR titles and forks. `workflint/action` is shorter but generic in search and fork lists.

### 3.6 Keeping it updated

- **Push, then poll.** At the end of workflint's release workflow, after the immutable release is published, send a `repository_dispatch` with a GitHub App token to the action repo. Also run an hourly cron fallback, which is what zizmor-action uses, but on GitHub releases instead of PyPI.
- **The bot PR should be auto-mergeable.** zizmor-action's is a draft and manual:
  - verify attestation + checksums
  - write `versions/<ver>.json`
  - bump `default-version`
  - run the full self-test matrix with the new version
  - auto-merge when green
  - **release automatically** with release-please or a tag workflow, pasting workflint's release notes (as zizmor-action does)
- **Downstream**:
  - Users' Dependabot (`github-actions` ecosystem) bumps `workflint-action@<sha> # vX.Y.Z`. Because the default tool version equals the bundled one, this moves the linter too. That is the property #262 asked for.
  - Users pinning `version:` explicitly can use a Renovate regex manager. Ship a preset `github>workflint/workflint-action//renovate` that matches `version: "X.Y.Z"` in `uses: workflint/workflint-action` blocks.
  - Inside the action repo, Dependabot weekly with cooldown (zizmor-action `dependabot.yml`) for its own CI only.

---

## 4. Recommended design: `workflint/workflint-action`

### 4.1 Principles

1. **Thin wrapper; the binary owns presentation.** All formats (GitHub annotations, job-summary Markdown, SARIF, JSON, rdjson) are implemented and tested in Rust. The action only installs, verifies, maps inputs to flags and sets outputs. This lesson comes from zizmor PR #38 (rejected summary parsing in bash) and from raven and devops (JSON parsing in shell and JS).
2. **One analysis, many sinks.** The CLI gets a repeatable `--output <kind>[=<path>]` (or `--report`) so a single run writes console + annotations + summary + SARIF at once. This avoids zizmor's annotations-vs-SARIF exclusivity (`action.sh:56-59`) and devops-actions' run-twice approach.
3. **Defaults equal the CLI's.** For online features, the action's default is the CLI's default. When online mode is enabled, the binary prints `online checks: enabled (token from GITHUB_TOKEN)` as its first log line. This lesson comes from #113, #119 and #55.
4. **No nested `uses:` in the action.** This means:
   - nothing GHES has to mirror (#23)
   - no Dependabot churn (22 of zizmor-action's PRs)
   - nothing extra in consumers' lockfiles

   SARIF upload is a documented second step (see 4.4).
5. **Fail closed on verification; fail open (with a warning) on reporting extras.** If Checks, review or upload are not permitted (forks, Dependabot), degrade with a `::warning::` and keep the lint verdict intact (kjanat "review failure is advisory").

### 4.2 Layout

```
workflint/workflint-action
├── action.yml                 composite, single `shell: bash` step (+ no nested uses)
├── action.sh                  install → verify → run; inputs only via env (WORKFLINT_ACTION_*)
├── versions/
│   ├── 0.4.2.json             { "x86_64-unknown-linux-musl": "sha256…", "aarch64-…": …, "x86_64-pc-windows-msvc": … }
│   └── default                "0.4.2"   (the version this action release bundles)
├── support/sync-versions.*    bot: fetch release, gh release verify + gh attestation verify, write versions/*.json
├── fixtures/                  clean/ and findings/ workflow trees for self-tests
└── .github/workflows/{selftest,sync-versions,release,lint}.yml
```

### 4.3 Inputs

| Input | Default | Maps to |
|---|---|---|
| `version` | `""`, which uses `versions/default` (= the action release) | Exact `X.Y.Z` / `vX.Y.Z`. A version present in `versions/` is checked against the embedded sha256. A **newer, unknown** version is allowed only through attestation verification (see `verify`), which fixes zizmor #101's lag. `latest` is **not** offered: it is non-reproducible (raven, zizmor #52). |
| `executable` | `""` | Use a preinstalled binary (mise, self-hosted, CI of workflint itself) and skip the download (jactionlint `action.yml:15-20`). |
| `files` | `""`, meaning auto-discover: workflows, local actions, dependabot, `actions.lock` | Newline/whitespace list, passed after `--`, never globbed or eval'd (`set -f; read -ra`, jactionlint `scripts/action.bash:10-14`). |
| `working-directory` | `.` | `cd` before running. |
| `config` | `""`, meaning the CLI's own discovery | `--config-file`. No inline YAML input (#143 debate). Document `config: ${{ vars.X }}`-to-file recipe instead. |
| `changed-only` | `false` | On `pull_request`, pass `--changed-since ${{ github.event.pull_request.base.sha }}`. Needs `fetch-depth: 0` or a fetch of base, documented (zizmor #89). |
| `min-severity` | `""` | `--min-severity`. |
| `fail-on` | `error` | `error` \| `warning` \| `any` \| `never`. It maps to exit status and is independent of output sinks. |
| `annotations` | `true` | Sink `github`. The binary sorts by severity and emits at most 10 errors, 10 warnings and 10 notices, then `::notice::N more findings, see job summary`. It escapes `%`, `\r`, `\n`, `:` and `,` correctly; upstream's template doesn't (`04-cli-output-distribution.md:223`). |
| `summary` | `true` | Sink `summary=$GITHUB_STEP_SUMMARY`: a table of counts per rule and file, then findings with snippets and permalinks. It keeps within 1 MiB per step (https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-commands) by truncating with a count. |
| `sarif` | `false` | Sink `sarif=$RUNNER_TEMP/workflint.sarif`, exposed as output `sarif-file`. |
| `checks` | `false` (v1.1) | Binary subcommand posts one check run named `workflint` with all annotations in batches of 50. Needs `checks: write`. On fork or Dependabot PRs it warns and skips. |
| `online` | `""`, inheriting CLI/config | `true`/`false`/`cache`. The token is passed **only** when online is not `false` (jactionlint `scripts/action.bash:22-34`). |
| `token` | `${{ github.token }}` | Env var for online checks and `checks` mode only. Never echoed. |
| `verify` | `auto` | `auto`: embedded checksum, and if the version is unknown, `gh attestation verify --signer-workflow …` + `gh release verify-asset`. `attestation`: always do both. `checksum`: embedded only, and fail if unknown. |
| `color` | `true` | Console colour (#36). |
| `args` | `""` | Escape hatch for any CLI flag, so the input list stays small (#141). |

### 4.4 Outputs and the SARIF recipe

Outputs:

- `executable`, `version`
- `exit-code`
- `error-count`, `warning-count`
- `sarif-file`
- `result-file`, which is always a JSON report (kjanat's `result-file` idea) for DefectDojo-style aggregation (zizmor #86)

SARIF upload is a second step owned by the user. This keeps the main action free of nested `uses:` and lets users choose `category` and whether to upload on forks:

```yaml
permissions: { contents: read, security-events: write }
steps:
  - uses: actions/checkout@<sha> # v7
    with: { persist-credentials: false }
  - id: lint
    uses: workflint/workflint-action@<sha> # v1.0.0
    with: { sarif: true, fail-on: never }
  - uses: github/codeql-action/upload-sarif@<sha> # v4
    if: ${{ !cancelled() && steps.lint.outputs.sarif-file != '' }}
    with: { sarif_file: "${{ steps.lint.outputs.sarif-file }}", category: workflint }
```

If a one-step SARIF experience is wanted later, add it as a **sub-action** `workflint/workflint-action/sarif@v1` so that the root action stays dependency-free.

**SARIF content requirements for the CLI.** These come from #43 and from upstream's template:

- tight `region` on the primary location (start line/col, and end only on the same line)
- context in `relatedLocations`
- `partialFingerprints`, so alerts track across runs
- `columnKind: unicodeCodePoints` or real UTF-16 columns
- the full `rules[]` with `helpUri`, regardless of which rules ran
- `automationDetails.id`
- exit code governed by `--fail-on`, not by the format

That last point fixes zizmor's "SARIF mode never fails" surprise. Users who want zizmor's semantics set `fail-on: never` and gate with a code-scanning ruleset. jactionlint shows the other pattern: upload then fail (`action.yml:123-139`). Our two-step recipe gets that for free with `if: !cancelled()`.

### 4.5 Output modes summary

| Mode | Default | Permission | Fork PR (`pull_request`) | Limits |
|---|---|---|---|---|
| Console (human, colour) | on | — | ✔ | — |
| Annotations | on | — | ✔ | 10/10/10 per step, 50 per job; sorted and capped with an overflow notice |
| Job summary | on | — | ✔ | 1 MiB per step; 20 summaries per job |
| JSON result file | always | — | ✔ | — |
| SARIF file + user upload | off | `security-events: write` (+ `actions: read`, `contents: read` on private) | ✔ upload allowed on `pull_request` | public repo or GHAS/Code Security; alert shown on PR only if the region is fully in the diff |
| Checks API (v1.1) | off | `checks: write` | ✘ read-only token, so warn and skip | 50 annotations per request; text ≤ 65,535 chars (reviewdog #72) |
| PR review comments | **not in v1** | `pull-requests: write` | ✘ | Recommend reviewdog with `--format rdjson` in docs instead; revisit as `review: true` later |

### 4.6 Install and verify flow (`action.sh`)

```
target = map(RUNNER_OS, RUNNER_ARCH) → {linux: *-unknown-linux-musl, macOS: *-apple-darwin, Windows: *-pc-windows-msvc}
ver    = inputs.version || cat versions/default ; strip leading v ; validate ^\d+\.\d+\.\d+$
if executable given → use it, run `--version`, done
cache  = $RUNNER_TOOL_CACHE/workflint/$ver/$target  → reuse if present and sha matches
url    = https://github.com/workflint/workflint/releases/download/v$ver/workflint_${ver}_${os}_${arch}.(tar.gz|zip)
curl -fsSL --retry 3 -o $tmp/archive "$url"
if versions/$ver.json exists: sha256(archive) == json[target] else if verify != checksum:
      gh release verify-asset v$ver $tmp/archive -R workflint/workflint
      gh attestation verify $tmp/archive -R workflint/workflint --signer-workflow workflint/workflint/.github/workflows/release.yml
   else die "unknown version $ver; upgrade the action or set verify: auto"
extract; echo "$dir" >> $GITHUB_PATH; outputs executable/version
run: workflint --output console --output github --output summary=$GITHUB_STEP_SUMMARY \
              [--output sarif=…] --output json=$RUNNER_TEMP/workflint.json --fail-on=$FAIL_ON $ARGS -- $FILES
```

Rules:

- Never read above `GITHUB_ACTION_PATH` (#82).
- No `curl | bash`.
- Use `sha256sum` on Linux, `shasum -a 256` on macOS and `certutil`/`sha256sum` from Git Bash on Windows, wrapped in a helper.
- Print the version and verification method in a collapsed `::group::` (#131).

### 4.7 Permissions (README table)

| Use | `permissions:` |
|---|---|
| Default (annotations + summary), public repo | `{}` |
| Default, private repo | `contents: read` |
| + online checks of other private repos in the org | a GitHub App token or PAT through `token:` (`github.token` can't read sibling private repos; zizmor #55) |
| + SARIF upload | `security-events: write` (+ `actions: read`, `contents: read` on private) |
| + `checks: true` | `checks: write` |

Docs must say: never run on `pull_request_target` with the PR head checked out. workflint's own rules flag this.

### 4.8 Versioning and release automation

- Action semver independent of the tool. `v1.MINOR.PATCH`:
  - MINOR when the bundled workflint minor changes, because new rules mean new findings.
  - PATCH for a workflint patch or an action-only fix.
  - MAJOR for input or default changes.
- **Immutable releases on.**
- A movable major tag `v1` (no release attached), moved by a workflow that checks the tag signature and that the target is the newest `v1.*.*` release (kjanat `major-tag.yml`).
- The README pins `@<sha> # v1.2.3` (the convention workflint and zizmor enforce). `@v1` is documented as the convenience form.
- A broken release cannot be replaced, so the runbook is: mark it pre-release, de-list it from Marketplace, ship the next patch (zizmor #82).
- Pipeline:

  ```
  workflint release (immutable, attested)
     └─ repository_dispatch ─▶ workflint-action: sync-versions.yml (also hourly cron)
            verify (gh release verify + gh attestation verify) → versions/<v>.json, versions/default
            PR → selftest matrix with new version → auto-merge (GitHub App) → release-please tag v1.x.y
            → release notes = action changes + link/paste of workflint notes → move v1
  ```

### 4.9 Test matrix (`selftest.yml`)

Every case uses `uses: $/` (remote-like layout), with an `alls-green` aggregator.

| Axis | Cases |
|---|---|
| Runners | `ubuntu-latest`, `ubuntu-24.04-arm`, `ubuntu-slim` (no Docker, 1 vCPU), `ubuntu-22.04`, `macos-latest` (arm64), `macos-15-intel` (x86_64, while it exists), `windows-latest`, `windows-11-arm`, plus a `container: alpine` job, which must fail with a clear "bash required" message or pass once bash is installed. |
| Install | Bundled default; explicit `X.Y.Z` and `vX.Y.Z`; an older listed version; an unknown-but-real version through attestation (`verify: auto`); bogus `9999.0.0` (xfail); tampered-checksum fixture (xfail); `executable:` from a locally built binary; warm `RUNNER_TOOL_CACHE`. |
| Verdict | Clean fixture passes. Findings fixture fails with `fail-on: error`, and passes with `fail-on: never` but sets `error-count > 0`. `exit-code` values for invalid args and fatal errors. |
| Sinks | Annotations: assert the count and truncation notice through `GET /commits/{sha}/check-runs` → `annotations_count` (zizmor PR #172 technique). Summary file non-empty and under 1 MiB. SARIF validates against the 2.1.0 schema and has tight regions. JSON result-file matches its schema. All sinks together in one run. |
| Inputs | `files` with spaces, globs and `;` stay literal (injection test); `working-directory`; `config`; `changed-only` on a PR; `args`; `online: false` doesn't export the token. |
| Forks / tokens | `checks: true` with a read-only token warns and skips without failing; `permissions: {}` default mode passes. |
| Self-lint | workflint (and zizmor) on the action repo itself. `shellcheck` on `action.sh`. |

A contract test in the **workflint** repo: on each workflint PR, run the action's `main` with the freshly built binary through `executable:` (jactionlint `action.yaml` pattern). Output-format regressions are then caught before release.

### 4.10 Explicitly out of scope for v1

- Docker or container action.
- `latest` resolved at runtime.
- `actions/cache`.
- Installing npm or pip packages at runtime.
- Inline-YAML config input.
- PR review comments (point to reviewdog + `--format rdjson`).
- Forgejo or GHES guarantees: best effort. No nested `uses:` keeps GHES viable.
