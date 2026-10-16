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

The command checks the Go version before it builds. With another Go 1.21 or later installed, `GOTOOLCHAIN=go1.26.4 cargo xtask reference` makes it download Go 1.26.4. `docs/research/README.md` lists the other projects under `.reference/`, which you clone by hand.

The test suite does not need `.reference/`. It reads a copy of the reference's test data in `tests/conformance/actionlint/`, which is in git. `cargo xtask vendor-testdata` recreates that copy from the checkout, and the `README.md` there lists the files it holds.

The conformance suite, the `conformance` test target of `xtask`, lints each of those fixtures with the `workflint` binary. `tests/conformance/README.md` describes how it runs them and the files that list expected failures and allowed differences. Fixtures that need shellcheck or pyflakes are skipped when the tool is not on `PATH`, so install both to run every fixture.

## Fuzzing

`fuzz/` holds the fuzz targets for [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz). It is a separate workspace with its own `Cargo.lock`, because `cargo fuzz` needs a nightly toolchain. Install cargo-fuzz with `cargo install --locked cargo-fuzz@0.13.2`.

| Target | What it checks |
| --- | --- |
| `yaml_events` | The event parser of `workflint-yaml` reads the input up to the end of the stream or the first error without panicking. Every event's marks lie inside the input on character boundaries. |
| `yaml_tree` | `workflint_yaml::parse` builds the tree without panicking. Every span lies inside the input on character boundaries and converts to a line and column, and every node ID in the tree refers to a node. Every scalar's source map is exact and maps each character inside the scalar's span. |

Both targets replace invalid UTF-8 with U+FFFD and run the checks in `crates/workflint-yaml/tests/invariants/mod.rs`. Write the seed inputs first, then start a target from the repository root:

```sh
cargo xtask fuzz-seeds
cargo +nightly fuzz run yaml_tree -- -max_total_time=300
```

`cargo xtask fuzz-seeds` copies the YAML files of the vendored test data, the 1,503 workflows in its corpus archive, the YAML test suite inputs, and the fuzz regression inputs into `fuzz/corpus/<target>/`, which git ignores. `cargo fuzz` keeps the inputs it finds in the same directory. `fuzz/rust-toolchain.toml` pins the nightly that CI uses. Run `cargo fuzz` inside `fuzz/` without `+nightly` to use it.

CI builds the targets on pull requests that change `workflint-yaml`, `workflint-diagnostics`, or `fuzz/`. Every night it runs each target for five minutes. A target fails the run when an input panics or takes longer than 10 seconds, and the run uploads that input as the artifact `fuzz-<target>`.

To turn a failing input into a regression test:

1. Download the artifact, or take the input from `fuzz/artifacts/<target>/` after a local run. `cargo fuzz` names it `crash-<hash>` for a panic and `timeout-<hash>` for a hang.
2. Reproduce the failure with `cargo +nightly fuzz run yaml_tree <input>`. `cargo +nightly fuzz tmin yaml_tree <input>` writes a smaller input that fails the same way.
3. Copy the input into `crates/workflint-yaml/tests/fuzz-regressions/`, under a name that says what it covers, such as `tab-before-flow-key`. `cargo nextest run -p workflint-yaml fuzz_regression` runs both targets' checks on every file there, so it now fails.
4. Fix the bug in `workflint-yaml`, or fix the check in `tests/invariants/mod.rs` if the check is wrong. The file stays as a regression test, and `cargo xtask fuzz-seeds` adds it to the seed inputs.

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
