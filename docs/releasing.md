# Releasing workflint

`.github/workflows/release.yml` builds workflint for six targets, packs each build into an archive, writes `checksums.txt` and an SBOM, attests every asset, and publishes them as a GitHub release (ADR 0014, ADR 0021). Every other channel repackages these assets.

## Publishing is off until M5.10

Nothing publishes yet. M5.10, Release 0.1.0 ([#258](https://github.com/stokehold/workflint/issues/258)), turns publishing on. Until then every channel runs as a dry run only: this workflow, and the later M6 tasks for the install scripts, npm, PyPI, Docker, Homebrew, pre-commit, and the GitHub Action. No job uses a secret, pushes a tag, uploads to a registry, or creates a GitHub release. Rehearsals and tag runs create attestations. They are stored in the repository and in Sigstore's public transparency log, and they are not releases.

The release workflow has a switch, `PUBLISH: "false"` in its top-level `env`. A tag run builds, checks, and attests the assets, and skips the `release` job.

A tag pushed before M5.10 never gets a release. Tags that match `v*` cannot be deleted or moved, and a re-run of its workflow reads the switch from the tagged commit, where it is off. Do not push a `v*` tag before M5.10. If one is pushed anyway, the next release uses the next version.

To turn publishing on, M5.10:

1. Checks that the `release` environment exists, requires the maintainer's approval, and deploys only from tags that match `v*` (M6.0, [#17](https://github.com/stokehold/workflint/issues/17)).
2. Sets `PUBLISH` to `"true"` in `.github/workflows/release.yml`.
3. Turns on the switch of every other channel in the table below.
4. Works through the [release-day checklist](#release-day-checklist) before the tag is pushed.
5. Rewrites this section to say that publishing is on.

Each M6 task that adds a channel adds its switch to this table, off.

| Channel | Switch | State |
| --- | --- | --- |
| GitHub release | `PUBLISH` in `.github/workflows/release.yml` | off |

## Release-day checklist

The maintainer does these steps by hand before a release. Each item says when.

- [ ] **npm trusted publishers, within the 48 hours before the first publish from `release.yml`.** That publish is the 0.1.0 release (M5.10). This is step 6 of M6.1 ([#18](https://github.com/stokehold/workflint/issues/18)). An npm trusted publishing configuration expires 48 hours after it was created unless a publish has succeeded with it, and the first successful publish binds it to the repository for good. So create the configurations for `workflint` and the six platform packages at most 48 hours before that publish. `npm trust` needs npm 11.15.0 or later and 2FA on the account, and it takes one package per call, so the loop pauses 2 seconds between calls:

  ```sh
  for pkg in workflint @stokehold/workflint-linux-x64 @stokehold/workflint-linux-arm64 \
    @stokehold/workflint-darwin-x64 @stokehold/workflint-darwin-arm64 \
    @stokehold/workflint-win32-x64 @stokehold/workflint-win32-arm64; do
    npm trust github "$pkg" --repo stokehold/workflint --file release.yml --env release --allow-publish --yes
    sleep 2
  done
  ```

  If the publish does not happen within those 48 hours, the configurations expire, and the loop has to run again before the next attempt.

## How a release happens

1. release-plz (M6.3) opens a release pull request with the version bump and the changelog. Merging it pushes the tag `v<version>`.
2. The tag starts the release workflow. The `plan` job reads the version of the `workflint` package with `cargo metadata` and fails unless the tag is `v` followed by that version. When it fails, no other job runs.
3. The `build` jobs build and pack the six archives, the `sbom` job writes the SBOM, and the `checksums` job writes `checksums.txt` and checks every asset. The `attest` job attests each asset. The `gate` job runs `cargo test --workspace --locked`.
4. The `release` job waits for the maintainer to approve the deployment to the `release` environment.
5. It checks the assets, their checksums, and their attestations, creates a draft release for the tag with every asset, and then publishes the draft. The notes are the version's section of `CHANGELOG.md`, or notes that GitHub generates when the changelog has no such section.

Releases are immutable. Once published, a release cannot gain, lose, or change an asset, so the workflow uploads every asset while the release is a draft. A later task that adds a release asset, such as the install scripts of M6.4, uploads it in the `release` job before the publishing step. Channels that publish somewhere else, such as npm, PyPI, Docker, and Homebrew, run after the release is published.

## Modes

| Mode | Started by | Jobs |
| --- | --- | --- |
| Dry run | A pull request that changes `.github/workflows/release.yml`, `scripts/release/`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, or `crates/workflint/Cargo.toml` | `plan`, `build`, `sbom`, `checksums` |
| Rehearsal | A manual run on `main`: `gh workflow run release.yml --repo stokehold/workflint --ref main` | The dry run, `gate`, and `attest`, which also verifies each attestation with `gh attestation verify` |
| Release | A push of the tag `v<version>` | Every job. `release` runs only when `PUBLISH` is `"true"` |

Each run uploads the assets as workflow artifacts, and the `checksums` job lists each file with its size and SHA-256 in the job summary. A rehearsal from another branch fails in `plan`.

The build jobs have only `contents: read`, no secrets, no environment, and no caches, and they build with `--locked`. Only `attest` has `id-token: write` and `attestations: write`, and only `release` has `contents: write`.

## Assets

| Target | Runner | Archive |
| --- | --- | --- |
| `x86_64-unknown-linux-musl` | `ubuntu-24.04` | `workflint_<version>_linux_amd64.tar.gz` |
| `aarch64-unknown-linux-musl` | `ubuntu-24.04-arm` | `workflint_<version>_linux_arm64.tar.gz` |
| `x86_64-apple-darwin` | `macos-15-intel` | `workflint_<version>_darwin_amd64.tar.gz` |
| `aarch64-apple-darwin` | `macos-latest` | `workflint_<version>_darwin_arm64.tar.gz` |
| `x86_64-pc-windows-msvc` | `windows-latest` | `workflint_<version>_windows_amd64.zip` |
| `aarch64-pc-windows-msvc` | `windows-11-arm` | `workflint_<version>_windows_arm64.zip` |

`<version>` has no leading `v`, as in actionlint's asset names. Each archive holds `workflint`, or `workflint.exe` on Windows, with `README.md`, `LICENSE`, and `NOTICE` at its root and no directory around them. The Linux binaries are static musl binaries, linked by cargo-zigbuild 0.23.4 with zig 0.16.0 (`scripts/release/requirements.txt`).

Each release also has:

- `checksums.txt`, one `<SHA-256>  <file name>` line per asset, sorted by file name, in the format of `sha256sum`.
- `workflint_<version>.cdx.json`, a CycloneDX 1.5 SBOM of the dependencies of the `workflint` binary on every target, written from `Cargo.lock` by cargo-cyclonedx 0.5.9.
- A build provenance attestation for each archive, `checksums.txt`, and the SBOM.

## Verifying an asset

Download the asset and `checksums.txt` into one directory, then check the checksum:

```sh
sha256sum -c checksums.txt --ignore-missing
```

Check that the release workflow of this repository built the asset:

```sh
gh attestation verify <file> --repo stokehold/workflint
```

Check that the release is immutable and that an asset belongs to it:

```sh
gh release verify v<version> --repo stokehold/workflint
gh release verify-asset v<version> <file> --repo stokehold/workflint
```

## A broken release

A published release is never replaced. Its tag cannot move and its assets cannot change. If a release is broken, fix the problem and ship the next patch version.

## Smoke test

`scripts/release/smoke-test.sh <binary> <version>` runs `<binary> -version` and fails unless it exits with 0 and prints `<version>` on its first line. Every channel runs it on the binary it installs. Each build job runs it on the binary from its archive, and then checks that the second line of `-version` is the build-time value of `WORKFLINT_INSTALLED_FROM`.

## `WORKFLINT_INSTALLED_FROM`

The second line of `workflint -version` says how workflint was installed (ADR 0014). The release pipeline sets `WORKFLINT_INSTALLED_FROM` when it builds, and the binary reads it with `option_env!`. A launcher can override it at run time with an environment variable of the same name. A binary built without it prints `installed by building from source`.

| Channel | Value |
| --- | --- |
| GitHub release archives | `installed by downloading from release page` |

The tasks that add the other channels add their values here.

## The release scripts

The workflow's logic lives in `scripts/release/`, where it can be tested:

| Script | Purpose |
| --- | --- |
| `plan.sh` | Chooses the mode, checks the tag against the version, and reads the `PUBLISH` switch. |
| `package.sh` | Packs a binary with `README.md`, `LICENSE`, and `NOTICE` into an archive. |
| `smoke-test.sh` | The smoke test above. |
| `check-assets.sh` | Checks the set of assets, the archive contents, static linking, `checksums.txt`, and the SBOM. |
| `summary.sh` | Writes the job summary. |
| `notes.sh` | Reads the release notes from `CHANGELOG.md`. |
| `test.sh` | Tests `plan.sh`, `notes.sh`, `smoke-test.sh`, and `package.sh`. |

The `plan` job runs shellcheck and `test.sh` before it plans. To run them locally:

```sh
shellcheck scripts/release/*.sh
scripts/release/test.sh
```
