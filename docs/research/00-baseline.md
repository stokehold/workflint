# Baseline: Go actionlint performance (2026-10-15)

Machine: Apple M1 Pro, 8 cores. actionlint built from main @ 011a6d1 (v1.7.13-dev), go1.26.4.

| Input | Flags | Wall | User | Sys | Max RSS |
|---|---|---|---|---|---|
| realworld corpus (1503 files, 7.5 MB, from testdata/realworld/dataset.zip) | -shellcheck= -pyflakes= | 0.31 s | 0.46 s | 0.27 s | 36 MB |
| realworld corpus | shellcheck enabled (homebrew shellcheck) | **HUNG >10 min** (deadlock, many shellcheck children stuck in S state) | | | |

Output without shellcheck: 27,278 lines (~6.8k diagnostics).

Note: the shellcheck hang reproduces upstream issue #702/#704/#712 on macOS even with PR #651 ("fix-shellcheck-deadlock-macos") merged on main.
Note: hyperfine -N doesn't expand globs; use a file list or shell mode for future comparisons.
