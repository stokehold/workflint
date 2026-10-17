# workflint

[![CodSpeed](https://img.shields.io/endpoint?url=https://codspeed.io/badge.json)](https://app.codspeed.io/stokehold/workflint?utm_source=badge)

workflint is a linter for GitHub Actions workflows and action metadata files, written in Rust. It is a compatible rewrite of [actionlint](https://github.com/rhysd/actionlint). It accepts actionlint's flags and config file, prints the same output layout, and fixes actionlint's known bugs.

workflint is in development and has no usable release yet. [`docs/roadmap.md`](docs/roadmap.md) shows what is being built and in which order.

## Goals

- Catch mistakes that make a workflow fail on GitHub before you push it: invalid keys, type errors in `${{ }}` expressions, contexts used where they are unavailable, broken `needs` and matrix references, unknown runner labels, and script problems found by shellcheck.
- Replace actionlint without changing CI scripts. Exit codes, flags, `.github/actionlint.yaml`, and `-format` templates keep working ([`docs/adr/0003-actionlint-compatibility.md`](docs/adr/0003-actionlint-compatibility.md)).
- Lint `action.yml` files and composite actions.
- Keep GitHub platform data current: runner labels, permissions, events, and action inputs are regenerated from official sources and released automatically.
- Give per-rule control: severities, opt-in rule categories, and `# workflint: ignore[rule-id]` comments.
- Lint the 1,503-workflow test corpus at least three times faster than actionlint ([`docs/adr/0019-performance-targets.md`](docs/adr/0019-performance-targets.md)).

## Planned installation

These channels are planned for the first release ([`docs/adr/0014-distribution-and-releases.md`](docs/adr/0014-distribution-and-releases.md)):

```sh
brew install stokehold/tap/workflint
npm install --save-dev workflint
pip install workflint
cargo binstall workflint   # from 1.0, when the crates are published
docker run --rm -v "$PWD:/repo" -w /repo ghcr.io/stokehold/workflint
```

A GitHub Action (`stokehold/workflint-action`), a pre-commit hook, and install scripts for Linux, macOS, and Windows are also planned.

## Documentation

| Document | Contents |
|---|---|
| [docs/roadmap.md](docs/roadmap.md) | Milestones and tasks. |
| [docs/architecture.md](docs/architecture.md) | Crates, data flow, main types. |
| [docs/adr/](docs/adr/README.md) | Architecture decision records. |
| [docs/parity-checklist.md](docs/parity-checklist.md) | Every actionlint feature workflint must reproduce. |
| [docs/differences.md](docs/differences.md) | Where workflint deliberately differs from actionlint. |
| [docs/issues-coverage.md](docs/issues-coverage.md) | How workflint handles each open actionlint issue and pull request. |
| [docs/glossary.md](docs/glossary.md) | Terms used across the project. |
| [docs/research/](docs/research/README.md) | The research behind the decisions. |
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to contribute. |

## License

MIT. workflint is a derivative work of actionlint by rhysd, also MIT-licensed. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
