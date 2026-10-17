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

The command checks the Go version before it builds. With another Go 1.21 or later installed, `GOTOOLCHAIN=go1.26.4 cargo xtask reference` makes it download Go 1.26.4. It stops with an error, and changes nothing, when the checkout has local changes or untracked files that its `.gitignore` does not ignore. It builds the binary again when the commit, the Go version, or a Go build variable such as `GOFLAGS`, `GOOS`, or `GOARCH` changes. The corpus is up to date when every file has the content of its archive entry. `docs/research/README.md` lists the other projects under `.reference/`, which you clone by hand.

The test suite does not need `.reference/`. It reads a copy of the reference's test data in `tests/conformance/actionlint/`, which is in git. `cargo xtask vendor-testdata` recreates that copy from the checkout, and the `README.md` there lists the files it holds.

The conformance suite, the `conformance` test target of `xtask`, lints each of those fixtures with the `workflint` binary. `tests/conformance/README.md` describes how it runs them and the files that list expected failures and allowed differences. Fixtures that need shellcheck or pyflakes are skipped when the tool is not on `PATH`, so install both to run every fixture.

After a change to `crates/workflint-yaml/`, `crates/workflint-diagnostics/`, or `xtask/goyaml-dump/`, run:

```sh
cargo xtask yaml-parity
```

It builds a small Go program with go-yaml, the YAML library of the reference, and compares the trees of both parsers on the vendored test data, the 1,503 corpus files, and `tests/yaml-parity/`. Every node must have the same kind, position, explicit tag, anchor, and alias name, and every scalar the same style and decoded value. A file that go-yaml rejects must fail in workflint too, at any position, with the same message after the prefixes `yaml:`, `line N:`, and `could not parse as YAML:`. The one exception is an encoding error that workflint reports where go-yaml reports a syntax error (`docs/differences.md#yaml-error-positions`). A few files in `tests/yaml-parity/` differ as an entry of `docs/differences.md` describes, and its `README.md` lists them. The command prints a diff for each file that differs and exits with an error. It needs Go 1.26.4, not `.reference/`. Pass file or directory paths to compare only those. CI runs it when these paths change.

## Oracle

`cargo xtask oracle` runs workflint and the reference binary on the same inputs and compares their output (ADR 0004). It needs git and Go 1.26.4 to build the reference, and builds it first if `.reference/` lacks it.

```sh
cargo xtask oracle
cargo xtask oracle tests/conformance/actionlint/testdata/err/one_error.yaml
```

The inputs are the vendored `err`, `examples`, `ok`, and `projects` fixtures and the 1,503 corpus files. Both binaries run with `-oneline -no-color -shellcheck= -pyflakes=` and an empty config file, in a temporary project, as the conformance runner runs them (`tests/conformance/README.md`). Each fixture is one run, and the corpus is one run over all its files. The command groups the output by file, replaces each rule ID in workflint's output with its actionlint kind from `tests/conformance/rule-kinds.toml`, applies `tests/conformance/allowed-differences.toml` to the reference's output, and compares the lines and the exit codes.

It prints a summary and a diff for each file that differs, and writes the same Markdown to `target/oracle/report.md`, with the summary alone in `target/oracle/summary.md`. It exits with an error if a difference is not in the allow-list. `--report-only` exits with success after writing the report.

`--kind KIND` compares only the diagnostics of that actionlint kind, and repeating it adds kinds. A milestone uses it to show that workflint matches the reference on the checks it has built, while the checks of other kinds are still missing:

```sh
cargo xtask oracle --kind syntax-check --report-only
```

The kinds are the values in `tests/conformance/rule-kinds.toml`, and any other name is an error. The command keeps a line of either output when its kind is one of the given kinds: the kind in the reference's brackets, or the kind that workflint's rule ID maps to. It filters the `.out` lines and the lines of each allow-list record the same way, so a record for another kind is neither applied nor reported as stale (`tests/conformance/README.md#allowed-differences`). A line without a kind stays, such as a line without a position or a rule ID missing from `rule-kinds.toml`. Exit codes 0 and 1 count as equal, because the lines of the other kinds decide them. A fatal error or a signal still counts as another exit code. The report and the summary name the kinds.

