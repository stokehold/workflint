# Contributing to workflint

workflint is early in development. The plan lives in `docs/roadmap.md`, and the decisions behind it live in `docs/adr/`.

## Before you start

- Tasks are GitHub issues, grouped in milestones (ADR 0020). Pick one with no open blockers and no linked pull request, or comment on the issue first if you are unsure.
- Open an issue before large changes that no task covers, so the work matches the roadmap.
- Read `docs/architecture.md` and `docs/glossary.md`.
- Check the ADR index in `docs/adr/README.md`. A change that contradicts an accepted ADR needs a new ADR that supersedes it.

## Setting up

You need [rustup](https://rustup.rs). The repository pins its toolchain in `rust-toolchain.toml`, and rustup installs it on first use.

```sh
cargo run -p workflint
cargo test --workspace
```

Parity work also needs the reference checkout of actionlint and its test corpus. `docs/research/README.md` explains how to recreate `.reference/`. The conformance tooling added in milestone M1 will also need a Go toolchain and shellcheck.

## Pull requests

- One issue per pull request. Open it as a draft with `Closes #N` in the description as soon as you start, so others can see the task is taken.
- The pull request title follows Conventional Commits and names the task, for example `feat(expr): M3.2 Pratt parser`. It becomes the squash commit message.
- Write the failing test before the fix or feature.
- Before you open the pull request, run `cargo fmt --all`, `cargo clippy --workspace --all-targets`, and `cargo test --workspace`.
- If your change makes workflint differ from actionlint, add an entry to `docs/differences.md`.
- Say in the pull request how you verified the change.

AI-assisted contributions are welcome. `AGENTS.md` has the instructions for coding agents, and the same rules apply to people.

## License

By contributing, you agree that your contributions are licensed under the MIT license.
