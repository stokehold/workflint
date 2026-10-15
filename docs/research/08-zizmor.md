# 08 — zizmor deep dive: fork, depend, borrow, or ignore?

Research date: **2026-10-15**. zizmor clone at `.reference/zizmor`, HEAD `5eb37696` (v1.31.0, 2026-10-15). It was read-only: the build went to a `CARGO_TARGET_DIR` in a scratch directory and `git status` stayed clean.

Measurements were taken on an Apple M1 Pro (8 cores) with rustc 1.97.1.

The scratch code is not committed:
- `zz-probe`: an edge-case probe linking `yamlpath`, `github-actions-models`, and `github-actions-expressions` by path.
- `zz-prof.sh`: per-audit timing.

Raw issue dumps are in `research/raw/`:
- `zizmor-open-issues.json`: 138 open issues, with comments.
- `zizmor-open-prs.json`: 25 open PRs.
- `zizmor-actionlint-mentions.json`.
- `zizmor-issue-{931,2382,2029,1571}.json`: the scope-defining threads.

This report extends the short zizmor section in `06-rust-ecosystem.md` §2. It does not repeat that section's distribution notes (Homebrew, PyPI/maturin, Docker, zizmor-action).

---

## TL;DR

**Verdict: C (borrow architecture and patterns), plus two narrow uses of the crates as dev-dependencies or data. Do not fork (A). Do not put any zizmor crate in workflint's production dependency graph (B), at least not for v1.**

The reasons, each backed by evidence below:

1. **zizmor's data model is built to drop exactly what actionlint reports.**
   - The models are lenient serde structs with no spans. Unknown keys such as `runs_on`, `stepz`, and `names` are silently accepted. There is no `deny_unknown_fields` in `github-actions-models`.
   - A wrong type or a duplicate key fails the *whole file*, with an opaque message like `data did not match any variant of untagged enum Job at line 3 column 3`.
   - When the serde parse fails, zizmor re-parses and runs a JSON Schema check. The resulting errors are JSON-pointer paths with no line or column, full of `oneOf` noise (§3.6).
   - actionlint, by contrast, reports every bad key at its exact `line:col` and keeps going.
2. **zizmor finds positions *after the fact*.**
   - It re-parses each file with tree-sitter-yaml and walks a "route" (`jobs → build → steps → 3 → run`) to a CST node.
   - Positions inside strings are recovered with a self-described "fuzzy anchor" text search plus a whitespace regex. See `crates/subfeature/src/lib.rs:63` and `:132`.
   - This is the opposite of what a correctness linter needs: one parse into a span-carrying AST.
3. **Robustness.**
   - One file that libyaml (and GitHub) accept but tree-sitter rejects aborts the entire run with `fatal: no audit was performed`. In our corpus, `apache_airflow ci.yml` killed all 1503 files (§3.1).
   - There is no error recovery. Any tree-sitter `ERROR` node anywhere means the whole document is refused. The reported position is often `line 1, column 1`.
4. **Performance.** zizmor took **2.82 s** offline on the corpus, against **0.27 s** for actionlint. That makes it about **10× slower**.
   - It is effectively single-threaded.
   - It uses about 205 MB RSS, against 44 MB for actionlint.
   - Loading alone (all audits disabled) takes 0.56 s, which is already twice actionlint's whole run.
