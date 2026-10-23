# Releasing workflint

`.github/workflows/release.yml` builds workflint for six targets, packs each build into an archive, writes `checksums.txt`, an SBOM, and the install scripts, attests every asset, and publishes them as a GitHub release (ADR 0014, ADR 0021). Every other channel repackages these assets. The same workflow builds the [PyPI wheels](#pypi-wheels) and publishes them to PyPI, and builds the [Docker images](#docker-images) and pushes them to GHCR.

## Publishing is off until M5.10

Nothing publishes yet. M5.10, Release 0.1.0 ([#258](https://github.com/stokehold/workflint/issues/258)), turns publishing on. Until then every channel runs as a dry run only: this workflow, and the later M6 tasks for the install scripts, npm, PyPI, Docker, Homebrew, pre-commit, and the GitHub Action. No job pushes a tag, uploads to a registry, or creates a GitHub release. The only job that uses a secret is `release-pr` in `release-plz.yml`, which mints a stokehold-bot token on a manual run to open a rehearsal release pull request ([release-plz](#release-plz)). Rehearsals and tag runs create attestations. They are stored in the repository and in Sigstore's public transparency log, and they are not releases.

The release workflow has a switch, `PUBLISH: "false"` in its top-level `env`. A tag run builds, checks, and attests the assets, builds and checks the wheels, builds, checks, and attests the Docker images, and skips the `release`, `pypi-publish`, and `docker-publish` jobs.

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
| Docker images | `PUBLISH` in `.github/workflows/release.yml`, which also gates the `docker-publish` job | off |

## Release-day checklist

The maintainer does these steps by hand before a release. Each item says when.

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

- [ ] **The `workflint` container package public and linked, after the first `docker-publish` run.** A package that a workflow pushes for the first time is private. In the organization's Packages, open `workflint`, then Package settings: set the visibility to public, and check that the package is linked to `stokehold/workflint`. Until then `docker pull ghcr.io/stokehold/workflint` fails for everyone else.

## How a release happens

1. release-plz opens a release pull request with the version bump and the changelog ([release-plz](#release-plz)). Merging it pushes the tag `v<version>`.
2. The tag starts the release workflow. The `plan` job reads the version of the `workflint` package with `cargo metadata` and fails unless the tag is `v` followed by that version. When it fails, no other job runs.
3. The `build` jobs build and pack the six archives and build the six wheels, and the `sbom` job writes the SBOM. The `checksums` job writes the release copies of the install scripts, then `checksums.txt`, and checks every asset. The `install-scripts` jobs test the install scripts against these assets. The `attest` job attests each asset. The `gate` job runs `cargo test --workspace --locked`.
4. The `release` job waits for the maintainer to approve the deployment to the `release` environment.
5. It checks the assets, their checksums, and their attestations, creates a draft release for the tag with every asset, and then publishes the draft. The notes are the version's section of `CHANGELOG.md`, or notes that GitHub generates when the changelog has no such section.
6. The `pypi-publish` job waits for a second approval of the `release` environment. It checks the assets and their attestations again, checks each wheel against the SHA-256 that its build job recorded, and uploads the wheels to PyPI.
7. The `docker-publish` job waits for its own approval of the `release` environment. It verifies the attestations of the two images and pushes them to GHCR ([Docker images](#docker-images)).

Releases are immutable. Once published, a release cannot gain, lose, or change an asset, so the workflow uploads every asset while the release is a draft. The install scripts go up in the same step as the archives. A later task that adds a release asset uploads it in the `release` job before the publishing step. Channels that publish somewhere else, such as npm, PyPI, Docker, and Homebrew, run after the release is published.

## Modes

| Mode | Started by | Jobs |
| --- | --- | --- |
| Dry run | A pull request that changes `.github/workflows/release.yml`, `scripts/release/`, `scripts/install.sh`, `scripts/install.ps1`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `crates/workflint/Cargo.toml`, `pyproject.toml`, or `docker/` | `plan`, `build`, `sbom`, `checksums`, `install-scripts`, `pypi-check`, `pypi-smoke`, `docker-build`, `docker-index` |
| Rehearsal | A manual run on `main`: `gh workflow run release.yml --repo stokehold/workflint --ref main` | The dry run, `gate`, `attest`, and `docker-attest`, which also verify each attestation with `gh attestation verify` |
| Release | A push of the tag `v<version>` | Every job. `release`, `pypi-publish`, and `docker-publish` run only when `PUBLISH` is `"true"` |

Each run uploads the assets as workflow artifacts, and the `checksums` job lists each file with its size and SHA-256 in the job summary. A rehearsal from another branch fails in `plan`.

The build jobs have only `contents: read`, no secrets, no environment, and no caches, and they build with `--locked`. Only `attest` and `docker-attest` have `attestations: write`, only they and `pypi-publish` have `id-token: write`, only `release` has `contents: write`, and only `docker-publish` has `packages: write`.

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

The build jobs install maturin 1.15.0 from `scripts/release/requirements-maturin.txt`, `pypi-smoke` installs uv from `requirements-uv.txt`, and `pypi-check` installs twine from `requirements-twine.txt`. Each file pins every package with its hashes, and pip installs them with `--require-hashes`. The maturin version in `requirements-maturin.txt` and in `[build-system]` of `pyproject.toml` must match.

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

`scripts/release/smoke-test.sh <binary> <version>` runs `<binary> -version` and fails unless it exits with 0 and prints `<version>` on its first line. Every channel runs it on the binary it installs. Each build job runs it on the binary from its archive, and then checks that the second line of `-version` is the build-time value of `WORKFLINT_INSTALLED_FROM`. `smoke-test-wheel.sh` runs it on the binary that pip installs from a wheel and on `uvx --from <wheel> workflint`. `check-image.sh` runs it on `docker run --rm <image>`.

## `WORKFLINT_INSTALLED_FROM`

The second line of `workflint -version` says how workflint was installed (ADR 0014). The release pipeline sets `WORKFLINT_INSTALLED_FROM` when it builds, and the binary reads it with `option_env!`. A launcher can override it at run time with an environment variable of the same name. A binary built without it prints `installed by building from source`.

| Channel | Value |
| --- | --- |
| GitHub release archives | `installed by downloading from release page` |
| Install scripts | `installed by downloading from release page`, from the archive that they install |
| PyPI wheels | `installed by downloading from release page`, because a wheel holds the archive's binary. ADR 0014 lists the npm launcher, the Docker image, and the Homebrew formula as the only channels that set their own value. |
| Docker images | `installed from Docker image`, from `ENV` in `docker/Dockerfile`, which overrides the archive binary's value at run time |

The tasks that add the other channels add their values here.

## The release scripts

The workflow's logic lives in `scripts/release/`, where it can be tested:

| Script | Purpose |
| --- | --- |
| `plan.sh` | Chooses the mode, checks the tag against the version, and reads the `PUBLISH` switch. |
| `package.sh` | Packs a binary with `README.md`, `LICENSE`, and `NOTICE` into an archive. |
| `smoke-test.sh` | The smoke test above. |
| `check-assets.sh` | Checks the set of assets, the archive contents, static linking, `checksums.txt`, and the SBOM. |
| `compare-wheel.sh` | Checks the files in a wheel, and that its binary is the binary of the archive of the same target. |
| `check-wheels.sh` | Checks that there is one wheel per platform tag, and each wheel against the SHA-256 that its build job recorded. |
| `smoke-test-wheel.sh` | Installs a wheel with pip and with uvx and runs the smoke test on each. |
| `summary.sh` | Writes the job summary. |
| `docker-tags.sh` | Prints the tags of the default or the full image for a version. |
| `merge-oci.sh` | Merges the OCI layouts of the two architectures into one multi-platform layout and prints its digest. |
| `check-image.sh` | Checks a Docker image against the release archive of its architecture. |
| `notes.sh` | Reads the release notes from `CHANGELOG.md`. |
| `install-scripts.sh` | Writes the release copies of `scripts/install.sh` and `scripts/install.ps1`, which install the release's version by default. |
| `test.sh` | Tests `plan.sh`, `notes.sh`, `smoke-test.sh`, `package.sh`, `install-scripts.sh`, `compare-wheel.sh`, `check-wheels.sh`, `docker-tags.sh`, and `merge-oci.sh`. |
| `test-install.sh` | Tests the install scripts against a directory of assets. |
| `serve.py` | Serves the assets to `test-install.sh` on 127.0.0.1. |

The `plan` job runs shellcheck on the scripts and `install.sh`, PSScriptAnalyzer on `install.ps1`, and `test.sh` before it plans. To run them locally:

```sh
shellcheck scripts/release/*.sh scripts/install.sh
pwsh -c 'Invoke-ScriptAnalyzer -Path scripts/install.ps1'
scripts/release/test.sh
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
