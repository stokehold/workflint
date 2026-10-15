# Competitive landscape (snapshot 2026-10-15)

| Project | Lang | Stars | Activity | Notes |
|---|---|---|---|---|
| rhysd/actionlint | Go | 4.3k | last commit 2026-04-19, last release v1.7.12 (2026-03-30) | upstream; maintainer inactive, weekly data generator failing since March |
| kjanat/actionlint | Go fork | 47 | v1.17.0 (2026-09-13), active daily | composite-action context checks, cache-mode + cache safety policies ON by default, opt-in policies (`policy.require-job-timeout`, `require-permissions`), config JSON schema in npm pkg, Nix flake, aqua/mise, completions, man page, checks against GitHub's own schemas/fixtures |
| jdx/jactionlint | Go fork (jdx = mise author) | 242 | v2.2.0 (2026-10-14), very active | ~30 new rules per release; rule profiles (`default`, `correctness`, `pedantic`, security); `--online` + `JACTIONLINT_ONLINE` env; `--changed-since`, `--list-rules`, `--explain`, fact export (JSON repo model), JSON Schemas for config+output, PyPI wheels (`uvx`), shell syntax check without shellcheck, github-script JS syntax check, autofix reasons, signed macOS binaries, positions itself as zizmor replacement |
| zizmorcore/zizmor | Rust | 6.7k | very active | security-focused GH Actions auditor; see 06-rust-ecosystem.md |

Implication: "port to Rust" alone is not a differentiator vs. these forks. Differentiators must come from: speed, single static binary w/o Go, embedded shellcheck-equivalent/hermetic results, LSP, precise spans, autofix, library crate, action.yml as first-class, release cadence/data automation.