5. **Scope is explicitly disjoint.**
   - The maintainer repeatedly says general correctness is a non-goal and sends users to actionlint (#931, #2382, #1571, #2029, #1225).
   - A fork would mean removing about 80% of the code (41 security audits, personas, the online GitHub layer) and replacing its foundation (the YAML and model layers).
6. **What *is* worth taking:**
   - As code:
     - `github-actions-expressions` and `github-actions-models` as **dev-dependency differential oracles**.
     - Its KAT harness, which syncs `actions/languageservices` expression tests.
   - As MIT-licensed data, with attribution:
     - `context-capabilities.csv`: 4,608 webhook-derived contexts classified fixed/structured/arbitrary.
     - `codeql-injection-sinks.json`.
   - As design patterns:
     - the Finding/Location/Fix model;
     - `# tool: ignore[rule]` comments scoped to a finding's span;
     - safe/unsafe fix dispositions;
     - offline-by-default with online audits that skip themselves;
     - git smart-protocol `ls-refs` instead of REST for ref lookups;
     - a two-tier HTTP cache.
7. **Licensing is not an obstacle for any option.** Everything is MIT, and the dependency graph is permissive. A fork or vendored code only needs to keep `Copyright (c) 2024 William Woodruff`.

---

## 1. Architecture

### 1.1 Workspace layout

`Cargo.toml` declares the workspace: edition 2024, MSRV 1.97, `unsafe_code = "forbid"`, a strict clippy set, `lto = true`. Every crate is versioned in lockstep at **1.31.0**, MIT.

| Crate | LoC (src + tests) | Purpose | Deps of note | crates.io (downloads total / 90-day) | Reverse deps |
|---|---|---|---|---|---|
| `zizmor` | 38.7k (25.5k src) | CLI, 41 audits, registry, output, LSP, GitHub client | tokio, reqwest(+middleware, http-cache), tree-sitter-{yaml,bash,powershell}, jsonschema, annotate-snippets, tower-lsp-server, jemalloc, fst | 258k / 47k | 0 |
| `github-actions-models` | 3.4k | serde models: workflow, action, dependabot | `yaml_serde` (libyaml-rs) | 413k / 113k | zizmor only |
| `github-actions-expressions` | 5.5k (2.7k lib.rs) | `${{ }}` lexer, parser, consteval | serde_json, subfeature | 126k / 38k | zizmor, `shuck-extract` |
| `yamlpath` | 2.0k (1 file) | route → tree-sitter CST node → span, comments, anchors | tree-sitter 0.27, tree-sitter-yaml 0.7.2, line-index, self_cell | 638k / 282k | 10 (zizmor, yamlpatch, rsigma-parser, clapfig, …) |
| `yamlpatch` | 5.5k | comment/format-preserving YAML edits | yamlpath, yaml_serde, subfeature | 363k / 205k | 7 |
| `subfeature` | 0.2k | locates a substring inside a feature (fuzzy) | regex, memchr | 374k / 215k | 3 |
| `tree-sitter-iter` | 0.1k | pre-order CST iterator | tree-sitter | 364k / 212k | yamlpath |
| `zizmor-sarif` | 0.3k | SARIF 2.1.0 structs | serde | 41k / 30k | zizmor |
| `pre-commit-models` | 0.4k | pre-commit config models | yaml_serde | 19k | zizmor |
| `zizmor-dev` | 0.7k | test/bench helpers | — | — | — |

On versioning and API stability:
- `github-actions-models` went from **0.1 to 0.45 in about two years** (2024-01 to 2026-03), then switched to lockstep 1.x at 1.24.0 (2026-04).
- Lockstep "minor" releases still break APIs. For example, `Trigger` changed from an `enum` to a `struct` in #2414 (commit `e966565b`, 1.30 to 1.31).
- 29 commits touched models, expressions, or yamlpath between v1.24 and HEAD.

The full zizmor dependency graph has **447 packages**.

### 1.2 How a workflow is loaded (the double or triple parse)

`Workflow::from_string` (`crates/zizmor/src/models/workflow.rs:143`) runs two or three steps.

1. **`Validatable::validate`** (`crates/zizmor/src/models.rs:96`) runs `yaml_serde::from_str::<github_actions_models::workflow::Workflow>`. This is libyaml-rs, a c2rust port. If it fails:
   - The source is re-parsed as `yaml_serde::Mapping` (`:122`). If that also fails, the result is a **Syntax** error.
   - Otherwise the Mapping is converted to `serde_json::Value` and validated against the bundled SchemaStore `github-workflow.json` (`crates/zizmor/src/data/`, via `jsonschema` 0.58). If the schema rejects it, the result is a **Schema** error, whose message is a list of JSON-pointer paths.
   - If the schema accepts it, the result is a **Model** error, which means a bug in github-actions-models.
2. **`yamlpath::Document::new`** (`crates/yamlpath/src/lib.rs:569`) parses the source *again* with tree-sitter-yaml.
   - If `root.has_error()`, it returns `InvalidInput(row, col)` for the first `ERROR`/`MISSING` node (`:584-596`). The whole document is rejected, and there is no partial tree.
   - It builds an anchor map, `HashMap<name, BTreeMap<offset, Node>>`, inside a `self_cell` (`:485-538`).
3. In non-strict mode, `InputGroup::register` (`crates/zizmor/src/registry/input.rs:575-584`) skips **only** Syntax and Schema errors, with a warning. **A yamlpath error, or a Model error, is fatal for the whole run** (§3.1).

The audits operate on the **typed serde model**, wrapped in zizmor-side "enriching" types: `models/workflow.rs`, `models/action.rs`, `models/workflow/matrix.rs` (1,177 lines, matrix expansion), and `runners.rs`. These wrappers implement `Locatable` and return a **`SymbolicLocation`**: an `InputKey` plus a `yamlpath::Route` (keys and indices), an annotation, a feature kind (Normal / KeyOnly / Subfeature), and a kind (Primary / Related / Hidden). See `crates/zizmor/src/finding/location.rs:37-70`.

Concrete positions are computed **lazily, when a finding is built** (`FindingBuilder::build`, `crates/zizmor/src/finding.rs:247-289`, then `SymbolicLocation::concretize`, `location.rs:137-205`):
- `Normal` calls `query_pretty`, which returns the whole `key: value` pair.
- `KeyOnly` calls `query_key_only`, which returns just the key.
- `Subfeature` calls `query_exact`, extracts the text, and then runs `Subfeature::locate_within`:
  - The search is `memmem` or a regex, starting from an `after` byte hint.
  - Whitespace becomes `\s+` (`subfeature/src/lib.rs:47-83`).
  - The crate itself calls this "a ridiculous overapproximation" (`:63`) and "a fuzzy anchor" (`:132`, `:161`).
- In every case, it then collects `feature_comments` (`yamlpath/src/lib.rs:1059-1117`). This trawls the **whole tree** for comment nodes within the feature's *line* range.

Each query (`query_node`, `:1150-1279`) walks down from the document root, scanning siblings linearly. There is no caching.

### 1.3 The audit trait and registry

- **`AuditCore`** (`audit/mod.rs:178-210`) supplies `ident()`, `desc()`, and `url()`, generated by the `audit_meta!` macro. It also provides `finding()`, which returns a `FindingBuilder`.
- **`Audit`** (`audit/mod.rs:306-482`) is an `#[async_trait]` with hooks at increasing specificity:
  - `audit_workflow`, `audit_normal_job` / `audit_reusable_job`, `audit_step`;
  - `audit_action`, `audit_docker_action`, `audit_composite_step`;
  - `audit_dependabot`, `audit_pre_commit_*`;
  - `audit_raw`, which works on the raw document.
  
  The default implementations fan out downward. The blanket `audit()` checks `config.disables(ident)` and then dispatches.
- **`Audit::new(&AuditState) -> Result<Self, AuditLoadError::Skip>`**. An audit that needs the network returns `Skip` when offline or when no token is set, as in `known_vulnerable_actions.rs:317-326`. The registry then logs a debug line and omits the audit.
- **`AuditRegistry::default_audits`** (`registry.rs:30-91`) is a hand-written `register_audit!` list of **41 audits**, stored as `IndexMap<&'static str, Box<dyn Audit + Send + Sync>>`.
- **`FindingRegistry::extend`** (`registry.rs:146-190`) applies, in order:
  1. a config severity remap;
  2. the persona: if the finding's persona is below the requested one, it is *suppressed*;
  3. inline or config ignores, `--min-severity`, and `--min-confidence`: these mark it *ignored*;
  4. otherwise the finding is kept, and the highest severity is tracked.
  
  Exit codes are 11, 12, 13, or 14 by highest severity (`registry.rs:257-268`). SARIF output always exits 0.

### 1.4 The finding model

`Finding { ident, desc, url, determinations: { confidence: Low|Medium|High, severity: Informational|Low|Medium|High, persona: Auditor|Pedantic|Regular }, locations: Vec<Location>, tip, ignored, fixes: Vec<Fix> }` (`finding.rs:18-160`).

- Each `Location` is a symbolic location plus a concrete `Feature { location: { start_point, end_point, offset_span }, feature: &str, comments }`.
- `Fix { title, key: &InputKey, disposition: Safe|Unsafe, patches: Vec<yamlpatch::Patch> }` (`finding.rs:81-124`).
- `--fix=safe|unsafe-only|all` applies the fixes. `apply_yaml_patches` re-parses the document after **every** patch (`yamlpatch/src/lib.rs:200-218`).
- yamlpatch ops include `RewriteFragment`, `ReplaceComment`, `EmplaceComment`, `Replace`, `Add`, `MergeInto`, and `Append`.

### 1.5 Ignore comments and config

- **Inline ignores** use the regex `# zizmor: ignore\[(.+)\](?:\s+.*)?$` (`finding/location.rs:301`), with comma-separated rule IDs and an optional trailing reason.
  - A finding is ignored if **any** comment within the line span of **any** of its locations matches, including hidden locations (`finding.rs:291-296`).
  - Since "pretty" features often cover a whole `key: value` block (a job, an `on:` mapping), a comment anywhere inside such a block can suppress the finding.
  - Comments inside block scalars don't count, because tree-sitter treats them as string content. This is documented in `docs/usage.md` "Ignoring results".
  - There are no file-level or "next-line" directives, and nothing reports unused ignores.
- **Config** is a `zizmor.yml` holding `rules: { <id>: { disable, ignore: [file:line:col patterns], config: {...}, remap } }` (`config/mod.rs:176-199`).
  - It uses `deny_unknown_fields` and is discovered per input group.
  - Its JSON Schema is `support/zizmor.schema.json`.
  - A move to TOML is proposed (#1560, 8 reactions), partly because the config is literally named like a workflow and schema-matchers confuse the two.

### 1.6 Online and offline modes, and the GitHub client

zizmor is offline unless a token is present (`GH_TOKEN`, `GITHUB_TOKEN`, or `ZIZMOR_GITHUB_TOKEN`). It warns when it falls back to offline (`main.rs:343-357`, #2178).

- **Audits that need the network:** `impostor-commit`, `known-vulnerable-actions`, `ref-confusion`, `ref-version-mismatch`, `stale-action-refs`. `artipacked`, `typosquat-uses`, and `unpinned-uses` use the client optionally.
- **The client** (`github.rs`):
  - `reqwest` plus `reqwest-middleware`, with host-scoped retry (`:289-353`).
  - A **two-tier cache** (`:355-389`): `http-cache-reqwest` with a `cacache` file cache (`CacheMode::Default`) chained with a `moka` in-memory cache (`ForceCache`), plus logging middleware that reads `x-cache`.
  - **Ref listing** uses the **git smart HTTP protocol v2 `ls-refs`** against `https://github.com/<slug>.git/git-upload-pack`, through a hand-rolled pkt-line codec (`github.rs:392`, `github/pktline.rs`). This doesn't count against REST API rate limits. It is a clever trick worth copying if workflint ever resolves action refs.
  - Remote inputs (`zizmor owner/repo@ref`) are fetched by tarball or contents API.
  - GHES is supported via `--gh-hostname`.

### 1.7 Output formats

| Format | Implementation | Notes |
|---|---|---|
| `plain` (default) | `output/plain.rs`, annotate-snippets 0.12, cargo-style multi-span snippets, OSC-8 links | The one actionlint doesn't have: **multi-location findings** (primary plus related spans) |
| `json` / `json-v1` | `output/json/v1.rs` | Points are **0-based** (#1101 asks for 1-based) |
| `sarif` | `output/sarif.rs` + `zizmor-sarif` | Columns are byte columns + 1 (`sarif.rs:173-179`); SARIF's default `columnKind` is UTF-16, so non-ASCII lines are off |
| `github` | `output/github.rs` | `::warning file=…,line=…,title=…::msg`. Start line only; **no `%`/CR/LF escaping** of the message (`github.rs:24-50`) |

### 1.8 LSP

`zizmor --lsp` (`lsp.rs`, built on tower-lsp-server 0.23 behind the default `lsp` feature) is minimal:
- **Sync and analysis:**
  - It uses `TextDocumentSyncKind::FULL`.
  - Every open, change, or save rebuilds the model and re-runs every audit.
  - It always runs **offline**, because the state is `AuditState::default()` (`lsp.rs:406`).
- **Diagnostics:**
  - It publishes the primary location only; `related_information: None` (`:357`).
  - The column is used directly as the LSP `character` (`:394-399`). The column is a UTF-8 byte offset, while LSP expects UTF-16 by default, so diagnostics drift on lines with non-ASCII text.
- **Gaps:**
  - There are no code actions, even though fixes exist.
  - Parse failures become a `log_message`, not a diagnostic. **A file with a YAML error shows nothing in the editor.**
  - The open items are tracked in meta issue #986.

### 1.9 WASM

There is none.
- tree-sitter and its grammars are C (and `tree-sitter-bash` and `-powershell` are linked too).
- The binary uses jemalloc, tokio multi-thread, and reqwest/rustls.

A WASM build would mean feature-gating most of the crate and building tree-sitter for `wasm32` (wasi-sdk or emscripten). None of that is set up.

### 1.10 Concurrency model

- A tokio multi-thread runtime (`#[tokio::main]`, `main.rs:438`) runs `collect_inputs` **sequentially**; the code has a `// TODO: use tokio's JoinSet?` (`main.rs:79`).
- For each input, all 41 audit futures go into a `FuturesOrdered` that is awaited in place (`main.rs:361-382`). There is no `tokio::spawn`, `spawn_blocking`, or rayon anywhere in the crate.
- CPU-bound audits therefore run one after another on one thread. The measurements confirm it: user time roughly equals wall time (§2).
- The async machinery only pays off for overlapping network I/O in online audits.

### 1.11 Tests and benchmarks

- **Tests:** 266 YAML fixtures and insta snapshots under `crates/zizmor/tests/integration/`, plus e2e tests (some online, some needing a token, all behind cargo features).
- **Upstream expression tests:** `github-actions-expressions/tests/languageservices_kat.rs` runs GitHub's own `actions/languageservices` expression test JSON (28 suites in `tests/testdata/`, synced by `support/sync-expression-tests.py`). There is an explicit skip list (`:77-102`): depth errors, memory errors, unknown-context strictness, the empty expression, and `fromJSON` property and index consteval.
- **Benchmarks:** `bench/` contains pytest benchmarks (CodSpeed-style) that run the release binary on pinned grafana and cpython zipballs, offline and online. There are no in-process microbenchmarks.

---

## 2. Performance measurements

The corpus was `.reference/realworld`: 1503 workflows, 3.84 MB. zizmor only collects files under `.github/workflows/` (`registry/input.rs:686-694`), so the files were copied into `.github/workflows/` under a scratch directory.

**The full corpus cannot be measured with zizmor:** one file aborts the run (§3.1). All numbers below therefore use the remaining **1502 files** for both tools. Each run used `hyperfine -w 3 -r 15` with a quiet machine.

| Run | Wall (mean ± σ) | User | Sys | Peak RSS |
|---|---|---|---|---|
| actionlint `-shellcheck= -pyflakes=` (Go, v1.7.13-pre) | **272.9 ms ± 42.8** | 419 ms | 161 ms | 44 MB |
| zizmor `--offline --no-config --format=plain` | **2.816 s ± 0.229** | 2.342 s | 0.221 s | 205 MB |
| zizmor `--offline --no-config --format=json` | 3.558 s ± 0.457 | 2.342 s | 0.897 s | — |
| zizmor, all 41 audits disabled via config (load + startup only) | 0.559 s (best of 3) | | | |
| zizmor, 1 file (startup floor) | 74.9 ms ± 2.0 | 38.5 ms | | |
| `yamlpath::Document::new` alone, all 1503 files (probe, in-process) | 248 ms total (0.165 ms per file) | | | |

What the numbers show:

- **actionlint is about 10.3× faster than zizmor-plain**, and 13× faster than zizmor-json. actionlint's user time is greater than its wall time because it parallelizes. zizmor's user time roughly equals its wall time: it is single-threaded (§1.10).
- **Loading alone costs about 0.56 s**: serde, tree-sitter, the anchor map, the ignore walker, and about 75 ms of startup (jsonschema validators, fst maps, runtime). That is already twice actionlint's whole run.
  - tree-sitter alone is 0.25 s, which agrees with the 06 benchmark: tree-sitter-yaml is about 5× slower than event parsers.
- **The rest, about 2.3 s, goes to audits plus building findings.** The audits that produce the most findings dominate:
  - excessive-permissions (3,180 findings) adds about 0.44 s;
  - unpinned-uses (6,999 findings) adds about 0.44 s;
  - every other audit adds 0.01–0.15 s.
  
  Each finding concretizes its locations: a route walk from the root, plus `feature_comments`, which trawls the whole tree per location. Then annotate-snippets renders it. Finding construction grows as files × findings × tree size.
- **Binary sizes:** zizmor 23.4 MB (release, LTO, including the LSP, bash and PowerShell grammars, and rustls). actionlint 8.0 MB.
- The findings (default persona) were 14,551 in total. The leaders were unpinned-uses 6,999, excessive-permissions 3,180, artipacked 2,406, template-injection 631, and self-repository 406. Two files were skipped with Schema warnings (the tauri pair, §3.6).

**For workflint:** zizmor's speed is not a ceiling to inherit. A fork would start about 10× behind the Go tool we are trying to beat. Getting below about 0.3 s would mean replacing:
- the YAML stack (one parse, no tree-sitter);
- the location model (spans stored in the AST, not recomputed per finding);
- the scheduler (rayon over files).

That is most of what makes zizmor zizmor.

---

## 3. Correctness probe of the reusable crates (`zz-probe`)

### 3.1 Whole-run abort on a file GitHub accepts

`apache_airflow_.github_workflows_ci.yml`, lines 331–332, contains a double-quoted flow scalar whose continuation line is indented by one space:

```yaml
        run: "echo \"::set-output name=host-python-version::$(python -c
 'import platform; print(platform.python_version())')\""
```

libyaml, go-yaml (actionlint), and GitHub all accept it. tree-sitter-yaml 0.7.2 produces an `ERROR` node spanning bytes `0..14311`, so yamlpath reports `input is not valid YAML: error at line 1, column 1`.

This surfaces as `CollectionError::Yamlpath`, which is not one of the skippable variants (`registry/input.rs:575-584`). zizmor prints `fatal: no audit was performed … this typically indicates a bug in zizmor`. **One file stops the audit of all 1503.**

This is the same class of bug as open #1116 (multi-line single-quoted scalar; the fix is in tree-sitter-yaml upstream and unreleased). The #1116 snippet itself did not reproduce with 0.7.2 in our probe.

### 3.2 Edge-case matrix

| Input | `github-actions-models` (serde) | `yamlpath` (tree-sitter) | actionlint behaviour (for contrast) |
|---|---|---|---|
| Unknown keys (`runs_on`, `stepz`, step `names`) | **OK, silently ignored** | finds them if asked | error per key at line:col |
| Duplicate key in a job (`runs-on` twice) | `ERR … untagged enum Job at line 3 column 3` (points at the job, not the key) | returns the **first** occurrence | positioned duplicate-key error |
| Duplicate job ID (`a:` twice) | **OK**, one job kept | returns the first | positioned error |
| Merge key `<<: *j` | `ERR … untagged enum Job` | `mapping has no key runs-on` (merge keys unsupported) | (GitHub rejects merge keys, so does actionlint) |
| Alias as whole job (`b: *j`) | OK | resolves to the anchor's span | resolves |
| Bad indentation | ERR untagged enum | `InvalidInput` **line 1, column 1** | positioned syntax error |
| Tab indentation | ERR "found character that cannot start any token at line 4 column 1" | **line 1, column 1** | positioned |
| Unclosed quote | ERR with line 4 col 14 | line 4 col 14 | positioned |
| Multi-document (`---`) | `ERR more than one document is not supported` | silently uses the first doc | actionlint errors |
| CRLF | OK | OK (byte spans include `\r`) | OK |
| Non-ASCII on same line (`{name: 日本語, runs-on: …}`) | OK | `runs-on` at column **23** (UTF-8 bytes); character column is 17 | rune-based column |
| Quoted key with escape (`"ab":`) | OK, key `ab` | `mapping has no key ab` (naive unquote, `yamlpath/src/lib.rs:1383-1389`) | handled |
| Flow-sequence steps `steps: [uses: x@v4]` | OK | `unexpected node: plain_scalar` (#1671) | OK |
| `continue-on-error: yes` | ERR untagged enum (no position on the field) | — | type error at the value |
| `timeout-minutes: abc` | ERR untagged enum Job at the job | — | type error at the value |
| Empty file | `missing field on` | — | positioned |

**Position model summary.**
- Spans are byte spans `[start, end)` plus 0-based (row, byte-column) pairs. They are available as key-only, value-only ("exact"), or key-plus-value ("pretty").
- That is fine-grained, *if* the route resolves. It is computed by a second parser after the fact, and it disagrees with the serde model whenever the two parsers disagree: duplicate keys, multi-doc, merge keys, escapes in keys.
- Positions *inside* scalars are fuzzy text searches over the decoded value. They break on escapes in quoted scalars (#2422, where `'${{ ''foo'' }}'` fails to parse in raw audits; the maintainer calls the YAML abstractions "brittle").

**Anchors** are "beta" (`docs/usage.md:1085-1110`): there is a one-time warning (`main.rs:362-366`) and duplicate findings (#1577). Anchor *definitions* are tracked by offset, so redefinition works, but anchor names cannot come out of the serde model.

### 3.3 `github-actions-expressions` probe

| Input | Result |
|---|---|
| `github.event.issue.title` | OK; contexts with span `0..24` |
| `a == 'x' && b \|\| c` | OK; three contexts with spans |
| `foo(1)` | `ERR Invalid function call`, with **no offset**; unknown functions are rejected at parse time (`call.rs:83`) |
| `success(1)` | `ERR Invalid function call` (arity), with **no offset** |
| `contains(a, b` | `ERR … expected , or ) (at offset 13)` |
| `1 +` | `ERR invalid numeric literal (at offset 2)` (the message is misleading: `+` lexes as a number start) |
| `0x1F`, `1e3`, `format('{0}', 1, 2)` | consteval: 31, 1000, `"1"` |
| `a.*.b`, `github['foo bar']` | OK; the context pattern is normalised to `github.foo bar` |

**Capabilities:**
- A hand-written lexer and a Pratt parser (`lexer.rs`, `parser.rs`).
- A recursion budget of 50, matching GitHub (`parser.rs:20`).
- Case-insensitive functions, including `case()`.
- `SpannedExpr` with byte `Origin`s.
- Context extraction (`contexts()`, `dataflow_contexts()`), context patterns with `*`.
- **Constant evaluation** that follows GitHub's coercion rules (`consteval`, `lib.rs:770+`), KAT-tested against GitHub's own suite.

**Missing:**
- Type checking.
- Context *availability* (which contexts are allowed in which keys).
- Context schemas (`github.event.*` shapes, `steps.<id>.outputs`, `needs`, `matrix` typing).
- Multi-error recovery.

The parser is also "deliberately permissive about unknown context names" (`parser.rs:4`).

Error quality: a single error per expression, a `&'static str` message, a byte offset only for syntax errors, and no span at all for call errors (`lib.rs:23-47`, `call.rs:6-14`). actionlint's messages (`expr_lexer.go`/`expr_parser.go`/`expr_sema.go`) are different text with different positions. We would have to post-process every error anyway, and still write the whole type checker.

### 3.4 `github-actions-models`

The models are complete and up to date: parallel steps (`wait`, `wait-all`, `cancel`, `parallel`, `background`, June 2026), `cache-mode`, `image_version` trigger, dependabot v2.

But for a linter:
- There are **no spans**.
- **Unknown keys are accepted** (#290 is the open plan to tighten this, which needs "a nontrivial API change" because `deny_unknown_fields` doesn't compose with `flatten`).
- There are **20 `#[serde(untagged)]` enums**. Each one swallows the real error and reports "did not match any variant" at the enclosing node (#1673, #1241 on performance).
- **Duplicate map keys are accepted silently** in `IndexMap` fields (jobs, env, with).

`common.rs` has decent `uses:` parsers (`RepositoryUses`, `DockerUses`, local). These are about 300 lines and easy to reimplement.

### 3.5 `yamlpath`, `yamlpatch`, `subfeature`

`yamlpath` is the most-used crate (10 reverse dependencies). Its API is tidy (`Route`, `Component`, `query_{pretty,exact,key_only,exists}`, `removal_span`, `feature_comments`, `extract*`).

The maintainer himself calls the internals "pretty messy and not super easy to maintain" (#1472). He proposes an eager "reduced CST" and has weighed replacing tree-sitter with saphyr `MarkedYaml` (#930).

`yamlpatch` has open soundness bugs:
- #1579: `Replace` on a flow mapping drops sibling keys.
- #2286: route-addressed fixes on the same node don't converge in one pass.
- #1235: root insertions land at EOF.

It also pulls in a second YAML stack (tree-sitter plus yaml_serde) just to emit edits. Workflint's AST already has spans, so span-based text edits (rustfix/ruff-style `Edit { range, replacement }`) are simpler and cannot disagree with the parser.

### 3.6 Schema errors (zizmor) vs actionlint, on the same file

Here is zizmor `--strict-collection` on `tauri-apps_tauri_.github_workflows_update-docs.yml`:

```
jobs.update-docs.steps.8.with: [{"version":…},…] is not of type "object"
jobs.update-docs.steps.8.with: [...] is not of type "string"
jobs.update-docs.steps.10: "run" is a required property
jobs.update-docs: Additional properties are not allowed ('runs-on', 'steps' were unexpected)
jobs.update-docs: "uses" is a required property
```

And actionlint on the same file:

```
…update-docs.yml:74:11: "with" section is sequence node but mapping node is expected [syntax-check]
…update-docs.yml:85:9: unexpected key "working-directory" for step to execute action. expected one of … [syntax-check]
```

zizmor's output has no positions. The `oneOf` branches leak out: it claims `runs-on` and `steps` are not allowed, because the reusable-workflow branch also failed. And without `--strict-collection` the file is just *skipped* with a one-line warning. **This is the gap workflint exists to fill.**

---

## 4. Crate-by-crate reuse assessment

| Crate | API quality | Maturity / churn | License | Fit for actionlint-style precise diagnostics | Recommendation |
|---|---|---|---|---|---|
| `zizmor` (fork base) | Internal `pub(crate)` everything; async trait; good Finding model | 87 releases; very active; effectively one maintainer | MIT | Poor: lossy models, double parse, fatal on parse disagreement, single-threaded, 10× slower | **No fork** |
| `github-actions-models` | Clean serde structs, good docs | 0.1 → 0.45 → 1.31 in 2.7 years; breaking changes in minors (#2414); strictness redesign pending (#290) | MIT | None: no spans, lenient on unknown keys, opaque errors, whole-file failure | **Dev-dependency only**: oracle for "does our typed model agree on accepted workflows", and a checklist of newly supported syntax (parallel steps, cache-mode) |
| `github-actions-expressions` | Good: spanned AST, consteval, KATs; small dependency footprint (serde_json, subfeature) | 30 releases since 2025-05; no open bugs; one outside user (shuck) | MIT | Partial: parser only, single error, spanless call errors, unknown-function errors at parse time; no typing | **Dev-dependency differential fuzz oracle** (parse accept/reject plus consteval equality vs our parser). Copy the *KAT harness idea* and sync the same upstream JSON (MIT, from `actions/languageservices`) ourselves |
| `yamlpath` | Tidy public API, internals self-described messy | 52 releases | MIT | Wrong layer: after-the-fact tree-sitter CST queries, rejects whole doc on any error, byte columns, naive key unquoting | **No.** Our AST carries spans |
| `yamlpatch` | Nice op vocabulary (RewriteFragment, EmplaceComment, MergeInto…) | Open unsound bugs (#1579, #2286, #1235) | MIT | Brings a second YAML stack just for edits | **No**; borrow the op vocabulary and the safe/unsafe disposition; implement span-based edits |
| `subfeature` | Small | — | MIT | Fuzzy by design | **No** |
| `tree-sitter-iter` | Trivial | — | MIT | n/a | No |
| `zizmor-sarif` | Minimal SARIF 2.1.0 serde structs (311 lines) | 10 releases since 2026-05 | MIT | Fine but tiny, and lacks `columnKind` handling | **Optional.** Either depend on it (no deps beyond serde) or write our own 300 lines; `serde-sarif` is the alternative |
| `pre-commit-models` | — | — | MIT | n/a | No |

**Data assets worth copying (MIT, with attribution):**
- `crates/zizmor/data/context-capabilities.csv`: 4,608 `github.event.*` paths classified `fixed`/`structured`/`arbitrary`, generated from GitHub's webhook schemas by `support/webhooks-to-contexts.py`. This is a better-sourced superset of actionlint's hand-maintained untrusted-input list (`expr_insecure.go`). Re-run their generator against upstream, rather than vendoring the CSV snapshot.
- `crates/zizmor/data/codeql-injection-sinks.json`: action inputs that are code-injection sinks, derived from CodeQL models (MIT, GitHub). It is useful if workflint extends actionlint's `untrusted` check beyond `run:` and `github-script`.
- `support/sync-expression-tests.py`: the pattern for pulling `actions/languageservices` expression KATs.

---

## 5. Strengths to copy

1. **Every finding has a concrete span, and multi-location findings exist.** Primary, related, and hidden locations, each with an annotation, rendered by annotate-snippets and mapped to SARIF related locations (#1803: "a critical internal assumption… every symbolic location… can be concretized"). Workflint should allow related spans, for example "defined here" for duplicate IDs or `needs` cycles, while keeping actionlint's single-line default output.
2. **Key-only, value, and pair span variants.** The `query_key_only` / `query_exact` / `query_pretty` vocabulary is the right one for diagnostics. In our AST these are just three fields per mapping entry.
3. **The `Fix` model:** a title, a target input, a `Safe`/`Unsafe` disposition, and `--fix=safe|unsafe-only|all`. The exit code becomes success when everything was fixed. The yamlpatch op names are a good checklist: rewrite fragment, emplace comment, merge-into, append.
4. **Inline ignore syntax** `# tool: ignore[a,b] reason`, plus config-based ignores by `file:line:col`. Improve on it: scope to the *primary key line* rather than the whole span, add `ignore-next-line` and file-level directives, and report unused ignores.
5. **Self-skipping online audits.** `Audit::new` returns `Skip` when the network or a token is missing. zizmor is offline by default with a clear warning, and it has a two-tier HTTP cache and the git `ls-refs` trick for ref resolution without REST quota. This is the right template if workflint ever adds `--online` (for example, fetching `action.yml` metadata for third-party input checking, which actionlint covers with a bundled snapshot).
6. **Upstream KAT tests** for expressions, from `actions/languageservices`.
7. **Personas mapped to rule profiles.** `regular`, `pedantic`, and `auditor` map onto actionlint-style "default" plus opt-in pedantic rules. Users already use zizmor's `pedantic` as a quality linter (#1445).
8. **Per-severity exit codes**, with SARIF exit 0 so upload steps don't fail. Offer it opt-in, and keep actionlint's exit codes by default for parity.
9. **Engineering hygiene:** `unsafe_code = "forbid"`, the clippy lints (`unwrap_used`, `print_stdout`, …), insta snapshots, jemalloc on non-Windows, and `human-panic`.
10. **Input breadth:** workflows, actions, dependabot, and pre-commit, with `--collect=` modes and `.gitignore`-aware walking via `ignore`. Workflint should at least do workflows plus `action.yml`, and maybe dependabot later.

## 6. Weaknesses to avoid

1. **Two parsers that can disagree.** serde plus tree-sitter is the root cause of #1116, #1671, #2422, #1577, #1744, and our airflow abort. **Use one parser that produces one span-carrying AST** (06 §1.5).
2. **Fatal parse failures.** One file kills the run (§3.1); malformed YAML gives `line 1, column 1`; the LSP shows nothing. Workflint must report syntax errors as diagnostics and keep linting the other files. Ideally it also keeps linting the recoverable parts of the broken file.
3. **Lenient typed models with untagged enums.** Unknown keys vanish and type errors become "did not match any variant". Type checking must be done by our own checker walking the AST, as actionlint does.
4. **JSON Schema as the error reporter.** No positions, and `oneOf` noise. If we use SchemaStore at all, it should be a test oracle only.
5. **Positions recovered by text search** (subfeature). Expression spans must be computed from the scalar's *source* representation, mapping decoded offsets back through escapes and folding, at parse time.
6. **Byte columns everywhere,** fed straight into LSP `character` and SARIF. Store byte offsets, convert per consumer: runes or chars for actionlint parity, UTF-16 for LSP and SARIF.
7. **Single-threaded async.** Use rayon over files; keep async (if any) at the network edge.
8. **Lazy concretization with a full-tree comment trawl per location.** Attach comments to AST nodes once.
9. **Lockstep versioning with breaking minors.** If we publish library crates, use real semver.
10. **The GitHub output format doesn't escape `%`, CR, or LF**, and JSON uses 0-based points (#1101). These are small things to get right from day one.

---

## 7. Issue analysis (138 open issues, 25 open PRs)

**Dates:** 9 filed in 2024, 61 in 2025, 68 in 2026.

**Labels:** enhancement 92, new-audit 29, bug 20, triage 13, help wanted 10, autofix 8, config 8, cli 8, needs-decision 6, long-term 6, performance 5, false-negative 4, autofix:unsound 2, anchors 1, language-server 1, ghes 1.

### 7.1 Categories (each issue counted once)

| Category | # | Issues |
|---|---|---|
| New audit request | 35 | 195, 260, 288, 459, 479, 635, 711, 723, 730, 827, 914, 935, 1023, 1044, 1118, 1208, 1222, 1225, 1272, 1348, 1405, 1481, 1533, 1605, 1607, 1806, 1816, 1948, 1968, 2012, 2044, 2134, 2294, 2322, 2323 |
| Audit enhancement / false negative | 19 | 197, 237, 282, 678, 737, 738, 938, 1128, 1206, 1278, 1619, 1848, 2028, 2105, 2128, 2130, 2176, 2181, 2366 |
| Config / ignore / personas | 14 | 396, 617, 629, 784, 1086, 1271, 1445, 1560, 1885, 2111, 2142, 2232, 2343, 2443 |
| False positive / persona tuning | 13 | 398, 540, 1043, 1098, 1126, 1424, 1865, 1914, 2071, 2148, 2257, 2378, 2433 |
| Output formats & CLI UX | 12 | 864, 1101, 1117, 1135, 1569, 1695, 1819, 1951, 2094, 2190, 2203, 2296 |
| Online / GitHub API / caching / GHES | 11 | 278, 380, 606, 690, 735, 766, 1252, 1350, 2052, 2210, 2403 |
| yamlpatch / autofix | 10 | 1235, 1346, 1524, 1563, 1579, 1958, 2033, 2096, 2286, 2427 |
| YAML parsing / positions / anchors | 8 | 930, 963, 1116, 1472, 1577, 1671, 1744, 2422 |
| New input kinds (pre-commit, prek, lockfiles, …) | 5 | 1209, 1803, 2256, 2260, 2371 |
| Docs | 4 | 325, 1039, 2309, 2359 |
| Performance | 3 | 1241, 1244, 1908 |
| Models crate | 2 | 290, 1673 |
| LSP | 1 | 986 |
| Distribution | 1 | 2194 |
| Expressions crate | 0 | — |

**Most reacted:**

| # | Title | Reactions |
|---|---|---|
| #1350 | Poor experience across multiple private repos | 14 |
| #398 | `dangerous-triggers` persona when permissions are constrained | 11 |
| #1023 | Require `timeout-minutes` | 10 |
| #1560 | TOML config | 8 |
| #2260 | prek.toml | 7 |
| #1445 | Persona in config | 7 |
| #2371 | Actions lockfiles | 5 |
| #2256 | pre-commit parity | 5 |
| #766 | Immutable actions | 5 |
| #678 | Recursion through reusable workflows and composite actions | 5 |
| #2322 | `shell-syntax` | 4 |
| #288 | Encourage `shell: bash` | 4 |

Five of these top asks are correctness or quality checks, not security: #1023, #2322, #288, #2323, #678.

### 7.2 Workflint-relevant findings

**(a) Bugs in the YAML stack we would inherit by depending on yamlpath:**
- #1116: tree-sitter-yaml rejects a valid multi-line single-quoted scalar, so the repo is not audited. The fix is upstream and unreleased.
- #1671: flow-sequence steps crash `unpinned-uses` (reproduced in §3.2).
- #2422: `''` escapes break expression parsing in raw audits ("brittle YAML abstractions").
- #1472: the yamlpath internals are messy; an eager reduced CST is proposed.
- #930: saphyr `MarkedYaml` is considered as a replacement.
- #1577: anchors produce duplicate findings ("core would need to become anchor-aware").
- #1744: the `dangerous-triggers` span covers the whole `on:`, so GitHub won't annotate a PR when the span's first line isn't in the diff.
- #963: subfeatures are only partly rolled out.
- Related: #2443 (ignore comments on job-level `uses:` not honoured), #2433 (`# v7.0.1 # ryl disable-line` is one comment), #1101 (0-based JSON).

**(b) Performance:**
- #1241: untagged enums are slow; hand-written `Deserialize` impls are proposed.
- #1908: a git-protocol fast path for remote inputs.
- #2322: "Running shellcheck inside of zizmor is out of the question for performance reasons."
- #1405 and #2044: zizmor stays fast by reading few files.

**(c) Model and expression bugs we would inherit:**
- #1673: `secrets: inherit` on `workflow_call` produces "did not match any variant of untagged enum Trigger", so the whole file is skipped. PR #2159 is open.
- #290: unknown keys are accepted. The fix will hard-reject (not lint) and change the API.
- #2366: indirect matrix values are not tracked.
- The expressions crate has **no** open bugs and supports `case()`.

**(d) False positives in checks that overlap ours:**
- #1128: template-injection misses `services.*.options` and `container`.
- #1424: no call graph across inputs.
- #540: reusable caller and callee permissions.
- #1126: `excessive-permissions` knows scopes, not needs.
- #1043: audits are not action-version-aware; it suggests `persist-credentials` for `checkout@v1`, which has no such input. actionlint's version-aware action-metadata database handles this.
- #1914: bot-conditions dominance logic.

**(e) Feature requests that overlap actionlint-style checks:**
- #288: `shell: bash`.
- #2322: shell syntax via the tree-sitter `ERROR` nodes or shuck. The shuck author: "would allow for the types of static checks that `actionlint` provides".
- #2323: oversized `run:` blocks.
- #1023: `timeout-minutes`. The comparison table there notes that actionlint lacks it (actionlint #49).
- #1225: concurrency and path filters.
- #827: old runner images, using endoflife.date data.
- #1481: disallowed keys in `copilot-setup-steps.yml`.
- #678: recursive reusable-workflow and composite-action resolution.
- #2294: overly broad tag globs.

**(f) Distribution, LSP, packaging:**
- #986: LSP meta. Open items: config invalidation, fixes via LSP, online audits in the LSP, incremental analysis.
- #2194: npm distribution declined. The reasons are package sprawl and no glibc tagging; the maintainer calls the PyPI package "arguably… a mistake" and notes musl's performance cost.
  - Workflint's plan in 06 (npm `optionalDependencies` per platform, static musl or zigbuild) runs into exactly these points. We should benchmark musl plus mimalloc early.
- #2260 / #2256: TOML inputs would need a `tomlpath`.

**(g) Autofix:**
- #1579 and #1346 (both `autofix:unsound`).
- #2286: fixes don't converge.
- #1235: root insertion.
- #1098: a wrong `persist-credentials` fix for jobs that push.

**Governance:**
- There is effectively one maintainer (woodruffw).
- A strict AI-contribution policy is enforced in templates and triage (#1958, #2012, #2382 checkbox).
- Breaking changes are queued for 2.0 (#1560, #2094).

Upstreaming the fixes we would need (strict models with spans, a replacement for tree-sitter) would be slow, and it would cut against their roadmap.

---

## 8. Overlap and positioning: how zizmor describes its relationship to actionlint

The repo docs (`docs/*.md`, README) **never mention actionlint**. The positioning lives in maintainer comments on the issue tracker:

- **#931** (closed): "In *general*, I would prefer to have `zizmor` perform security checks only -- I want it to be a specialized tool, with other tools like actionlint providing non-security checks/linting for workflow and action definitions." Later: "general correctness checks is still a non-goal."
- **#2382** (closed as duplicate, asking for `runs-on: ubuntu` and `names:` typo detection): "general correctness checks are not in zizmor's scope (we do security checks, and we recommend actionlint or GitHub's own LSP tooling for correctness checks)."
- **#2029** (shellcheck wrapper PR, closed): "zizmor intentionally doesn't depend on any external tools, even optional ones, and this use case is already well-handled by tools like actionlint."
- **#1571** (unquoted shell variables, closed): "a lot of zizmor users combine the two effectively, with zizmor providing security checks and actionlint providing general quality and correctness checks."
- **#1225** (open): "the goal with zizmor is primarily *security* audits, not general code quality… I would definitely recommend checking out `actionlint` -- they do a much larger set of CQA checks."
- **#1023** (open): audits with "less-clear security impact… go under a non-default persona or are opt-in."
- **#1445** (a user): "actionlint is great, but it will only fail on really wrong stuff". That user runs zizmor `pedantic` as a quality linter.

**Recommendation for workflint's scope:**

1. **Stay a correctness linter.** Keep exactly actionlint's security-flavoured rules for parity:
   - `expression` untrusted-input (`expr_insecure.go`);
   - `credentials` (hardcoded container credentials);
   - `deprecated-commands`;
   - `permissions` scope validation;
   - `shellcheck` / `pyflakes` integration.
   
   Do **not** port zizmor's 41 audits. In particular, do not port the online, supply-chain ones (impostor-commit, known-vulnerable-actions, ref-confusion, unpinned-uses, cache-poisoning, artipacked). Users will run zizmor next to us, and both maintainers already recommend that pairing.
2. **Improve the overlapping security rule with zizmor's data.** Replace actionlint's hand-written untrusted-context list with one generated from webhook schemas (the `context-capabilities.csv` approach). Extend sinks with the CodeQL list. Do this behind actionlint-compatible messages, perhaps with an opt-in stricter mode.
3. **Absorb the correctness and quality demand zizmor declines.** Each of these was asked for on zizmor's tracker and declined or parked:
   - `timeout-minutes` (#1023);
   - explicit `shell: bash` (#288);
   - shell syntax without shellcheck (#2322);
   - oversized `run:` (#2323);
   - step and job naming (#1272);
   - concurrency hygiene (#1225, #1619);
   - outdated runner images (#827);
   - disallowed keys for special workflows like `copilot-setup-steps.yml` (#1481);
   - local reusable-workflow and composite-action resolution (#678).
   
   They fit an opt-in "pedantic" profile modelled on zizmor's personas. Note that jactionlint (07) is already moving into this space and markets itself as a zizmor replacement. We should differentiate on precision, speed, and LSP, not on security breadth.
4. **Interop, not competition.**
   - Emit SARIF with correct UTF-16 columns and a stable `ruleId` namespace (`workflint/<rule>`), so that both tools' results coexist in GitHub code scanning.
   - Accept `# actionlint:` style ignores (parity) plus our own.
   - Don't parse `# zizmor: ignore[...]`; those belong to zizmor.

---

## 9. Verdict

| Option | Assessment |
|---|---|
| **A — Fork zizmor** | **Reject.** We would delete the audits, online layer, and personas, and rewrite the YAML layer, model layer, location layer, scheduler, and LSP. What's left is the CLI skeleton and output renderers, which are cheap to write. It starts 10× slower than the Go actionlint we are trying to beat. Its foundations (lenient lossy models, a double parse, fatal parse disagreement) contradict actionlint's "report everything with a position and keep going". Upstream would never take those changes (#931, #290 direction). MIT allows forking, so license is not the blocker; architecture is. |
| **B — Depend on crates** | **Reject for production code** (for v1). `github-actions-models` has no spans, is lenient, and uses untagged enums. `yamlpath` and `yamlpatch` are a second, fragile parser. `subfeature` is fuzzy by design. `github-actions-expressions` is the only plausible candidate, but parity needs our own error messages, positions, multi-error recovery, and a type checker built on our own AST. The parser is the easy 20% (about 1k lines), and taking it couples us to a lockstep-versioned crate with breaking minors. |
| **B′ — Dev-dependencies / data** | **Adopt.** (1) `github-actions-expressions` as a differential fuzz and KAT oracle for our expression parser and consteval. (2) `github-actions-models` as an oracle that our typed layer accepts every workflow theirs accepts, and as a feature tracker for new GitHub syntax (parallel steps, `cache-mode`, `image_version`). (3) Regenerate `context-capabilities` and the injection sinks with attribution. Optionally (4) `zizmor-sarif` for SARIF structs. |
| **C — Borrow architecture and patterns** | **Adopt (primary).** The patterns: Finding with primary/related/hidden locations; key/value/pair span vocabulary; Fix with safe/unsafe dispositions; ignore comments plus config ignores; offline-by-default with self-skipping online rules, a two-tier HTTP cache, and git `ls-refs`; personas as rule profiles; KAT syncing from `actions/languageservices`; insta snapshot tests; strict workspace lints; and (from 06) the release, packaging, and pre-commit approach. |
| **D — Ignore** | Too strong. zizmor is the closest Rust precedent. Its issue tracker is a free map of YAML-span pitfalls (#1116, #1671, #2422, #1577, #1744) and of unmet correctness demand. |

**Final answer: C, plus B′.** Keep zizmor crates out of `[dependencies]`, use them in `[dev-dependencies]` as oracles, and reuse their MIT data with attribution. Revisit `zizmor-sarif` when we implement SARIF.

---

## Appendix: reproduction

```sh
# $SCRATCH is any scratch directory outside the repo
# build (keeps the clone clean)
cd .reference/zizmor && CARGO_TARGET_DIR=$SCRATCH/zizmor-target cargo +1.97.1 build --release -p zizmor --locked   # 7m31s cold

# corpus (zizmor only collects .github/workflows/*)
mkdir -p $SCRATCH/zz-corpus/.github/workflows && cp .reference/realworld/* $SCRATCH/zz-corpus/.github/workflows/
$SCRATCH/zizmor-target/release/zizmor --offline --no-config $SCRATCH/zz-corpus   # -> fatal (airflow ci.yml)
# 1502-file variant without that file: $SCRATCH/zz-corpus2

hyperfine -w 3 -r 15 \
  ".reference/bin/actionlint -shellcheck= -pyflakes= $SCRATCH/zz-corpus2/.github/workflows/* >/dev/null" \
  "$SCRATCH/zizmor-target/release/zizmor --offline --no-config --no-exit-codes --format=plain $SCRATCH/zz-corpus2 >/dev/null 2>&1"

# load-only and per-audit cost: bash $SCRATCH/zz-prof.sh (writes zizmor.yml configs disabling all audits but one)
# crate edge-case probe: $SCRATCH/zz-probe (cargo run --release; bin tserr = list files tree-sitter rejects)
```
