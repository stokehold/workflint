# Research synthesis (2026-10-15)

Reports in this folder: 00-baseline (perf), 01 parsing/AST, 02 expressions, 03 rules (parity checklist), 04 CLI/output/distribution, 05 upstream issues+PRs, 06 Rust ecosystem, 07 landscape, 08 zizmor, 09 jactionlint, 10 GitHub Action design, 11 open PRs (zizmor, kjanat) + consolidated checklist §d.

## Facts that drive the design
- Upstream actionlint (Go, 4.3k★): ~34k LOC incl. tests; stalled since 2026-04; data generators failing since March. 154 open issues, 55 open PRs; most pain = stale data + missing rule control.
- Perf baseline: 1503 real-world files in 0.27–0.31 s, 36–44 MB without shellcheck; shellcheck dominates (50 files ≈ 18 s) and hangs on macOS on main.
- jactionlint (jdx, Go, 10 days old, AI-built, 161 rules): actionlint+zizmor merged; default profile loud (24k findings vs 6.8k); 1.0 s; node-per-script → 16–19 s.
- kjanat/actionlint (Go): best-quality fork; composite actions, schemas generated from actions/runner + actions/languageservices, exact shellcheck positions, rule catalogue + inline ignores, ruff. Author considering Rust core (#69); maintains rshellcheck (GPL-3).
- Astral also maintains an actionlint fork.
- zizmor (Rust, 6.7k★): security auditor; verdict = borrow patterns only (C). Typed serde models lose positions & reject whole files; double parse (serde + tree-sitter); ~10× slower than actionlint; one tree-sitter failure aborted the whole 1503-file run. Reuse MIT data (untrusted-context list, languageservices expression test sync) + dev-dep oracles.
- YAML: fork libyaml-safer 0.3.0 → byte-identical positions with go-yaml on 1719/1733 files, ~4× faster; patch EOF block scalar panic; add comment capture.
- Names: workflint free on crates.io, npm (+@workflint scope), PyPI, Homebrew, Docker Hub. GitHub org "WorkFlint" exists (2026-08-02, 0 repos) — ownership unknown.

## Candidate differentiators
Speed (rayon, shellcheck dedupe/cache/batch), hermetic core checks (no node/shellcheck needed for core), exact spans (block-scalar source maps) → autofix + LSP + SARIF regions, quiet defaults with opt-in rule groups, action.yml first-class, data generated from authoritative sources with automated refresh, reads actionlint config + ignore comments, official verified GitHub Action, library crate, WASM playground.
