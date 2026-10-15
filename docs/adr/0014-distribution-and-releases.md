# ADR 0014: Distribution and release pipeline

- Status: accepted
- Date: 2026-10-15

## Context

People install linters through the channel their ecosystem already uses. JavaScript projects use npm. Python projects and pre-commit users use PyPI. macOS users use Homebrew. CI pipelines use a GitHub Action, Docker, or a download script. Rust developers use cargo. The project lives in the `stokehold` GitHub organization, which will host more tools later.

## Decision

Targets:

- workflint ships prebuilt binaries for six targets: Linux x86_64 and aarch64 (static musl binaries, which also run on glibc systems), macOS x86_64 and aarch64, and Windows x86_64 and aarch64.
- Builds run on native GitHub-hosted runners for each OS and architecture. Linux musl builds use `cargo-zigbuild`.

Release assets:

- Archives are named `workflint_<version>_<os>_<arch>.tar.gz`, or `.zip` on Windows, with `os` in `linux`, `darwin`, `windows` and `arch` in `amd64`, `arm64`. These names follow actionlint's convention, so generic installers that already handle actionlint work unchanged.
- Each release has `checksums.txt` (SHA-256), GitHub artifact attestations for every archive, and an SBOM.

Versioning:

- Versions follow semantic versioning. Releases stay at 0.x until 1.0 (`docs/roadmap.md`).
- Commit messages follow Conventional Commits. release-plz opens a release pull request with the version bump and changelog. Merging it tags the release, and the tag triggers the release workflow.
- Data refreshes trigger patch releases automatically (ADR 0011).

Channels, all built from the same release assets:

| Channel | Name | Notes |
|---|---|---|
| GitHub Releases | `stokehold/workflint` | Source of every other channel. |
| Install scripts | `install.sh`, `install.ps1` | Attached to each release. They verify SHA-256 against `checksums.txt`, and run `gh attestation verify` when `gh` is available. `install.sh` accepts actionlint's download-script arguments (`[VERSION\|latest] [DIR]`). |
| Homebrew | `stokehold/homebrew-tap`, formula `workflint` | A formula, not a cask, so macOS does not quarantine it. The tap is shared by all stokehold tools. The release workflow updates it. |
| npm | `workflint`, plus `@stokehold/workflint-<os>-<arch>` | The main package has a small JavaScript launcher and lists one package per platform as `optionalDependencies`, with `os` and `cpu` fields. No install scripts, so it works with pnpm, Bun, and `--ignore-scripts`. Published with npm trusted publishing and provenance. |
| PyPI | `workflint` | Binary-only wheels built with maturin. Published with trusted publishing. Gives `pip install workflint` and `uvx workflint`. |
| pre-commit | `stokehold/workflint-pre-commit` | A mirror repository with a `language: python` hook backed by the PyPI wheel, updated on each release. The main repository also has `.pre-commit-hooks.yaml` with a `language: system` hook. |
| Docker | `ghcr.io/stokehold/workflint` | Multi-arch images for amd64 and arm64. The default tag is a distroless static image with only workflint. The `-full` tag adds shellcheck and the Python linters. Images carry provenance and SBOM attestations. |
| crates.io | `workflint` and the `workflint-*` crates | The name is reserved now with a placeholder. Full publishing starts at 1.0 (ADR 0006). Includes `cargo binstall` metadata pointing at the release archives. |
| GitHub Action | `stokehold/workflint-action` | See ADR 0015. |

The maintainer reserves the npm name, the `@stokehold` npm scope, and the PyPI name by hand before M6 (`docs/roadmap.md`).

Version output:

- `workflint -version` prints three lines: the bare version, how it was installed (for example `installed by downloading from release page` or `installed from npm`), and the compiler and target. The install channel is set at build time and can be overridden by the `WORKFLINT_INSTALLED_FROM` environment variable, which the npm launcher, the Docker image, and the Homebrew formula set.

Pipeline:

- The release pipeline is built in M6, in parallel with M1 to M5. It runs in dry-run mode on every pull request that touches packaging, so every channel is proven before 0.1.0.

## Consequences

Users get workflint through the channel they already use, with verifiable artifacts. One tag produces every channel.

Nine channels are a lot to maintain. The dry-run job keeps them working between releases, and all of them repackage the same six archives.