Pass fixture files, project directories, directories of fixtures, the corpus archive, or files in `.reference/realworld` to compare only those. The `cargo xtask oracle` job of CI runs the command with `--report-only` when a change touches `crates/`, `xtask/`, `tests/conformance/`, `.go-version`, `Cargo.toml`, `Cargo.lock`, or `rust-toolchain.toml`, and adds the summary to the run page. M5.9 removes `--report-only`, so that an unexplained difference fails the job. The job first runs the hidden `--self-check` flag, which runs the reference in place of workflint without the kind mapping or the allow-list and fails on any difference. That shows the runs are deterministic, and the job log keeps its report. Then the job runs the conformance suite with the reference binary (`tests/conformance/README.md#running-the-reference`), which shows that the runner works. Every run of the command skips the three fixtures on which the reference's output varies between runs (`docs/differences.md#cached-metadata-errors`), and the report lists them.

After the report, the job compares the `syntax-check` kind on the four fixture directories, and an unexplained difference fails the job:

```sh
cargo xtask oracle --kind syntax-check tests/conformance/actionlint/testdata/{err,examples,ok,projects}
```

The corpus stays out of this step until #49 adds its known `syntax-check` differences to the allow-list. Last, the job unpacks the corpus with `cargo xtask reference` and runs the two ignored corpus tests, `every_corpus_file_passes_the_fuzz_check` in `workflint-syntax` and `every_corpus_file_passes_the_fuzz_checks` in `workflint-yaml`:

```sh
cargo nextest run --run-ignored only -p workflint-yaml -p workflint-syntax -E 'binary(reference_files)'
```

## Benchmarks

