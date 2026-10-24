# Releasing workflint

`.github/workflows/release.yml` builds workflint for six targets, packs each build into an archive, writes `checksums.txt`, an SBOM, and the install scripts, attests every asset, and publishes them as a GitHub release (ADR 0014, ADR 0021). Every other channel repackages these assets. The same workflow builds the [PyPI wheels](#pypi-wheels) and publishes them to PyPI, builds the [Docker images](#docker-images) and pushes them to GHCR, builds the [npm packages](#npm-packages) and publishes them to npm, and tests the [Homebrew formula](#homebrew) and opens a pull request with it in `stokehold/homebrew-tap`. It tests the [pre-commit hooks](#pre-commit-hooks) and asks the mirror repository `stokehold/workflint-pre-commit` to pin the new version.

## Publishing is off until M5.10

Nothing publishes yet. M5.10, Release 0.1.0 ([#258](https://github.com/stokehold/workflint/issues/258)), turns publishing on. Until then every channel runs as a dry run only: this workflow, and the later M6 tasks for the install scripts, npm, PyPI, Docker, Homebrew, pre-commit, and the GitHub Action. No job pushes a tag, uploads to a registry, or creates a GitHub release. The only job that uses a secret is `release-pr` in `release-plz.yml`, which mints a stokehold-bot token on a manual run to open a rehearsal release pull request ([release-plz](#release-plz)). Rehearsals and tag runs create attestations. They are stored in the repository and in Sigstore's public transparency log, and they are not releases.

The release workflow has a switch, `PUBLISH: "false"` in its top-level `env`. A tag run builds, checks, and attests the assets, builds and checks the wheels, builds, checks, and attests the Docker images, builds and checks the npm packages, tests the Homebrew formula, tests the pre-commit hooks, and skips the `release`, `pypi-publish`, `docker-publish`, `npm-publish`, `homebrew-update`, and `pre-commit-dispatch` jobs.

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
| Release tag, pushed by release-plz | The repository variable `RELEASES_ENABLED`, read by `.github/workflows/release-plz.yml` | off, unset |
| Install scripts | `PUBLISH` in `.github/workflows/release.yml`. The `release` job uploads them with the other assets. | off |
| PyPI | `PUBLISH` in `.github/workflows/release.yml`, which also gates the `pypi-publish` job | off |
| npm | `PUBLISH` in `.github/workflows/release.yml`, which also gates the `npm-publish` job | off |
| Docker images | `PUBLISH` in `.github/workflows/release.yml`, which also gates the `docker-publish` job | off |
| Homebrew | `PUBLISH` in `.github/workflows/release.yml`, which also gates the `homebrew-update` job | off |
| pre-commit mirror | `PUBLISH` in `.github/workflows/release.yml`, which also gates the `pre-commit-dispatch` job. The mirror tags only versions with platform wheels on PyPI. | off |

## Release-day checklist

The maintainer does these steps by hand before a release. Each item says when. Before 0.1.0 (M5.10, [#258](https://github.com/stokehold/workflint/issues/258)), every item applies.

- [ ] **A green rehearsal on `main`, on the day of the release.** [Run one](#running-a-rehearsal) on the commit to release, and check that `release-dry-run` passed with every channel and both attestation jobs at `success`.

- [ ] **Reviewers on the `release` environment, before the first release.** The environment requires the maintainer's approval and deploys only from tags that match `v*` (M6.0, [#17](https://github.com/stokehold/workflint/issues/17)). Every job that publishes waits for that approval.

- [ ] **`RELEASES_ENABLED` set to `true`, when the version is 0.1.0.** release-plz pushes no tag without it ([`RELEASES_ENABLED`](#releases_enabled), M6.3). Step 2 of [cutting 0.1.0](#cutting-010) sets it.

- [ ] **`pypa/gh-action-pypi-publish` in the organization's actions allowlist, before the first release.** The `pypi-publish` job uses it (ADR 0021). Dry runs skip that job, so a missing entry shows up only on the first tag run.

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

- [ ] **`stokehold/homebrew-tap` ready, before the first release.** The repository exists and is public, the `stokehold-bot` installation includes it, "Allow auto-merge" is on, and the ruleset on `main` requires a pull request, squash merges, and the `tests` checks. Its `tests.yml` passes on a pull request. `homebrew-update` fails on a tap without the installation, and auto-merge fails on a tap without the ruleset.

- [ ] **The `workflint` container package public and linked, after the first `docker-publish` run.** A package that a workflow pushes for the first time is private. In the organization's Packages, open `workflint`, then Package settings: set the visibility to public, and check that the package is linked to `stokehold/workflint`. Until then `docker pull ghcr.io/stokehold/workflint` fails for everyone else.

- [ ] **The settings of `stokehold/workflint-pre-commit`, before the first release.** The mirror's [update and tag workflows](#pre-commit-hooks) need them:
  - The repository is in the `stokehold-bot` installation and in the repository access of `STOKEHOLD_BOT_CLIENT_ID`.
  - Its `automation` environment deploys only from `main` and has the secret `STOKEHOLD_BOT_PRIVATE_KEY`. Its `release` environment deploys only from `main` and requires the maintainer's approval.
  - "Allow auto-merge" and immutable releases are on. A ruleset on `main` requires the checks `scripts`, `test (ubuntu-latest)`, `test (macos-latest)`, and `test (windows-latest)`, and a tag ruleset blocks moving and deleting `v*` tags.

## How a release happens

1. release-plz opens a release pull request with the version bump and the changelog ([release-plz](#release-plz)). Merging it pushes the tag `v<version>`.
2. The tag starts the release workflow. The `changes` job runs the [publishing guard](#the-publishing-guard), and the `plan` job reads the version of the `workflint` package with `cargo metadata` and fails unless the tag is `v` followed by that version. When it fails, no other job runs.
3. The `build` jobs build and pack the six archives and build the six wheels, and the `sbom` job writes the SBOM. The `checksums` job writes the release copies of the install scripts, then `checksums.txt`, and checks every asset. The `install-scripts` jobs test the install scripts against these assets. The `attest` job attests each asset. The `gate` job runs `cargo test --workspace --locked` and then `cargo xtask bench` with the Go, hyperfine, and shellcheck versions of the nightly benchmark job. It writes the benchmark results to the job summary and fails when a measurement fails or workflint misses a target of ADR 0019, so a release that misses a target never reaches the `release` job. The nightly job takes about ten minutes. The gate times out after 230 minutes, which covers every scenario timing out.
4. The `release` job waits for the maintainer to approve the deployment to the `release` environment.
5. It checks the assets, their checksums, and their attestations, creates a draft release for the tag with every asset, and then publishes the draft. The notes are the version's section of `CHANGELOG.md`, or notes that GitHub generates when the changelog has no such section.
6. The `pypi-publish` job waits for a second approval of the `release` environment. It checks the assets and their attestations again, checks each wheel against the SHA-256 that its build job recorded, and uploads the wheels to PyPI.
7. The `docker-publish` job waits for its own approval of the `release` environment. It verifies the attestations of the two images and pushes them to GHCR ([Docker images](#docker-images)).
8. The `npm-publish` job also waits for an approval of the `release` environment. It checks the archives and their attestations, checks each npm tarball against the SHA-256 that `npm-pack` recorded, and publishes the six platform packages and then `workflint` ([npm packages](#npm-packages)).
9. The `homebrew-update` job waits for an approval of the `release` environment too. It downloads `checksums.txt` from the published release, verifies its attestation, renders the formula, and opens the pull request `workflint <version>` in `stokehold/homebrew-tap` with auto-merge on ([Homebrew](#homebrew)). The tap merges it when its tests pass.
10. The `pre-commit-dispatch` job runs after `pypi-publish`, with an approval of the `release` environment. It sends `stokehold/workflint-pre-commit` a `workflint-release` event, and the mirror pins the version and tags it ([pre-commit hooks](#pre-commit-hooks)).

Releases are immutable. Once published, a release cannot gain, lose, or change an asset, so the workflow uploads every asset while the release is a draft. The install scripts go up in the same step as the archives. A later task that adds a release asset uploads it in the `release` job before the publishing step. Channels that publish somewhere else, such as npm, PyPI, Docker, and Homebrew, run after the release is published.

## Modes

| Mode | Started by | Jobs |
| --- | --- | --- |
| Dry run | A pull request that changes a file of `scripts/release/packaging-paths.txt` | `changes`, `plan`, `build`, `sbom`, `checksums`, `install-scripts`, `pypi-check`, `pypi-smoke`, `pypi-smoke-alpine`, `docker-build`, `docker-index`, `npm-pack`, `npm-smoke`, `homebrew`, `pre-commit`, and `release-dry-run` |
| No packaging change | Any other pull request | `changes` and `release-dry-run`. Every other job skips. |
| Rehearsal | A manual run on `main` ([how to run one](#running-a-rehearsal)) | The dry run, `gate`, `attest`, and `docker-attest`, which also verify each attestation with `gh attestation verify` |
| Release | A push of the tag `v<version>` | Every job. `release`, `pypi-publish`, `docker-publish`, `npm-publish`, `homebrew-update`, and `pre-commit-dispatch` run only when `PUBLISH` is `"true"` |

Each run uploads the assets as workflow artifacts, and the `checksums` job lists each file with its size and SHA-256 in the job summary. A rehearsal from another branch fails in `plan`.

### The `release-dry-run` check

The release workflow runs on every pull request, with no `paths:` filter, because a required check from a filtered workflow never reports when the filter does not match. Three jobs make the dry runs one check that pull requests can require:

1. `changes` lists the files that the pull request changes since its merge base with `git diff --name-only <base>...<head>`, and `scripts/release/changes.sh` matches them against `scripts/release/packaging-paths.txt`. Its output `packaging` is `true` when one matches, and always `true` on a tag or a rehearsal. The job also runs the [publishing guard](#the-publishing-guard).
2. `plan` runs only when `packaging` is `true`. Every other job needs it, so with it skipped, they all skip.
3. `release-dry-run` needs every dry-run job and both attestation jobs, and runs even when one of them fails. `scripts/release/dry-run-result.sh` writes a table to the job summary with one row per channel and the result of its jobs, and fails the check when a job failed or was cancelled. When `packaging` is `true`, a dry-run job that skipped fails it too, and in a rehearsal or a release, so does an attestation job that skipped. A pull request without packaging changes gets the check as a pass in under a minute.

A new dry-run job goes into the `needs` of `release-dry-run` and into the table of `dry-run-result.sh`. The script fails when the two disagree. A new file that a release depends on goes into `packaging-paths.txt`.

The `main` ruleset requires `release-dry-run`.

### The publishing guard

`scripts/release/check-publish-guard.py` reads `release.yml` and fails when a job that could publish can run outside a tag run. A job could publish when it has a `write` permission, an `environment:`, or a reference to `secrets.`. Its `if:` must then start with `startsWith(github.ref, 'refs/tags/v') &&`, followed by a condition without `||`, so that a dry run or a rehearsal skips the job whatever the rest says. The `release`, `pypi-publish`, `docker-publish`, `npm-publish`, `homebrew-update`, and `pre-commit-dispatch` jobs all have that condition in front of the `PUBLISH` switch. The attestation jobs `attest` and `docker-attest` are the exception, listed by name in the script: they run in rehearsals, and may write only `id-token` and `attestations`. The workflow itself grants no write permission. The guard runs in the `changes` job, so every pull request checks it, and it needs PyYAML from `scripts/release/requirements-pyyaml.txt`.

### Running a rehearsal

A rehearsal runs every dry-run job, `gate`, and both attestation jobs on `main`, and verifies each attestation as a user would. It creates attestations, which are stored in the repository and in Sigstore's transparency log. It creates no release, tag, package version, or pull request.

1. Start it: `gh workflow run release.yml --repo stokehold/workflint --ref main`.
2. Watch it: `gh run watch --repo stokehold/workflint "$(gh run list --repo stokehold/workflint --workflow release.yml --event workflow_dispatch --limit 1 --json databaseId --jq '.[0].databaseId')"`.
3. Check the summary of `release-dry-run`: every row, `Attestations` included, says `success`.

The build jobs have only `contents: read`, no secrets, no environment, and no caches, and they build with `--locked`. Only `attest` and `docker-attest` have `attestations: write`, only they, `pypi-publish`, and `npm-publish` have `id-token: write`, only `release` has `contents: write`, and only `docker-publish` has `packages: write`. `homebrew-update` gives the `GITHUB_TOKEN` no permissions and writes to the tap with a stokehold-bot token. `pre-commit-dispatch` also gives the `GITHUB_TOKEN` no permissions and sends its event with a stokehold-bot token.

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
- `install.sh` and `install.ps1`, the [install scripts](#install-scripts), which install `<version>` by default.
- A build provenance attestation for each archive, `checksums.txt`, the SBOM, and each install script.

## PyPI wheels

`pyproject.toml` at the root of the repository describes the PyPI package `workflint`: binary-only wheels built with maturin, with the version of the `workflint` crate. Each build job builds its target's wheel with maturin after it packs the archive, and `compare-wheel.sh` fails the job unless the binary in the wheel is byte-identical to the binary in the archive. The job then records the SHA-256 of the wheel as a job output and uploads the wheel as the artifact `wheel-<target>`. PyPI gets no source distribution.

| Target | Wheel platform tag |
| --- | --- |
| `x86_64-unknown-linux-musl` | `manylinux_2_17_x86_64.manylinux2014_x86_64.musllinux_1_1_x86_64` |
| `aarch64-unknown-linux-musl` | `manylinux_2_17_aarch64.manylinux2014_aarch64.musllinux_1_1_aarch64` |
| `x86_64-apple-darwin` | `macosx_10_12_x86_64` |
| `aarch64-apple-darwin` | `macosx_11_0_arm64` |
| `x86_64-pc-windows-msvc` | `win_amd64` |
| `aarch64-pc-windows-msvc` | `win_arm64` |

Each Linux wheel carries a manylinux tag and a musllinux tag, so pip installs it on glibc systems and on musl systems such as Alpine. The binary is a static musl binary and links no system library, so one wheel per Linux architecture works on both, and PyPI gets one wheel per target. Two wheels per architecture would hold the same bytes under two names. The tags are old ones, `manylinux2014` for glibc 2.17 and `musllinux_1_1`, because the binary needs no newer glibc or musl, and older pip versions and distributions accept old tags.

The wheels hold the archive's binary, so the second line of `-version` is `installed by downloading from release page`, as the [`WORKFLINT_INSTALLED_FROM`](#workflint_installed_from) table says.

The jobs that check the wheels:

- `pypi-check` downloads every wheel, runs `check-wheels.sh` on them, and runs `twine check --strict` on their metadata.
- `pypi-smoke` runs `smoke-test-wheel.sh` on each wheel on a runner of its platform, and on the x86_64 Linux wheel in a `python:3-alpine` container. The script installs the wheel with `pip install` into a new virtual environment and with `uvx --from <wheel> workflint`, and runs the smoke test on each. Neither install reads an index.
- `pypi-publish` runs only on a tag run with `PUBLISH` on. PyPI's trusted publisher for `workflint` accepts only this workflow in the `release` environment, so the job runs there, with `id-token: write` and no token. It verifies the archives and their attestations, runs `check-wheels.sh`, and uploads the wheels with `pypa/gh-action-pypi-publish`, which creates a PEP 740 attestation for each. A re-run skips the wheels that PyPI already has.

The build jobs install maturin 1.15.0 from `scripts/release/requirements-maturin.txt`, `pypi-smoke` installs uv from `requirements-uv.txt`, `pypi-check` installs twine from `requirements-twine.txt`, and the `pre-commit` job installs pre-commit from `requirements-pre-commit.txt`. Each file pins every package with its hashes, and pip installs them with `--require-hashes`. The maturin version in `requirements-maturin.txt` and in `[build-system]` of `pyproject.toml` must match.

## Docker images

`docker/Dockerfile` builds the images of `ghcr.io/stokehold/workflint` for `linux/amd64` and `linux/arm64`. Both stages copy the binary of the Linux release archive and compile nothing (ADR 0014).

| Stage | Base image | Contents | Tags of a release `1.2.3` |
| --- | --- | --- | --- |
| `default` | `gcr.io/distroless/static-debian13:nonroot` | `/usr/local/bin/workflint` | `1.2.3`, `1.2`, `latest` |
| `full` | `python:3.14-alpine3.24` | workflint, shellcheck 0.11.0, ruff, and pyflakes | `1.2.3-full`, `1.2-full`, `full` |

A pre-release such as `1.2.3-rc.1` gets only its exact tags, `1.2.3-rc.1` and `1.2.3-rc.1-full`, so `latest` and `full` stay on the last release. `scripts/release/docker-tags.sh` computes the tags.

Both stages set the entrypoint to `/usr/local/bin/workflint`, the working directory to `/repo`, and `WORKFLINT_INSTALLED_FROM` to `installed from Docker image`. They run as uid 65532, the `nonroot` user of distroless, and carry the labels `org.opencontainers.image.source`, `org.opencontainers.image.licenses`, and `org.opencontainers.image.version`. The `full` stage checks the shellcheck release archive against its SHA-256, and pip installs ruff and pyflakes from `docker/requirements-full.txt` with `--require-hashes`. shellcheck is GPL-3.0 and runs only as its own executable (ADR 0017). The image holds its license in `/usr/share/doc/shellcheck/LICENSE.txt` and the URL of its source in `SOURCE.txt` next to it.

Pins:

- The base images in `docker/Dockerfile`, and BuildKit and the SBOM scanner in `docker/build-tools.Dockerfile`, are pinned by digest. Dependabot updates them weekly. `build-tools.Dockerfile` is never built. The `docker-build` job reads the two references from it.
- shellcheck: the version and the two SHA-256 values in the `ADD` lines of `docker/Dockerfile`, and the version in `docker/shellcheck-source.txt`. Dependabot does not update them. The SHA-256 values are the `digest` of each asset of the shellcheck release on GitHub.
- ruff and pyflakes: `docker/requirements-full.txt`, updated by hand with the hash of each musllinux wheel and the pure-Python wheel.

The jobs:

- `docker-build` runs on `ubuntu-24.04` for amd64 and `ubuntu-24.04-arm` for arm64, so the `full` stage installs its packages without emulation. It checks the Linux archive of its architecture with `sha256sum -c` against `checksums.txt`, then builds both stages with `docker buildx build` on a `docker-container` builder, with provenance in `mode=max` and an SBOM attestation, and exports each image as an OCI layout. It loads each layout into Docker with skopeo and runs `check-image.sh`, which runs the smoke test in the image, compares its binary with the archive's, checks that it runs as a non-root user, lints a small repository, and in `full` checks the versions of shellcheck, ruff, and pyflakes and that shellcheck and the Python linter report findings. The job summary lists the size of each image. It has only `contents: read`, no secrets, and no caches, and it pushes nothing.
- `docker-index` merges the two layouts of each stage into one multi-platform layout with `merge-oci.sh`, prints the digest of each image index in the job summary, and checks that `skopeo copy --all --preserve-digests` keeps that digest.
- `docker-attest` runs on tag runs and rehearsals. It attests each index digest under the name `ghcr.io/stokehold/workflint` with `actions/attest-build-provenance`, without pushing the attestation to a registry. A rehearsal verifies each one with `gh attestation verify` on the index blob of the layout, a file whose SHA-256 is the image's digest, so nothing has to be pushed first.
- `docker-publish` runs only on a tag run with `PUBLISH` on, after the `release` job, in the `release` environment, with `contents: read` and `packages: write`. It verifies the attestations again, logs in to GHCR with the job's `GITHUB_TOKEN`, pushes each layout under each of its tags with `skopeo copy --all --preserve-digests`, and fails unless every pushed tag has the attested digest.

To verify a pushed image:

```sh
gh attestation verify oci://ghcr.io/stokehold/workflint:<version> --repo stokehold/workflint
```

To build an image locally, put a Linux binary of each architecture you build in `<dir>/linux_amd64/workflint` or `<dir>/linux_arm64/workflint`:

```sh
docker buildx build docker --build-context dist=<dir> --build-arg VERSION=<version> --target full --load -t workflint:full
scripts/release/check-image.sh workflint:full full <archive> <version>
```

## npm packages

The npm package `workflint` has a launcher and no binary. Each binary is in a platform package of its own, which `workflint` lists as an optional dependency at its exact version (ADR 0014):

| Archive | Package | `os` | `cpu` |
| --- | --- | --- | --- |
| `linux_amd64` | `@stokehold/workflint-linux-x64` | `linux` | `x64` |
| `linux_arm64` | `@stokehold/workflint-linux-arm64` | `linux` | `arm64` |
| `darwin_amd64` | `@stokehold/workflint-darwin-x64` | `darwin` | `x64` |
| `darwin_arm64` | `@stokehold/workflint-darwin-arm64` | `darwin` | `arm64` |
| `windows_amd64` | `@stokehold/workflint-win32-x64` | `win32` | `x64` |
| `windows_arm64` | `@stokehold/workflint-win32-arm64` | `win32` | `arm64` |

npm, pnpm, and Bun install only the platform package whose `os` and `cpu` match the machine. No package has an install script, so `--ignore-scripts` and the defaults of pnpm and Bun, which block dependencies' scripts, change nothing. The Linux binaries are static musl binaries, so the Linux packages set no `libc` field and install on glibc and musl systems.

`npm/workflint/bin/workflint.js` is the launcher. It picks the package for `process.platform` and `process.arch`, finds it with `require.resolve`, and runs its binary with `spawnSync` and the launcher's standard streams. It exits with the binary's exit code. When a signal kills the binary, the launcher kills itself with the same signal, or exits with 128 plus the signal number when node ignores that signal, as it does SIGPIPE. It sets `WORKFLINT_INSTALLED_FROM` to `installed from npm` unless the variable is set, even to an empty value. On a platform without a package, or when the platform package is missing, as after `npm install --omit=optional`, it names the package it expected, says how to fix the install, and exits with code 3. `npm/workflint/test/launcher.test.js` tests these cases with `node --test`.

The packages in git have no version. `npm/workflint/` holds the launcher, its `package.json`, and its `README.md`, and `npm/platform/` holds the `package.json` and `README.md` templates of the platform packages. `scripts/release/npm-pack.sh` writes the version of the `plan` job into all seven `package.json` files and pins the optional dependencies at it. It copies each binary from its archive unchanged, and `LICENSE` and `NOTICE` from the repository root.

The jobs:

- `npm-pack` runs on Ubuntu in every mode and builds no Rust. It downloads the archives and `checksums.txt`, checks the archives against `checksums.txt`, runs `npm-pack.sh`, and runs `check-npm.sh`, which fails unless each platform binary has the SHA-256 of the binary in its archive, each platform `package.json` has the `os` and `cpu` of the table, and no `package.json` has `scripts`. It records the SHA-256 of each tarball as a job output and uploads the tarballs as the artifact `npm-packages`.
- `npm-smoke` runs on `ubuntu-latest`, `ubuntu-24.04-arm`, `macos-latest`, and `windows-latest`. It runs the launcher's tests, then `smoke-test-npm.sh`. That script serves the tarballs from `npm-registry.mjs` on 127.0.0.1 and installs `workflint` into new projects with `npm install`, `npm install --ignore-scripts`, `pnpm add`, and `bun add`. Each package manager resolves the optional dependencies itself, and the script checks that it installed only this machine's platform package. It runs the smoke test through `npx`, `pnpm exec`, and `bunx` (`bun x`), checks that the second line of `-version` is `installed from npm`, and checks that `npm install --omit=optional` gives the launcher's message and code 3.
- `npm-publish` runs only on a tag run with `PUBLISH` on, after the `release` job, in the `release` environment with `id-token: write` and no token. It verifies the archives and their attestations, checks the tarballs against the SHA-256 values of `npm-pack` and with `check-npm.sh`, and runs `npm-publish.sh`. That publishes the six platform packages and then `workflint`, so `workflint` never names a platform package that npm does not have yet. The scoped packages get `--access public`, and a pre-release version gets the dist-tag `next` instead of `latest`. A re-run skips each `name@version` that `npm view` already returns, so it finishes a publish that failed halfway.

npm trusted publishing needs npm 11.5.1 or later, Node 22.14.0 or later, a GitHub-hosted runner, and a `repository.url` in each `package.json` that matches the repository exactly, `git+https://github.com/stokehold/workflint.git`. The jobs use Node 24.22.1 from `actions/setup-node`, which comes with npm 11.19.1, and `npm-publish` fails on an older npm. setup-node gets no `registry-url`, so it writes no `.npmrc` with a token line. The trusted publishers themselves are an item of the [release-day checklist](#release-day-checklist).

`npm-smoke` installs pnpm 12.14.0 and Bun 1.4.3 from `scripts/release/npm-tools/package-lock.json` with `npm ci --ignore-scripts`, so npm checks each package against its integrity hash, and runs the binaries from their platform packages, since the wrappers need install scripts. To update them, change the versions in `scripts/release/npm-tools/package.json` and run `npm install --package-lock-only --ignore-scripts` in that directory.

npx reads some single-dash flags as its own, so `npx workflint -version` prints npm's version. `npx workflint -- -version`, `npx -- workflint -version`, and `npm exec -- workflint -version` pass the flags to workflint.

To build and check the packages locally from a directory of archives and `checksums.txt`:

```sh
scripts/release/npm-pack.sh <version> dist npm-dist
scripts/release/check-npm.sh npm-dist dist <version>
scripts/release/smoke-test-npm.sh npm-dist <version>
node --test "npm/workflint/test/*.test.js"
```

## Homebrew

`brew install stokehold/tap/workflint` installs workflint from the formula `workflint` in [`stokehold/homebrew-tap`](https://github.com/stokehold/homebrew-tap), the tap that every stokehold tool shares (ADR 0014). It is a formula, not a cask, so macOS does not quarantine the binary.

`packaging/homebrew/workflint.rb.tmpl` is the formula with placeholders, and `scripts/release/homebrew-formula.sh <version> <checksums.txt> <base URL>` fills them in. The formula has an `on_macos` block and an `on_linux` block, each with an `on_arm` and an `on_intel` block that holds the URL of one archive, `<base URL>/<archive>`, and its SHA-256 from `checksums.txt`. The script fails unless `checksums.txt` has exactly one well-formed line for each of the four archives, and accepts only an `https://` or `file:///` base URL. The formula:

- Puts the binary in `libexec` and writes `bin/workflint` as a script that sets `WORKFLINT_INSTALLED_FROM` to `installed via Homebrew` and runs it.
- Depends on `shellcheck`. workflint runs shellcheck on `run:` scripts by default (ADR 0012), and without it every run warns that it skipped them.
- Tests that `-version` prints the version and `installed via Homebrew`, that a clean workflow exits with 0, and that a workflow with an unquoted variable in `run:` exits with 1 and reports SC2086, which only shellcheck finds.

The jobs:

- `homebrew` runs `scripts/release/test-homebrew.sh` in every mode on `macos-latest`, `macos-15-intel`, and `ubuntu-latest`, with only `contents: read` and no secrets. No release exists to download from, so the script renders the formula with `file://` URLs of the run's archives. It adds the formula to a new local tap made with `brew tap-new`, runs `brew style` and `brew audit --strict` on it, installs it, runs `brew test`, runs the smoke test on the installed `workflint`, and checks that the second line of `-version` is `installed via Homebrew`. The Ubuntu image has Homebrew in `/home/linuxbrew/.linuxbrew`, and the job puts it on the `PATH`.
- `homebrew-update` runs only on a tag run with `PUBLISH` on, after the `release` job and `homebrew`, in the `release` environment, with `permissions: {}`. It downloads `checksums.txt` from the published release, verifies its attestation for the tag, checks that it is the run's `checksums.txt`, and renders the formula with the release's download URLs. It mints a stokehold-bot token for `homebrew-tap` alone, with `contents: write` and `pull-requests: write`, and runs `scripts/release/homebrew-update.sh`.

`homebrew-update.sh` commits `Formula/workflint.rb` on the branch `workflint-<version>` through the contents API, so GitHub signs the commit and its author is `stokehold-bot[bot]`. It opens the pull request `workflint <version>` and turns on auto-merge with the squash method. The tap's `tests.yml` runs `brew style`, `brew audit --strict`, `brew install`, and `brew test` on the formula on the same three runners, and the ruleset on the tap's `main` merges the pull request once they pass. A re-run for the same version does nothing when `main` already has the formula. Otherwise it reuses the branch, commits only when the branch's formula differs, and opens no second pull request when one is open.

To test the formula locally from a directory of archives and `checksums.txt`, on a machine with Homebrew:

```sh
scripts/release/test-homebrew.sh dist <version>
```

## pre-commit hooks

There are two hooks, both with actionlint's filters `files: ^\.github/workflows/` and `types: [yaml]`, and with `require_serial: true`, so pre-commit runs one workflint process on all the files. workflint lints files in parallel itself and resolves reusable workflows across files.

- `workflint-system`, in `.pre-commit-hooks.yaml` of this repository, is a `language: system` hook that runs the `workflint` on the `PATH`.
- `workflint`, in the mirror repository [`stokehold/workflint-pre-commit`](https://github.com/stokehold/workflint-pre-commit), is a `language: python` hook. pre-commit installs the mirror as a Python package with no code, whose `pyproject.toml` depends on one pinned version of the PyPI package `workflint`. Each tag `v<version>` of the mirror pins workflint `<version>`, so `pre-commit autoupdate` moves users to the latest release.

The `pre-commit` job of the release workflow tests both hooks in every mode, with `scripts/release/test-pre-commit.sh`. That script runs `pre-commit try-repo` on a new project with a valid workflow, a workflow with an error, and a YAML file with the same error outside `.github/workflows/`, and checks that the hook passes the first, fails the second with workflint's message, and skips the third. The job runs it twice:

- on this repository's `workflint-system` hook, with the x86_64 Linux binary of the run on the `PATH`;
- on the mirror at `main`, after `scripts/set-version.sh` in the mirror pins this run's version, with `PIP_FIND_LINKS` set to the run's wheels and `PIP_NO_INDEX=1`. pip builds the mirror's package with setuptools, so the job downloads setuptools from `scripts/release/requirements-setuptools.txt` next to the wheels. The test reads nothing from PyPI, so it works before 0.1.0.

The job installs pre-commit from `scripts/release/requirements-pre-commit.txt` with `--require-hashes`. The setuptools version in `requirements-setuptools.txt` must match `requires` in the mirror's `pyproject.toml`.

On a tag run with `PUBLISH` on, the `pre-commit-dispatch` job mints a stokehold-bot token for the mirror alone, with `contents: write`, and sends a `repository_dispatch` event of type `workflint-release` with `{"version": "<version>"}`. The mirror's workflows then:

1. `update.yml` checks the version against `^[0-9]+\.[0-9]+\.[0-9]+([a-z0-9.+-]*)?$` and does nothing when the mirror already pins it. It waits up to ten minutes for PyPI to list platform wheels for the version, then, in the mirror's `automation` environment, commits the new pin and the `rev:` lines of the mirror's `README.md` to the branch `update/v<version>`, opens a pull request, and turns on auto-merge. A daily run, without an event, pins the latest version on PyPI, so a missed event costs at most a day.
2. `test.yml` runs the hook on the pull request on Linux, macOS, and Windows, against PyPI, and the pull request merges.
3. `tag.yml` runs on the merge. When the pinned version has no tag and PyPI has its platform wheels, it waits for the maintainer to approve the mirror's `release` environment, then creates the tag `v<version>` and a GitHub release for it.

PyPI has a placeholder `workflint` 0.0.1 with no binary, which the mirror pins until 0.1.0. The mirror's workflows never pin, test against PyPI, or tag a version without platform wheels, so the placeholder gets no tag.

To preview an update, run `update.yml` by hand with a version and `dry-run` checked. It prints the diff it would commit and opens no pull request. `dry-run` is checked by default.

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

## Install scripts

`scripts/install.sh` and `scripts/install.ps1` install workflint from a release (ADR 0014). The release copies install their own version by default, and the copies on `main` install the latest release.

```sh
curl -fsSL https://github.com/stokehold/workflint/releases/latest/download/install.sh | bash
bash <(curl -fsSL https://github.com/stokehold/workflint/releases/latest/download/install.sh) latest ./bin
```

```powershell
irm https://github.com/stokehold/workflint/releases/latest/download/install.ps1 | iex
& ([scriptblock]::Create((irm https://github.com/stokehold/workflint/releases/latest/download/install.ps1))) -Version latest -Dir .\bin
```

To move from actionlint's download script, replace its URL. `latest`, `DIR`, and the `executable` step output work as before, and a pinned version becomes a workflint version:

```sh
# before
bash <(curl -fsSL https://raw.githubusercontent.com/rhysd/actionlint/main/scripts/download-actionlint.bash) latest ./bin
# after
bash <(curl -fsSL https://github.com/stokehold/workflint/releases/latest/download/install.sh) latest ./bin
```

| `install.sh` | `install.ps1` | Meaning |
| --- | --- | --- |
| `VERSION` or `--version VERSION` | `-Version VERSION` | A version such as `0.1.0` or `v0.1.0`, or `latest`. |
| `DIR` or `--dir DIR` | `-Dir DIR` | The directory for the binary, created if missing. The default is the current directory. |
| `--require-attestation` | `-RequireAttestation` | Fail when the attestation check is skipped. `WORKFLINT_REQUIRE_ATTESTATION=1` does the same. |

`install.sh` runs with bash 3.2 and later on Linux, macOS, and Git Bash on Windows. `install.ps1` runs with Windows PowerShell 5.1 and PowerShell 7 on Windows, and with PowerShell 7 on Linux and macOS. Each script:

1. Picks the archive for the OS and architecture. On Windows on Arm it reads the CPU from `PROCESSOR_ARCHITEW6432` and `PROCESSOR_IDENTIFIER`, because Git Bash runs as x86_64 there. Under Rosetta 2 on macOS it picks `arm64`.
2. Resolves `latest` from the `Location` header of the redirect at `https://github.com/stokehold/workflint/releases/latest`, without the GitHub API and its rate limit.
3. Downloads the archive and `checksums.txt` into a temporary directory, with three attempts. `install.sh` uses curl, or wget when curl is missing.
4. Checks the archive's SHA-256 against `checksums.txt`, with `sha256sum`, `shasum -a 256`, `certutil`, or `Get-FileHash`. A mismatch or a missing line fails with both checksums in the message, and installs nothing.
5. Runs `gh attestation verify <archive> --repo stokehold/workflint --signer-workflow stokehold/workflint/.github/workflows/release.yml` when `gh` is installed and logged in, and fails if it fails. Otherwise it prints one line that says it skipped the check, with the command to run by hand.
6. Extracts the binary into `DIR` and prints its absolute path on standard output. When `GITHUB_OUTPUT` is set, it also appends `executable=<path>` there. Progress and errors go to standard error.

`WORKFLINT_DOWNLOAD_URL` replaces the base URL `https://github.com/stokehold/workflint/releases/download`, for a mirror. The files come from `<base URL>/v<version>/`, and `latest` comes from the redirect of the base URL with `/download` replaced by `/latest`. The URL changes where the files come from, never whether they are checked.

The `install-scripts` jobs run `scripts/release/test-install.sh` on Ubuntu, macOS, and Windows in every mode. No release exists before M5.10, so the test serves the run's assets from 127.0.0.1 with `serve.py` and points `WORKFLINT_DOWNLOAD_URL` at it. `serve.py` also answers `releases/latest` with a redirect, as github.com does. To run the test locally, put an archive for your platform, `checksums.txt`, and the release copies of the scripts in one directory:

```sh
scripts/release/install-scripts.sh <version> dist
(cd dist && sha256sum -- * > checksums.txt)
scripts/release/test-install.sh dist <version>
```

## Smoke test

`scripts/release/smoke-test.sh <binary> <version>` runs `<binary> -version` and fails unless it exits with 0 and prints `<version>` on its first line. Every channel runs it on the binary it installs. Each build job runs it on the binary from its archive, and then checks that the second line of `-version` is the build-time value of `WORKFLINT_INSTALLED_FROM`. `smoke-test-wheel.sh` runs it on the binary that pip installs from a wheel and on `uvx --from <wheel> workflint`, and `smoke-test-npm.sh` on `workflint` through `npx`, `pnpm exec`, and `bunx`. `check-image.sh` runs it on `docker run --rm <image>`.

## `WORKFLINT_INSTALLED_FROM`

The second line of `workflint -version` says how workflint was installed (ADR 0014). The release pipeline sets `WORKFLINT_INSTALLED_FROM` when it builds, and the binary reads it with `option_env!`. A launcher can override it at run time with an environment variable of the same name. A binary built without it prints `installed by building from source`.

| Channel | Value |
| --- | --- |
| GitHub release archives | `installed by downloading from release page` |
| Install scripts | `installed by downloading from release page`, from the archive that they install |
| PyPI wheels | `installed by downloading from release page`, because a wheel holds the archive's binary. ADR 0014 lists the npm launcher, the Docker image, and the Homebrew formula as the only channels that set their own value. |
| Docker images | `installed from Docker image`, from `ENV` in `docker/Dockerfile`, which overrides the archive binary's value at run time |
| npm | `installed from npm`, which the launcher sets at run time unless `WORKFLINT_INSTALLED_FROM` is already set |
| Homebrew | `installed via Homebrew`, which `bin/workflint`, the formula's env script, sets at run time |

The tasks that add the other channels add their values here.

## The release scripts

The workflow's logic lives in `scripts/release/`, where it can be tested:

| Script | Purpose |
| --- | --- |
| `changes.sh` | Tells whether a list of changed files touches `packaging-paths.txt`. |
| `packaging-paths.txt` | The files that a release depends on. A pull request that changes one runs the dry run. |
| `check-publish-guard.py` | The [publishing guard](#the-publishing-guard). |
| `requirements-pyyaml.txt` | The pinned PyYAML that `check-publish-guard.py` needs. |
| `dry-run-result.sh` | Decides the result of `release-dry-run` and writes its summary. |
| `plan.sh` | Chooses the mode, checks the tag against the version, and reads the `PUBLISH` switch. |
| `package.sh` | Packs a binary with `README.md`, `LICENSE`, and `NOTICE` into an archive. |
| `smoke-test.sh` | The smoke test above. |
| `check-assets.sh` | Checks the set of assets, the archive contents, static linking, `checksums.txt`, and the SBOM. |
| `compare-wheel.sh` | Checks the files in a wheel, and that its binary is the binary of the archive of the same target. |
| `check-wheels.sh` | Checks that there is one wheel per platform tag, and each wheel against the SHA-256 that its build job recorded. |
| `smoke-test-wheel.sh` | Installs a wheel with pip and with uvx and runs the smoke test on each. |
| `npm-pack.sh` | Writes the seven npm packages of a version from the archives and packs them. |
| `check-npm.sh` | Checks the npm tarballs: their files and `package.json` fields, and each binary against its archive. |
| `smoke-test-npm.sh` | Installs the npm tarballs with npm, pnpm, and Bun from `npm-registry.mjs` and runs the smoke test on each. |
| `npm-registry.mjs` | Serves the npm tarballs to `smoke-test-npm.sh` as a registry on 127.0.0.1. |
| `npm-publish.sh` | Publishes the npm tarballs, platform packages first, and skips the ones npm already has. |
| `homebrew-formula.sh` | Writes the Homebrew formula of a version from `packaging/homebrew/workflint.rb.tmpl` and `checksums.txt`. |
| `test-homebrew.sh` | Installs the formula from a local tap with `file://` URLs, and runs `brew style`, `brew audit --strict`, `brew test`, and the smoke test. |
| `homebrew-update.sh` | Opens or updates the pull request in `stokehold/homebrew-tap` with the formula, and turns on its auto-merge. |
| `summary.sh` | Writes the job summary. |
| `docker-tags.sh` | Prints the tags of the default or the full image for a version. |
| `merge-oci.sh` | Merges the OCI layouts of the two architectures into one multi-platform layout and prints its digest. |
| `check-image.sh` | Checks a Docker image against the release archive of its architecture. |
| `notes.sh` | Reads the release notes from `CHANGELOG.md`. |
| `install-scripts.sh` | Writes the release copies of `scripts/install.sh` and `scripts/install.ps1`, which install the release's version by default. |
| `test.sh` | Tests `plan.sh`, `notes.sh`, `smoke-test.sh`, `package.sh`, `install-scripts.sh`, `compare-wheel.sh`, `check-wheels.sh`, `docker-tags.sh`, `merge-oci.sh`, `npm-pack.sh`, `check-npm.sh`, `npm-publish.sh`, `homebrew-formula.sh`, `homebrew-update.sh`, `changes.sh`, `dry-run-result.sh`, and `check-publish-guard.py`. |
| `test-install.sh` | Tests the install scripts against a directory of assets. |
| `test-pre-commit.sh` | Runs a pre-commit hook with `pre-commit try-repo` on a valid workflow, an invalid one, and a file outside `.github/workflows/`. |
| `serve.py` | Serves the assets to `test-install.sh` on 127.0.0.1. |

The `plan` job runs shellcheck on the scripts and `install.sh`, PSScriptAnalyzer on `install.ps1`, and `test.sh` before it plans. To run them locally:

```sh
shellcheck scripts/release/*.sh scripts/install.sh
pwsh -c 'Invoke-ScriptAnalyzer -Path scripts/install.ps1'
python3 -m venv /tmp/pyyaml
/tmp/pyyaml/bin/pip install --no-deps --only-binary :all: --require-hashes -r scripts/release/requirements-pyyaml.txt
PYTHON=/tmp/pyyaml/bin/python scripts/release/test.sh
```

## release-plz

release-plz chooses the next version and writes the changelog (ADR 0014). `.github/workflows/release-plz.yml` keeps a release pull request open with the version bump and the new section of `CHANGELOG.md`. Merging that pull request pushes the tag `v<version>`, and the tag starts the release workflow. release-plz itself never publishes anything.

### `RELEASES_ENABLED`

The repository variable `RELEASES_ENABLED` is the switch of release-plz in the table of [switches](#publishing-is-off-until-m510). It is unset, and only M5.10, Release 0.1.0 ([#258](https://github.com/stokehold/workflint/issues/258)), sets it to `true`. Until then:

- A push to `main` skips both jobs of `release-plz.yml`. No job pushes a tag.
- A manual run opens a release pull request as a rehearsal. The maintainer checks it and closes it without merging.
- Nobody creates or sets `RELEASES_ENABLED`, and nobody pushes a `v*` tag by hand. A `v*` tag can never be moved or deleted, so a tag pushed early is permanent.

### The jobs

| Job | Runs on | What it does |
| --- | --- | --- |
| `release-pr` | A push to `main` when `RELEASES_ENABLED` is `true`, and every manual run on `main` | Runs `release-plz release-pr`, which opens the release pull request or updates the open one. |
| `release` | A push to `main` when `RELEASES_ENABLED` is `true` | Runs `release-plz release`. When the pushed commit is the merge of a release pull request, it pushes the tag `v<version>`. For any other commit it does nothing. |
| `release-plz update`, in `gate.yml` | A pull request that changes `release-plz.toml` or `scripts/release-plz/` | Runs `release-plz update` without pushing, and writes the changes to `CHANGELOG.md` and the `Cargo.toml` files to the job summary. It needs no token. |

On a push, `release-pr` waits for `release`, so that it sees the tag of a release pull request that just merged. A newer push cancels a running `release-pr`. Each run of `release` has its own concurrency group, so a later push can neither cancel it nor replace it while it waits.

Both jobs run in the `automation` environment, which deploys only from `main`. Each one mints a token for the `stokehold-bot` App that only works on this repository. `release-pr` gets `contents: write` and `pull-requests: write`, and `release` gets `contents: write` and `pull-requests: read`. A pull request or a tag that the `GITHUB_TOKEN` creates starts no workflows, so with that token CI would not run on the release pull request and the tag would not start the release workflow. release-plz commits to the release branch and creates the tag through the GitHub API, so the checkouts keep no credentials. GitHub signs the commits that release-plz makes on the release branch.

`scripts/release-plz/install.sh` downloads release-plz 0.3.172 and checks its SHA-256. Dependabot does not update it. To update release-plz, change the version and the checksum in the script. The checksum is the `digest` of the `x86_64-unknown-linux-gnu` asset of the release on GitHub.

### What `release-plz.toml` sets

- `git_only = true`: release-plz reads the previous version from the tag `v<version>` and never runs `cargo publish`. crates.io publishing starts at 1.0 (ADR 0006).
- `git_release_enable = false`: the release workflow creates the GitHub release with its assets. A release that release-plz created would have no assets, and an immutable release can never get them.
- `release_always = false`: `release-plz release` tags only the merge of its own pull request. It recognises one by its branch name, which starts with `release-plz-`.
- `semver_check = false`: the binary has no library API, and the internal crates promise no stability before 1.0.
- Only the `workflint` package pushes a tag, `v{{ version }}`. Every crate shares the version through `version.workspace = true`, so the release pull request bumps the version in the root `Cargo.toml` and the version requirements between the crates.
- `changelog_include` lists the internal crates. A commit that changes only `workflint-rules` still appears in the changelog and still counts for the version bump.
- `release_commits`: only a commit of a type that the changelog lists, or a breaking change, opens a release pull request. A `chore(deps)` bump or a `ci` change ships with the next release.
- The changelog is `CHANGELOG.md` in the repository root. Its sections are Breaking changes, Features, Bug fixes, Performance, and Documentation.

[CONTRIBUTING.md](../CONTRIBUTING.md#commit-messages) lists the commit types and what each one does to the changelog and the version.

release-plz counts a commit only if it changes a file of a crate, that is, a file that `cargo package --list` lists. A change to `docs/` or `.github/` alone never starts a release.

`pr_name` sets the title of the release pull request to `chore: release v<version>`, which passes the `pr-title` check. The pull request runs every required check, like any other.

### Before the first tag

release-plz finds the previous version in the newest tag `v<version>`. Before the first release there is no such tag, so release-plz treats the next release as the first one. It keeps the version in `Cargo.toml` as it is, and the changelog section covers up to 1000 commits of the history. `changelog_include` takes effect only once a previous release exists, so that first section lists only the commits that changed the `workflint` package itself.

### Cutting 0.1.0

M5.10 does these steps:

1. Sets the version to 0.1.0 in a pull request of its own, `chore: set the version to 0.1.0`. It changes `version` in `[workspace.package]` of the root `Cargo.toml` and the version requirements between the crates, then runs `cargo update --workspace`. release-plz then proposes 0.1.0 as the first release. Do not push the version change to the release pull request: release-plz closes a release pull request that has commits by anyone else and opens a new one.
2. Sets the variable: `gh variable set RELEASES_ENABLED --body true --repo stokehold/workflint`.
3. The next push to `main`, or a manual run, opens `chore: release v0.1.0`. Check the version and the changelog section in it.
4. Merging it pushes `v0.1.0`, and the release workflow builds the release from that tag.

### Rehearsing before M5.10

1. Run the workflow on `main`: `gh workflow run release-plz.yml --repo stokehold/workflint --ref main`.
2. Check the release pull request: `stokehold-bot[bot]` is its author, every required check and `pr-title` run on it, and the merge box shows that the ruleset lets it be squash-merged.
3. Close it without merging, and delete its branch. Merging it would put the version bump and the changelog on `main` without a tag.

To try a change to `release-plz.toml` without the workflow, run `release-plz update` in a clean checkout. It writes the changes to the working tree and pushes nothing. The `release-plz update` job does the same on a pull request.
