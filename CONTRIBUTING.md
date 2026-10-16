# Contributing to workflint

workflint is early in development. The plan lives in `docs/roadmap.md`, and the decisions behind it live in `docs/adr/`.

## Before you start

- Tasks are GitHub issues, grouped in milestones (ADR 0020). Pick one with no open blockers and no linked pull request, or comment on the issue first if you are unsure.
- Open an issue before large changes that no task covers, so the work matches the roadmap.
- Read `docs/architecture.md` and `docs/glossary.md`.
- Check the ADR index in `docs/adr/README.md`. A change that contradicts an accepted ADR needs a new ADR that supersedes it.

## Setting up

You need [rustup](https://rustup.rs). The repository pins its toolchain in `rust-toolchain.toml`, and rustup installs it on first use.

The tests run with [cargo-nextest](https://nexte.st) 0.9.148 or newer, the version CI uses. `.config/nextest.toml` sets that minimum, and older versions stop with an error. Install it with `cargo install --locked cargo-nextest`, or download a prebuilt binary as the nextest documentation describes.

```sh
cargo run -p workflint
cargo nextest run --workspace
cargo test --doc --workspace
```

nextest does not run doctests, so the last command runs them. CI runs nextest with `--profile ci`, defined in `.config/nextest.toml`, which runs every test instead of stopping at the first failure.

`cargo xtask --help` lists the developer commands.

Parity work also needs the reference checkout of actionlint, a reference binary built from it, and its test corpus. They live in `.reference/`, which git ignores. Install git and Go 1.26.4, the version in `.go-version`, then create them:

```sh
cargo xtask reference
```

The command checks the Go version before it builds. With another Go 1.21 or later installed, `GOTOOLCHAIN=go1.26.4 cargo xtask reference` makes it download Go 1.26.4. `docs/research/README.md` lists the other projects under `.reference/`, which you clone by hand. The conformance tooling added in milestone M1 will also need shellcheck.

### Benchmarks

`cargo xtask bench` compares a release build of workflint with the reference binary on the three scenarios in ADR 0019: the corpus with external tools off, the corpus with shellcheck on, and `testdata/bench/small.yaml`. It needs everything `cargo xtask reference` needs, plus [hyperfine](https://github.com/sharkdp/hyperfine) 2, and shellcheck 0.11.0 for the shellcheck scenario. CI uses hyperfine 2.0.0.

```sh
cargo install --locked hyperfine@2.0.0
cargo xtask bench
```

The command writes `target/bench/results.json` and `target/bench/results.md`, with wall time and peak memory for each scenario and binary and the ratio against each target. The output of every run and hyperfine's exports go to `target/bench/runs/`. Without shellcheck 0.11.0, the command skips the shellcheck scenario and says why.

Each run has a time limit: 2 minutes for the corpus, 10 minutes for the corpus with shellcheck, and 10 seconds for `small.yaml`. When a run exceeds it, the command kills the run and the processes it started, records the run as timed out, and goes on with the next one. `--timeout SECONDS` sets one limit for all scenarios. On macOS the reference can hang with shellcheck on, so `cargo xtask bench --timeout 60` gives a quicker result there.

The `Benchmarks` workflow runs the command every night on Linux and appends the results to `history.jsonl` on the `benchmarks` branch. It opens or updates an issue labelled `performance` when, in any scenario, workflint's wall time divided by the reference's from the same run grows by more than 10% against the previous run. Dividing by the reference cancels most of the speed difference between runners. `cargo xtask bench-history` does that comparison.

The test suite does not need `.reference/`. It reads a copy of the reference's test data in `tests/conformance/actionlint/`, which is in git. `cargo xtask vendor-testdata` recreates that copy from the checkout, and the `README.md` there lists the files it holds.

## Pull requests

- One issue per pull request. Open it as a draft with `Closes #N` in the description as soon as you start, so others can see the task is taken.
- The pull request title follows Conventional Commits and names the task, for example `feat(expr): M3.2 Pratt parser`. It becomes the squash commit message.
- Write the failing test before the fix or feature.
- Before you open the pull request, run `cargo fmt --all`, `cargo clippy --workspace --all-targets`, `cargo nextest run --workspace`, and `cargo test --doc --workspace`.
- If your change makes workflint differ from actionlint, add an entry to `docs/differences.md`.
- Say in the pull request how you verified the change.

AI-assisted contributions are welcome. `AGENTS.md` has the instructions for coding agents, and the same rules apply to people.

## License

By contributing, you agree that your contributions are licensed under the MIT license.
