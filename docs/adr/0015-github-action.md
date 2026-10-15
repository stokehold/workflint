# ADR 0015: Official GitHub Action

- Status: accepted. Under ADR 0021, each action release waits for the maintainer's approval of its `release` environment, so new workflint versions reach the action after that approval rather than automatically.
- Date: 2026-10-15

## Context

actionlint never had an official GitHub Action. Its maintainer declined one because action tags would conflict with the tool's own release tags (rhysd/actionlint#117, #262). Users fall back to a download script without checksum verification, or to third-party actions. Most of those actions run Docker (Linux only, slow to start), install packages at run time, or download binaries without verifying them (`docs/research/10-github-action.md`).

## Decision

Repository and type:

- The action lives in its own repository, `stokehold/workflint-action`, with its own tags.
- It is a composite action that runs one bash script. Bash is available on every GitHub-hosted runner, including Windows, so the action works on Linux, macOS, and Windows on x64 and arm64.

Installing workflint:

- Each action release pins a default workflint version and commits the SHA-256 of every release archive for that version. The script downloads the archive for the runner's OS and architecture and checks it against the committed hash. Pinning the action by commit SHA therefore pins the workflint binary too.
- The `version` input may request another version. The script then verifies the archive with `gh attestation verify` against the `stokehold/workflint` repository, and fails if verification fails.
- The binary is cached in the runner's tool cache.

Inputs:

- `version`: the workflint version. Default: the pinned version.
- `args`: extra arguments, for example paths.
- `config`: a path to a config file.
- `fail-on`: `error`, `warning`, `info`, or `never`. Default: `error`.
- `annotations`: whether to emit inline annotations. Default: `true`.
- `summary`: whether to write a job summary. Default: `true`.
- `sarif-file`: a path to write SARIF to. Default: none.
- `working-directory`: where to run.

Outputs:

- `exit-code`, `error-count`, `warning-count`, and `sarif-file`.

Results:

- workflint itself writes the annotations (`--format github`), the job summary, and the SARIF file in one run. The action contains no output formatting of its own.
- GitHub shows only 10 annotations per type per step. workflint emits the most severe findings first and ends with a notice that points to the job summary, which lists everything.
- Uploading SARIF to code scanning is a separate, documented step that users add with `github/codeql-action/upload-sarif`. The action does not nest other actions, so it works on GitHub Enterprise Server and needs no extra Dependabot updates.
- Annotation mode, SARIF output, and failure behaviour are independent. Whether the step fails depends only on `fail-on`.

Permissions:

- The action needs `contents: read`. The SARIF upload step needs `security-events: write`.

Releases:

- A workflint release triggers a bot pull request in the action repository that bumps the default version and the committed hashes. The test matrix runs, and the pull request merges automatically when it passes.
- Action releases are immutable GitHub releases. The release workflow moves the `v1` tag to the latest compatible release.
- The test matrix covers Ubuntu on x64 and arm64, `ubuntu-slim`, macOS on arm64 and x64, Windows on x64 and arm64, and an Alpine container job.

## Consequences

Users get a verified binary on every runner OS with no Docker or package-manager step. New workflint versions reach the action within hours through the automated bump.

The action repository needs its own release automation, which is planned in M6.
