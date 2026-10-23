# Releasing workflint

`.github/workflows/release.yml` builds workflint for six targets, packs each build into an archive, writes `checksums.txt` and an SBOM, attests every asset, and publishes them as a GitHub release (ADR 0014, ADR 0021). Every other channel repackages these assets.

## Publishing is off until M5.10

Nothing publishes yet. M5.10, Release 0.1.0 ([#258](https://github.com/stokehold/workflint/issues/258)), turns publishing on. Until then every channel runs as a dry run only: this workflow, and the later M6 tasks for the install scripts, npm, PyPI, Docker, Homebrew, pre-commit, and the GitHub Action. No job pushes a tag, uploads to a registry, or creates a GitHub release. The only job that uses a secret is `release-pr` in `release-plz.yml`, which mints a stokehold-bot token on a manual run to open a rehearsal release pull request ([release-plz](#release-plz)). Rehearsals and tag runs create attestations. They are stored in the repository and in Sigstore's public transparency log, and they are not releases.

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
| Release tag, pushed by release-plz | The repository variable `RELEASES_ENABLED`, read by `.github/workflows/release-plz.yml` | off, unset |

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

1. release-plz opens a release pull request with the version bump and the changelog ([release-plz](#release-plz)). Merging it pushes the tag `v<version>`.
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
