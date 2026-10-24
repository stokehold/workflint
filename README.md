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

A GitHub Action (`stokehold/workflint-action`) and a pre-commit hook are also planned.

### Install script

Each release will attach `install.sh` for Linux, macOS, and Git Bash on Windows, and `install.ps1` for PowerShell. They refuse an archive whose SHA-256 does not match the release's `checksums.txt`, and check its attestation when `gh` is installed and logged in:

```sh
curl -fsSL https://github.com/stokehold/workflint/releases/latest/download/install.sh | bash
bash <(curl -fsSL https://github.com/stokehold/workflint/releases/latest/download/install.sh) latest ./bin
```

```powershell
irm https://github.com/stokehold/workflint/releases/latest/download/install.ps1 | iex
```

`install.sh` takes the arguments of actionlint's `download-actionlint.bash`, `[VERSION|latest] [DIR]`, and also writes `executable=<path>` to `GITHUB_OUTPUT`. A CI script that runs actionlint's download script switches by changing the URL. [`docs/releasing.md`](docs/releasing.md#install-scripts) has every option.

### npm

The npm package `workflint` runs the binary of the release archive for your platform. Each binary is in its own package, such as `@stokehold/workflint-linux-x64`, and npm, pnpm, and Bun install only the one that matches your machine. Nothing runs at install time, so `--ignore-scripts` and the defaults of pnpm and Bun work:

```sh
npm install --save-dev workflint
pnpm add --save-dev workflint
bun add --dev workflint
```

Run it with `npx workflint`, `pnpm exec workflint`, or `bunx workflint`, or from a script in `package.json`. npx reads some single-dash flags as its own, so put `--` before workflint's flags: `npx workflint -- -version`. The second line of `workflint -version` is `installed from npm`.

### PyPI

The PyPI package `workflint` has a wheel for each of the six release targets: Linux on x86_64 and arm64 with glibc or musl, macOS on Intel and Apple silicon, and Windows on x86_64 and arm64. Each wheel holds only the workflint binary of the release archive, so installing it needs no Rust toolchain:

```sh
pip install workflint     # into the current environment
pipx install workflint    # into its own environment
uvx workflint             # runs it without installing it
```

### Docker

`ghcr.io/stokehold/workflint` has images for Linux on amd64 and arm64. Mount the repository at `/repo` and run the image there:

```sh
docker run --rm -v "$PWD:/repo" -w /repo ghcr.io/stokehold/workflint
docker run --rm -v "$PWD:/repo" -w /repo ghcr.io/stokehold/workflint:full
```

The default tag holds only workflint, on a distroless base. It has no shellcheck, so workflint warns once per run that it skips the `run:` scripts. Pass `-shellcheck=` to turn that check off, or use the `full` tag, which adds shellcheck and the Python linters ruff and pyflakes, so workflint checks shell and Python scripts as well. Both run as a non-root user. Pin a release with `:<version>` or `:<version>-full`.

## Documentation

| Document | Contents |
|---|---|
| [docs/cli.md](docs/cli.md) | The command line: flags, output, exit codes, shell completions, and the man page. |
| [docs/config.md](docs/config.md) | The config file: where workflint looks for it, and its keys. |
| [docs/sarif.md](docs/sarif.md) | SARIF output, and how to upload it to GitHub code scanning. |
| [docs/github-output.md](docs/github-output.md) | Annotations and a job summary in GitHub Actions, and several formats in one run. |
| [docs/releasing.md](docs/releasing.md) | How a release happens, the release assets, and how to verify them. |
| [docs/roadmap.md](docs/roadmap.md) | Milestones and tasks. |
| [docs/architecture.md](docs/architecture.md) | Crates, data flow, main types. |
| [docs/adr/](docs/adr/README.md) | Architecture decision records. |
| [docs/parity-checklist.md](docs/parity-checklist.md) | Every actionlint feature workflint must reproduce. |
| [docs/differences.md](docs/differences.md) | Where workflint deliberately differs from actionlint. |
| [docs/issues-coverage.md](docs/issues-coverage.md) | How workflint handles each open actionlint issue and pull request. |
| [docs/glossary.md](docs/glossary.md) | Terms used across the project. |
| [docs/research/](docs/research/README.md) | The research behind the decisions. |
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to contribute. |
| [SECURITY.md](SECURITY.md) | How to report a vulnerability. |

## License

MIT. workflint is a derivative work of actionlint by rhysd, also MIT-licensed. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
