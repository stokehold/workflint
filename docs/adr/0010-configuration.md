# ADR 0010: Configuration

- Status: accepted
- Date: 2026-10-15

## Context

actionlint reads `.github/actionlint.yaml`, which has three settings: self-hosted runner labels, known configuration variables, and per-path ignore patterns. It silently ignores unknown keys, so a misspelled key does nothing (rhysd/actionlint#554). Path globs match against the path relative to the current directory, not the repository root, so ignores stop working when run from a subdirectory (`docs/research/04-cli-output-distribution.md`).

workflint adds settings for rules, severities, external tools, and data extensions.

## Decision

Format and location:

- The config file is YAML, because workflint's users write YAML every day.
- workflint looks for config in this order and uses the first file it finds, without merging:
  1. The path given by `-config-file` or `--config`.
  2. `.github/workflint.yaml`, then `.github/workflint.yml`, in the project root.
  3. `.github/actionlint.yaml`, then `.github/actionlint.yml`, in the project root.
  4. The user config at `$XDG_CONFIG_HOME/workflint/config.yaml` (`%APPDATA%\workflint\config.yaml` on Windows).
- `--no-config` skips all of them.
- `-init-config` writes `.github/workflint.yaml`.

Project root:

- The project root is the nearest ancestor of the linted file that has a `.github/workflows` directory and a version control marker (`.git`, `.jj`, or `.hg`). If no ancestor has a marker, it is the nearest ancestor with a `.github` directory.
- Root checks compare whole path components, so `/work/repo` does not contain `/work/repo-other`.

Schema:

- The workflint schema is a superset of actionlint's: `self-hosted-runner.labels`, `config-variables`, and `paths.<glob>.ignore` keep their meaning.
- workflint adds keys such as `rules`, `shellcheck`, `python`, `known-secrets`, `runner-labels`, and `require-ignore-reason`. Each key is documented in `docs/config.md` when it ships.
- In a workflint config file, an unknown key is a fatal error (exit code 3) that names the key and its position.
- In an actionlint config file, an unknown key is a warning, because actionlint accepted such files.
- Path globs in `paths` and `rules` match against the path relative to the project root, with `/` separators on every OS.
- The JSON Schema is generated from the Rust config types and published with each release. `workflint config schema` prints it.

Later:

- `extends` for sharing config across repositories is planned after 1.0.
- TOML and `pyproject.toml` support are not planned.

## Consequences

actionlint users switch without touching their config. Typos in workflint config fail loudly. Editors can validate the file through the published JSON Schema.

A file found first wins completely, so a user config never changes results in a repository that has its own config. This keeps CI and local runs consistent.
