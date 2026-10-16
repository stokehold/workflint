# Research

These reports record the research done on 2026-10-15, before work on workflint started. They are historical records. We do not update them when the design changes.

The decisions live in the ADRs in `docs/adr/`. When an ADR and a research report disagree, the ADR wins.

## Reports

| File | Topic | Summary |
|---|---|---|
| [00-baseline.md](00-baseline.md) | Performance baseline | Go actionlint lints the 1503-file corpus in about 0.3 s without shellcheck and hangs on macOS with it. |
| [00-synthesis.md](00-synthesis.md) | Synthesis | Lists the facts from the other reports that drive the design, plus candidate differentiators. |
| [01-parsing-ast.md](01-parsing-ast.md) | Parsing and AST | Describes how actionlint turns workflow YAML into its AST and compares Rust YAML crates against go-yaml. |
| [02-expressions.md](02-expressions.md) | Expressions | Covers the `${{ }}` lexer, parser, type checker and untrusted-input checks in actionlint. |
| [03-rules.md](03-rules.md) | Rules | Documents every actionlint rule as a parity checklist for the port. |
| [04-cli-output-distribution.md](04-cli-output-distribution.md) | CLI, output and distribution | Covers the actionlint command, linter driver, output formats, test corpus and release process. |
| [05-issues.md](05-issues.md) | Upstream issues and PRs | Groups the open actionlint issues and PRs by theme and records a decision for each theme. |
| [06-rust-ecosystem.md](06-rust-ecosystem.md) | Rust ecosystem | Picks Rust crates for YAML, CLI, diagnostics, distribution, WASM and LSP, with a YAML benchmark. |
| [07-landscape.md](07-landscape.md) | Landscape | Compares related linters by language, stars, activity and features. |
| [08-zizmor.md](08-zizmor.md) | zizmor | Concludes that workflint should borrow patterns from zizmor but not fork it or depend on its crates. |
| [09-jactionlint.md](09-jactionlint.md) | jactionlint | Lists what the jactionlint fork adds over upstream and which parts workflint should take. |
| [10-github-action.md](10-github-action.md) | GitHub Action | Designs an official workflint GitHub Action, using zizmor-action as the main reference. |
| [11-open-prs-and-kjanat.md](11-open-prs-and-kjanat.md) | Open PRs and kjanat fork | Reviews open zizmor PRs and the kjanat/actionlint fork, and ends with a checklist of open-PR findings. |

## Recreating `.reference/`

The reports cite files under `.reference/`. That directory is a local checkout and git ignores it.

`cargo xtask reference` recreates the parts that the conformance tooling needs:

- `.reference/actionlint`, a checkout of actionlint at commit `011a6d15e749bb3f2d771eed9c7aa0e7e3e10ee7`.
- `.reference/bin/actionlint`, built from that checkout with the Go version in `.go-version`. That is go1.26.4, the version that built the binary in `00-baseline.md`.
- `.reference/realworld`, the 1503 workflow files unpacked from the checkout's `testdata/realworld/dataset.zip`.

The command needs git and that Go version. A second run skips every step that is already up to date. It leaves the other directories under `.reference/` alone.

The other projects stay manual. Clone each one and check out the commit the research used.

```sh
git clone https://github.com/jdx/jactionlint.git .reference/jactionlint
git -C .reference/jactionlint checkout 1bcb2641337b5a92c2c07c4eafae61d64268e94b

git clone https://github.com/zizmorcore/zizmor.git .reference/zizmor
git -C .reference/zizmor checkout 5eb37696bbf148d4fe74a84fc5073efe14e537a4

git clone https://github.com/zizmorcore/zizmor-action.git .reference/zizmor-action
git -C .reference/zizmor-action checkout 38c745a43764dcc8bcfd364e8295870e252e957c

git clone https://github.com/kjanat/actionlint.git .reference/kjanat-actionlint
git -C .reference/kjanat-actionlint checkout 8ac13d23f49d0aeccae67d2c33022ef9ef44279c
```

Some reports also cite raw GitHub issue and PR dumps in `.reference/research/raw/`. Those files stay local and are not in the repository.

Paths in the reports that pointed at scratch directories on the author's machine now read "a scratch directory" or `$SCRATCH`. We did not commit the probe programs in those directories.