`cargo xtask bench` compares a release build of workflint with the reference binary on the three scenarios in ADR 0019: the corpus with external tools off, the corpus with shellcheck on, and `testdata/bench/small.yaml`. It needs everything `cargo xtask reference` needs, plus [hyperfine](https://github.com/sharkdp/hyperfine) 2, and shellcheck 0.11.0 for the shellcheck scenario. CI uses hyperfine 2.0.0.

```sh
cargo install --locked hyperfine@2.0.0
cargo xtask bench
```

The command writes `target/bench/results.json` and `target/bench/results.md`, with wall time and peak memory for each scenario and binary and the ratio against each target. The output of every run and hyperfine's exports go to `target/bench/runs/`. Without shellcheck 0.11.0, the command skips the shellcheck scenario and says why.

Each run has a time limit: 2 minutes for the corpus, 10 minutes for the corpus with shellcheck, and 10 seconds for `small.yaml`. The binary first runs once on its own under that limit. When that run exceeds it, the command kills the run and the processes it started, records the binary as timed out, and goes on with the next one. Otherwise hyperfine runs the warmup and measured runs, and the limit applies to the whole series: hyperfine gets the limit once for each of its runs, so one slow run can use the time that the others leave unused. `--timeout SECONDS` sets one limit for all scenarios. On macOS the reference can hang with shellcheck on, so `cargo xtask bench --timeout 60` gives a quicker result there.

When every scenario times out, one binary takes about 86 minutes. The nightly job has a 200-minute timeout, which covers both binaries and the builds, and a test in `xtask/src/bench/scenario.rs` fails if a change to the limits or the runs no longer fits.

A measurement fails when a binary exits with a code other than 0 or 1, or when hyperfine fails. The command still writes the results, then exits with an error.

The `Benchmarks` workflow runs the command every night on Linux and appends the results to `history.jsonl` on the `benchmarks` branch. It opens or updates an issue labelled `performance` when a measurement failed, or when, in any scenario, workflint's wall time divided by the reference's from the same run grows by more than 10% against the latest scheduled run in the history. Dividing by the reference cancels most of the speed difference between runners. A run started by hand goes into the history too, but later runs are not compared with it. `cargo xtask bench-history` does that comparison, and it says when a scenario of the previous scheduled run has no result in the new one. After a failed measurement, the job records the results and opens the issue, then fails.

## Micro-benchmarks

Hot paths have micro-benchmarks written with [divan](https://docs.rs/divan) (ADR 0019). They live in each crate's `benches/` directory:

- `crates/workflint-yaml/benches/parse.rs` parses `minimal.yaml`, `small.yaml`, `large.yaml`, and `many_scripts.yaml` to events and to the span tree.
- `crates/workflint-diagnostics/benches/source.rs` builds the line index of `large.yaml` and converts the offset of each of its characters to a line and column.
- `crates/workflint-expr/benches/expressions.rs` lexes, parses, and type-checks the 1,858 expressions of `expressions.txt`. `check` type-checks trees parsed before the measurement, and `parse_and_check` lexes, parses, and type-checks in one pass. Divide the reported time by 1,858 for the time per expression.
- `crates/workflint-syntax/benches/parse.rs` parses the same four files as `workflint-yaml` and the largest corpus file, `mozilla_DeepSpeech_.github_workflows_build-and-test.yml`, into a `Workflow`. `workflow` starts from a span tree built before the measurement, and `tree_and_workflow` builds the span tree too, as the binary does for each file. The benchmark reads the corpus file from `dataset.zip`.

The inputs are the reference's benchmark files in `tests/conformance/actionlint/testdata/bench/`, and its corpus archive in `tests/conformance/actionlint/testdata/realworld/`. They sit outside the crates, so the published packages do not contain them (ADR 0006).

Run them locally with `cargo bench`:

```sh
cargo bench -p workflint-yaml
cargo bench -p workflint-diagnostics
cargo bench -p workflint-expr
cargo bench -p workflint-syntax
cargo bench -p workflint-yaml -- tree
```

The last command runs only the benchmarks whose name contains `tree`. Locally, divan measures wall time and prints a table.

The crates depend on `codspeed-divan-compat` under the name `divan`, so a benchmark imports `divan` as usual. In CI, the `CodSpeed` workflow builds the benchmarks with `cargo codspeed build` and runs them under CodSpeed, which counts instructions instead of measuring wall time. It runs on every pull request and on every push to `main`, and a maintainer can start it by hand from the Actions tab. CodSpeed comments on each pull request with the change against `main`. To add a benchmark target, add a `[[bench]]` section with `harness = false` to the crate's `Cargo.toml`.

## Fuzzing

`fuzz/` holds the fuzz targets for [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz). It is a separate workspace with its own `Cargo.lock`, because `cargo fuzz` needs a nightly toolchain. Install cargo-fuzz with `cargo install --locked cargo-fuzz@0.13.2`.

| Target | What it checks |
| --- | --- |
| `expr_check` | `workflint_expr::TypeEnv::check` type-checks each input that parses against the global contexts without panicking, and a second check gives the same type and errors. Every error has a semantic rule and a range inside the expression on character boundaries. |
| `expr_lex` | The expression lexer of `workflint-expr` reads the input as the text after `${{` up to the `}}` that ends it or the first error, without panicking and without looping. Every token and error range lies inside the input on character boundaries, tokens never overlap, and `lex_expression` returns the same tokens and error. |
| `expr_parse` | `workflint_expr::parse_expression` parses the input as the text after `${{` without panicking. Every node's range lies inside its parent's on character boundaries, and so do its token and names. The visitor enters and leaves each node once. An error range covers at most one character or one token. |
| `glob` | `workflint_syntax::glob` validates the input as a branch or tag pattern and as a path pattern without panicking. Every problem has a message, each range covers one character of the input, and the ranges never move backwards. |
| `render` | `workflint-diagnostics` builds a `SourceFile` from the input text and renders diagnostics with arbitrary spans in the default and one-line formats without panicking. The output is UTF-8, has one header per diagnostic and well-formed snippets, and differs between color on and off only in escape sequences. The comment at the top of `fuzz/fuzz_targets/render.rs` describes the input layout. |
| `syntax` | `workflint_syntax::parse` parses the tree of the input without panicking. Every diagnostic has a span inside the input on character boundaries that converts to a line and column. The section parsers that later tasks add run under the same target. |
| `yaml_events` | The event parser of `workflint-yaml` reads the input up to the end of the stream or the first error without panicking. Every event's marks lie inside the input on character boundaries. |
| `yaml_tree` | `workflint_yaml::parse` builds the tree without panicking. Every span lies inside the input on character boundaries and converts to a line and column, and every node ID in the tree refers to a node. Every scalar's source map is exact and maps each character, and each range of characters, inside the scalar's span. The written items and entries, the duplicate keys, and the comments of each node agree with the tree. |

The two YAML targets first decode the input with `workflint_yaml::decode`, as the linter reads a file, so UTF-16 input and encoding errors are covered. An input that does not decode, and the input of the other six targets, has invalid UTF-8 replaced with U+FFFD. The two YAML targets run the checks in `crates/workflint-yaml/tests/invariants/mod.rs`, `syntax` runs the check in `crates/workflint-syntax/tests/invariants/mod.rs`, and the three expression targets run the checks in `crates/workflint-expr/tests/invariants/mod.rs`. Write the seed inputs first, then start a target from the repository root:

```sh
cargo xtask fuzz-seeds
cargo +nightly fuzz run yaml_tree -- -max_total_time=300
```

`cargo xtask fuzz-seeds` copies the YAML files of the vendored test data, the 1,503 workflows in its corpus archive, the YAML test suite inputs, and the fuzz regression inputs into `fuzz/corpus/<target>/`, which git ignores. The expression targets get other seeds: each line of `testdata/bench/expressions.txt` followed by `}}`, the text after each `${{` in the YAML files of the vendored test data, and the inputs in `crates/workflint-expr/tests/fuzz-regressions/`. `cargo fuzz` keeps the inputs it finds in the same directory. `fuzz/rust-toolchain.toml` pins the nightly that CI uses. Run `cargo fuzz` inside `fuzz/` without `+nightly` to use it.

CI checks the formatting of the targets, runs clippy on them, and builds them when a change touches `workflint-data`, `workflint-diagnostics`, `workflint-expr`, `workflint-syntax`, `workflint-yaml`, `fuzz/`, or the root `Cargo.toml`. Every night the `Fuzz` workflow runs each target for five minutes. A target fails the run when an input panics or takes longer than 10 seconds, and the run uploads that input as the artifact `fuzz-<target>`. A scheduled run that fails opens an issue titled "Nightly fuzzing found a failure", or comments on the open one.

Each run starts from the corpus of the latest run on `main` and adds the seeds. At the end it uploads `fuzz/corpus/<target>/` as the artifact `fuzz-corpus-<target>`, which GitHub keeps for 14 days. A failed run uploads its corpus too. When no run on `main` kept a corpus in that time, the target starts from the seeds alone.

To turn a failing input into a regression test:

1. Download the artifact, or take the input from `fuzz/artifacts/<target>/` after a local run. `cargo fuzz` names it `crash-<hash>` for a panic and `timeout-<hash>` for a hang.
2. Reproduce the failure with `cargo +nightly fuzz run yaml_tree <input>`. `cargo +nightly fuzz tmin yaml_tree <input>` writes a smaller input that fails the same way.
3. Copy the input into `crates/workflint-yaml/tests/fuzz-regressions/`, under a name that says what it covers, such as `tab-before-flow-key`. `cargo nextest run -p workflint-yaml fuzz_regression` runs both targets' checks on every file there, so it now fails. An input of an expression target goes into `crates/workflint-expr/tests/fuzz-regressions/` instead, and `cargo nextest run -p workflint-expr fuzz_regression` runs the three expression checks on it.
4. Fix the bug in the crate, or fix the check in its `tests/invariants/mod.rs` if the check is wrong. The file stays as a regression test, and `cargo xtask fuzz-seeds` adds it to the seed inputs.

## Continuous integration

Two workflows run on every pull request and on every push to `main`.

`ci.yml` holds the checks that run on every change: `rustfmt`, `clippy`, `test` on four runners, `cargo-deny`, `msrv`, and `workflow-security`. `msrv` runs `cargo check --workspace --all-targets` with the toolchain in the `rust-version` field of `Cargo.toml`. The `main` branch ruleset requires each of these jobs by name.

`gate.yml` holds the checks that only matter when certain files change, and the job `ci-gate`, which the ruleset requires too. A required check cannot come from a workflow with a `paths:` filter. When the filter does not match, the workflow does not run, its jobs report no status, and the required check waits forever. So `gate.yml` has no `paths:` filter, and chooses the jobs itself in four steps.

1. The job `changes` lists the changed files. For a pull request, it compares the merge commit with its first parent, the tip of the base branch. For a push, it compares the pushed commit with the commit before the push. The job fetches two commits, so after a push of more than one commit it lacks that commit, and every job runs.
2. It matches the files against one list of patterns for each job. The lists are in the `env` of `changes`, in the syntax of a `paths:` filter. Each list contains `.github/workflows/gate.yml`, so a change to the workflow runs every job.
3. Each job runs only if its list matches.
4. `ci-gate` needs every other job, and runs even when one of them fails or the run is cancelled. It fails unless `changes` and `rustdoc` passed and each other job passed or was skipped.

| Job in `gate.yml` | Runs when a change touches |
| --- | --- |
| `cargo xtask oracle` | `crates/`, `xtask/`, `tests/conformance/`, `.go-version`, the Cargo files, `rust-toolchain.toml` |
| `cargo xtask yaml-parity` | `workflint-yaml`, `workflint-diagnostics`, the parts of `xtask/` that the comparison uses, `tests/yaml-parity/`, the vendored test data, `.go-version`, `Cargo.lock`, `rust-toolchain.toml` |
| `cargo xtask reference` | `xtask/`, `.go-version`, `.gitattributes`, the vendored test data, the Cargo files, `rust-toolchain.toml` |
| `cargo fuzz build` | the five library crates that the fuzz targets use, `fuzz/`, the root `Cargo.toml` |
| `release timing` | `crates/`, the Cargo files, `rust-toolchain.toml` |
| `rustdoc` | every change |

`release timing` runs the tests in `crates/workflint-yaml/tests/event_parser.rs` and `crates/workflint-syntax/tests/billion_laughs.rs` in a release build, where the parser timing tests must finish in under one second. A debug build allows them 5 and 10 seconds. `rustdoc` runs `cargo doc --workspace --no-deps --document-private-items` with `RUSTDOCFLAGS=-D warnings`.

To add a job that runs only for some files, add its pattern list to the `env` of `changes`, add its name to the loop that writes the outputs, and add the job to the `needs` of `ci-gate`. A job in `gate.yml` that runs on every change, as `rustdoc` does, also goes into the `ci-gate` step next to `changes` and `rustdoc`, so that a skipped run fails the gate.

## Pull requests

- One issue per pull request. Open it as a draft with `Closes #N` in the description as soon as you start, so others can see the task is taken.
- The pull request title follows Conventional Commits and names the task, for example `feat(expr): M3.2 Pratt parser`. It becomes the squash commit message.
- Write the failing test before the fix or feature.
- Before you open the pull request, run the checks below. They are the checks in the "Done" list of `AGENTS.md`, and CI runs them too. Clippy must report no warnings, and `cargo nextest run --workspace` includes the conformance suite. Install cargo-deny with `cargo install --locked cargo-deny`.

  ```sh
  cargo fmt --all --check
  cargo clippy --workspace --all-targets
  cargo nextest run --workspace
  cargo test --doc --workspace
  cargo deny check
  cargo deny --manifest-path fuzz/Cargo.toml check
  ```

- If your change makes workflint differ from actionlint, add an entry to `docs/differences.md`. If the difference changes a fixture's output, also add records to `tests/conformance/allowed-differences.toml` (ADR 0023).
- Say in the pull request how you verified the change.

AI-assisted contributions are welcome. `AGENTS.md` has the instructions for coding agents, and the same rules apply to people.

## License

By contributing, you agree that your contributions are licensed under the MIT license.
