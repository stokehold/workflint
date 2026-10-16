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

The test suite does not need `.reference/`. It reads a copy of the reference's test data in `tests/conformance/actionlint/`, which is in git. `cargo xtask vendor-testdata` recreates that copy from the checkout, and the `README.md` there lists the files it holds.

## Micro-benchmarks

Hot paths have micro-benchmarks written with [divan](https://docs.rs/divan) (ADR 0019). They live in each crate's `benches/` directory:

- `crates/workflint-yaml/benches/parse.rs` parses `minimal.yaml`, `small.yaml`, and `large.yaml` to events and to the span tree.
- `crates/workflint-diagnostics/benches/source.rs` builds the line index of `large.yaml` and converts the offset of each of its characters to a line and column.

The inputs are the reference's benchmark files in `tests/conformance/actionlint/testdata/bench/`. They sit outside the crates, so the published packages do not contain them (ADR 0006).

Run them locally with `cargo bench`:

```sh
cargo bench -p workflint-yaml
cargo bench -p workflint-diagnostics
cargo bench -p workflint-yaml -- tree
```

The last command runs only the benchmarks whose name contains `tree`. Locally, divan measures wall time and prints a table.

The crates depend on `codspeed-divan-compat` under the name `divan`, so a benchmark imports `divan` as usual. In CI, the `CodSpeed` workflow builds the benchmarks with `cargo codspeed build` and runs them under CodSpeed, which counts instructions instead of measuring wall time. It runs on every pull request and on every push to `main`, and CodSpeed comments on the pull request with the change against `main`. To add a benchmark target, add a `[[bench]]` section with `harness = false` to the crate's `Cargo.toml`.

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
