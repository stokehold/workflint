# Conformance suite

The conformance suite runs every fixture in this directory through the `workflint` binary and compares the output with the fixture's expected errors, using the rules of actionlint's own tests (ADR 0004). The runner is the `conformance` test target of the `xtask` package, in `xtask/tests/conformance.rs`, with its code in `xtask/src/conformance/`. `cargo xtask oracle` uses the same command line, output parsing, matching, kind mapping and allow-list to compare workflint with the reference binary, as `CONTRIBUTING.md` describes.

`cargo test --workspace` and `cargo nextest run --workspace` both run it. To run only the suite:

```sh
cargo nextest run -p xtask --test conformance
```

Each fixture is one test, named after its path, such as `actionlint::err/action_metadata_syntax_validation.yaml`. Three more tests check the data files described below, `parity_checklist::parity-checklist.md` and `checks_md::checks.md` check the boxes of `docs/parity-checklist.md`, and `skipped_fixtures::README.md` prints how many fixtures the runner skips and why. `.config/nextest.toml` shows its output at the end of every nextest run, and fails a conformance test that runs longer than 5 minutes.

The runner finds the reference's fixtures by listing `actionlint/testdata/`. It fails every test that reads them if a suite directory is missing or holds another number of fixtures than the reference has at its commit (`ACTIONLINT_FIXTURES` in `xtask/src/conformance/fixture.rs`), so a missing copy cannot pass with no tests.

## Contents

| Path | What it holds |
|---|---|
| `actionlint/` | The reference's test data, vendored by `cargo xtask vendor-testdata`. Do not edit. |
| `go/time-zones.txt` | The time zone names in the tzdata that the reference binary embeds, written by `cargo xtask vendor-testdata` from the Go in `.go-version`. Do not edit. |
| `workflint/` | workflint's own fixtures, with the same layout as `actionlint/testdata/`. |
| `expected-failures.toml` | Fixtures that do not pass yet, with the milestone expected to make them pass. |
| `milestones.toml` | The milestones whose work is done, which no expected failure may name. |
| `allowed-differences.toml` | Expected lines that workflint changes on purpose, each tied to `docs/differences.md`. |
| `rule-kinds.toml` | The actionlint kind of each workflint rule ID. |

## Fixtures

The runner reads four kinds of fixture from `actionlint/testdata/` and `workflint/`:

| Fixture | Expected errors | Linted files |
|---|---|---|
| `err/<name>.yaml` | `err/<name>.out` | the fixture, as `test.yaml` |
| `examples/<name>.yaml` | `examples/<name>.out` | the fixture, as `test.yaml` |
| `ok/<name>.yaml` | none | the fixture, as `test.yaml` |
| `projects/<name>/` | `projects/<name>.out` (empty means no errors) | every `.yml` and `.yaml` file under `projects/<name>/workflows/` |

A fixture's ID is its path without the `.yaml` extension, except for `ok` fixtures, which keep it: `err/name`, `examples/name`, `ok/name.yaml`, `projects/name`. These are the names that `docs/parity-checklist.md` cites. workflint's own fixtures get a `workflint/` prefix, as in `workflint/err/name`.

### workflint's own fixtures

Add a fixture under `workflint/` when a checklist row says "none" in its fixtures column, or when workflint behaves differently from actionlint on purpose. Use the same layout: `workflint/err/<name>.yaml` with `workflint/err/<name>.out`, `workflint/ok/<name>.yaml`, or `workflint/projects/<name>/` with `workflint/projects/<name>.out`. A `.github` directory next to `err` or `ok` fixtures is copied into the project, as it is for the reference's fixtures.

## Command line

This is the command line the runner uses. The reference binary and `workflint` both accept it. Since M5.6 `workflint` accepts every flag of the reference in the syntax of Go's `flag` package, and a few of its own (`docs/cli.md`).

For each fixture, the runner creates a temporary directory and builds a project in `<temp>/project`:

- For `err`, `examples` and `ok`, it copies the `.github` directory next to the fixture, if there is one, and copies the fixture to `test.yaml`. The reference's tests lint these with the fixture's directory as the project root, and the `.github` directory holds the local actions and reusable workflows that the fixtures call.
- For `projects`, it copies the project directory.
- In both cases it creates `.git` and `.github/workflows` directories, so that both workflint and the reference take `<temp>/project` as the project root.

It writes an empty file to `<temp>/config.yaml`. Then it runs the binary in `<temp>/project`:

```sh
workflint -oneline -no-color -shellcheck=<path> -pyflakes=<path> -config-file <config> <files>...
```

- `-oneline` prints one line per error, the form that [Matching](#matching) parses.
- `-no-color` keeps escape sequences out of the output whatever the environment, as `-color` or a terminal would add them. `NO_COLOR` and `TERM` do not matter with it.
- workflint runs the shellcheck that `-shellcheck` names. It accepts `-pyflakes` and `-config-file`, and ignores them until pyflakes (M5.12, #249) and the config (M5.2, #247) use them.
- The runner passes none of `-ignore`, `-color`, `-verbose`, `-debug`, `-format`, `-summary`, `-quiet`, and `-silent`, so standard output holds only the errors and standard error stays empty.
- `<config>` is `<temp>/project/actionlint.yaml` for a project that has one, and `<temp>/config.yaml` otherwise. Passing a config file means the CLI reads no config from the workflint repository or from the user's config directory.
- `<files>` is `test.yaml`, or a project's workflow files as relative paths with `/` separators, such as `workflows/nested/piyo.yaml`, in the order of Go's `filepath.Walk`. actionlint's CLI does not accept a directory.
- `-shellcheck=` and `-pyflakes=` take the tool's absolute path, or nothing to turn the tool off. As in the reference's tests:
  - `err` and `examples` fixtures turn a tool on only if the file name contains `shellcheck` or `pyflakes`.
  - `ok` fixtures turn on every tool found on `PATH`.
  - `projects` turn both off.
  - A fixture whose file name contains a tool's name is skipped, with a message, if the tool is not on `PATH`. The Linux legs of the CI test job install shellcheck 0.11.0 and the pyflakes version in `requirements.txt`, so these fixtures run there.
- Standard input is empty.
- Exit code 0 means no errors, and exit code 1 means errors were found. The fixture fails, with a message that names the cause, if the binary exits with 0 and prints output, exits with 1 and prints nothing, exits with any other code, is stopped by a signal, or prints anything to standard error. It fails this way even when `expected-failures.toml` lists it, because that file excuses wrong diagnostics, not a crash.
- The runner kills the binary, and its child processes on Unix, after 60 seconds, and fails the fixture.
- The runner runs the binary twice on each fixture: with `RAYON_NUM_THREADS=1`, and without `RAYON_NUM_THREADS`, which gives one thread per CPU. The fixture fails, with a diff, if the two outputs differ, because output must not depend on the thread count (ADR 0013). It fails this way even when `expected-failures.toml` lists it.

The runner builds the binary with `cargo build -p workflint` before the first fixture in each test process. Every `cargo build` deletes `target/debug/workflint` and links it again, so a test that started that path while another test process built could fail with "No such file or directory". The runner copies the binary to `target/conformance/workflint-<hash>` instead, where `<hash>` is a hash of the binary's content, and runs the copy. A change to `crates/workflint` gives a new copy, and the runner then deletes the older copies.

## Matching

The runner compares the output with the expected lines as `checkErrors` in the reference's `linter_test.go` does:

1. Each line of standard output must have the form `path:line:col: message [rule-id]`. A line that does not parse fails the fixture.
2. Backslashes in the path become `/`.
3. The rule ID becomes its actionlint kind, from `rule-kinds.toml`. An unknown rule ID fails the fixture.
4. The errors are sorted by path, line, column and message, as the reference sorts them. workflint sorts by rule ID before message at one position (`docs/differences.md#sort-order`), and this step undoes that.
5. The number of errors must equal the number of expected lines.
6. The n-th expected line must equal the n-th error, written as `path:line:col: message [kind]`. A line that starts and ends with `/` is a regular expression instead, which must match anywhere in the error. The `regex` crate reads these patterns, except that a `{` or `}` that does not form a repetition such as `{2,5}` is escaped first, because Go reads such braces as literals.

`projects/local_action_invalid.out` has patterns that expect the relative path `testdata/projects/local_action_invalid/...`. The reference's Go test passes a relative project root, but a CLI that finds the project root prints an absolute path, so the reference binary fails those lines too. The reference run lists the fixture as a known failure (see below). `allowed-differences.toml` replaces those lines with patterns for the absolute paths that both binaries print (`docs/differences.md#local-action-paths`). The reference prints no line that their `upstream` patterns match, so `cargo xtask oracle` skips the fixture, as it skips every fixture in `REFERENCE_FAILURES`.

## Running the reference

To check the runner itself, run the suite with the reference binary in place of workflint:

```sh
WORKFLINT_CONFORMANCE_REFERENCE=.reference/bin/actionlint cargo nextest run -p xtask --test conformance
```

A relative path is relative to the repository root. In this mode the runner does not map kinds, because the reference prints kinds already. It reads neither `expected-failures.toml` nor `allowed-differences.toml`, and it skips workflint's own fixtures. The only expected failures are the ones in `REFERENCE_FAILURES` in `xtask/src/conformance/runner.rs`, each with its reason: today only `projects/local_action_invalid`. Every other fixture must pass, so a runner that passes or fails fixtures by mistake shows up. The `cargo xtask oracle` job in `.github/workflows/gate.yml` runs this check.

## Expected failures

`expected-failures.toml` lists each fixture that does not pass yet, with the milestone expected to make it pass:

```toml
"err/action_metadata_syntax_validation" = "M4"
"ok/minimal.yaml" = "M5"
```

The milestone is the latest one among the `docs/parity-checklist.md` rows that cite the fixture, or M5 if no row cites it. A row whose notes say it waits on the fixture, as in ``the row waits on `err/name` (M4)``, does not count: its own check is done, and its milestone is the one of its last fixture. Its note gives the fixture's milestone in brackets, and that milestone must match this file.

`milestones.toml` lists the milestones whose work is done. No entry may name one of them: a fixture that a done milestone should have fixed is a bug, or the checklist gives the wrong milestone. A follow-up issue filed in a done milestone does not take the milestone out of the list.

The binary has linted for real since M2.0, so the file lists only fixtures that still fail. A task that makes a fixture fail again, and adds the fixture back to this file, introduces a regression. Fix the regression instead.

- A listed fixture that fails passes its test and prints `expected failure until <milestone>` with the first line of the reason.
- A listed fixture that passes fails its test, with a message that says to remove its line. The pull request that makes a fixture pass removes the line.
- A fixture that is not listed must pass.
- A listed fixture whose binary crashes, hangs, or prints to standard error fails its test.
- The test `expected_failures::expected-failures.toml` fails if a line names a fixture that does not exist, names another milestone than the rule above gives, or names a milestone in `milestones.toml`. It also fails if a note in the checklist says a row waits on a fixture that passes, or gives another milestone than this file.
- The test `parity_checklist::parity-checklist.md` fails if a ticked row cites a fixture that does not exist or is listed here, if an open row has a milestone in `milestones.toml`, or if a row has more or fewer cells than its table's header.
- The test `checks_md::checks.md` lints the `Example input` of each ticked row of the "docs/checks.md sections" table in the default output format, as an `err` fixture named after the section's anchor, so the section `check-shellcheck-integ` runs shellcheck and is skipped, with a message, when shellcheck is not installed. It compares the output with the section's `Output` block. It changes each rule ID to its kind and leaves out the `^~~~` lines, which differ by `docs/differences.md#column-units`. An allow-list record of a fixture in the row's "Upstream fixtures" cell that has both an `upstream` and a `workflint` line replaces that line of the `Output` block, if the block has it, and keeps the snippet under it. So does each `[[example]]` table of the section. A section whose only `Output` block upstream marks `<!-- Skip update output -->` is written by hand and printed by no binary, so the test compares nothing for it, and the row's fixtures check the section.

To see the full reason a listed fixture fails, remove its line and run its test.

## Allowed differences

`allowed-differences.toml` lists the expected lines that workflint replaces, removes, or adds on purpose, in fixtures and in the corpus files of `actionlint/testdata/realworld/dataset.zip`. It has one `[[difference]]` table per changed line:

```toml
[[difference]]
anchor = "yaml-error-positions"
fixture = "err/yaml_syntax_error"
upstream = "test.yaml:3:1: could not parse as YAML: ... [syntax-check]"
workflint = "test.yaml:3:5: could not parse as YAML: ... [syntax-check]"
```

| Key | Meaning |
|---|---|
| `anchor` | An anchor in `docs/differences.md`, without the `#`. Required. |
| `fixture` | The fixture ID, `realworld/<file name>` for a corpus file, or `realworld/*` for every corpus file. Required. |
| `upstream` | An expected line of the fixture's `.out` file, copied exactly, that workflint does not report. For a corpus file, a line of the reference's output for that file. |
| `workflint` | The line workflint reports instead or in addition, in the same form as `.out` lines: with the actionlint kind in brackets, or a `/pattern/`. |

An entry with both `upstream` and `workflint` replaces a line. With only `upstream` it removes one, and with only `workflint` it adds one. The runner inserts each `workflint` line before the first expected line that sorts after it by path, line, column and message. It sorts a pattern line by the position at its start, if it has one, and places the new line after pattern lines at the same position. A pattern line that replaces a line at its own position takes that line's place instead, because its order among the lines at that position is unknown.

The runner fails:

- the test `allowed_differences::allowed-differences.toml` if an anchor is not in `docs/differences.md`, a fixture does not exist, a corpus file is not in the archive, an entry has neither `upstream` nor `workflint`, an `upstream` line is not in the fixture's `.out` file, a line of an entry for a corpus file does not start with that file's name and a position, an entry for every corpus file lacks a valid `/pattern/` or a `workflint`, or the `upstream` line of an `[[example]]` table is not in the `Output` block of its section.
- a fixture that is not in `expected-failures.toml` if it also passes without one of its entries. Remove such an entry.

Every entry needs a matching section in `docs/differences.md`. Write that section first, then the entry.

An `[[example]]` table replaces a line of the `Output` block of a section of upstream `docs/checks.md`, for the test `checks_md::checks.md`. It is for a line that the section's fixtures match with a `/pattern/`, so that the fixtures need no `[[difference]]` entry, while the `Output` block has the whole line:

```toml
[[example]]
anchor = "platform-data-updates"
section = "check-runner-labels"
upstream = 'test.yaml:10:13: label "linux-latest" is unknown. available labels are ... [runner-label]'
workflint = 'test.yaml:10:13: label "linux-latest" is unknown. available labels are ... [runner-label]'
```

`section` is the section's anchor without the `#`. `upstream` and `workflint` are error lines with the kind in brackets. The fixture tests and `cargo xtask oracle` ignore these tables.

`cargo xtask oracle` applies the entries of a fixture to the reference binary's output for that fixture. An `upstream` line must be in the fixture's `.out` file, as in the runner. The oracle removes the first line of the reference's output that this `.out` line matches, exactly or as a `/pattern/`. An entry that does not apply is a problem in the report that names the `.out` file, and the other entries of the fixture still apply.

### Corpus files

The runner does not lint the corpus, so only `cargo xtask oracle` reads the entries for corpus files. It applies them to the reference's output in the corpus run, the entries for single files first. An entry for a corpus file works as a fixture's entry, but its `upstream` line quotes the reference's output, because a corpus file has no `.out` file. It removes the first line of that file that `upstream` matches. Each line of the entry, exact or a `/pattern/`, starts with the file name and a position:

```toml
[[difference]]
anchor = "dispatch-input-key-list"
fixture = "realworld/netdata_netdata_.github_workflows_docker.yml"
upstream = 'netdata_netdata_.github_workflows_docker.yml:11:9: unexpected key "name" for "inputs" section. expected one of "default", "description", "required" [syntax-check]'
workflint = 'netdata_netdata_.github_workflows_docker.yml:11:9: unexpected key "name" for "inputs" section. expected one of "default", "description", "options", "required", "type" [syntax-check]'
```

One entry can cover a difference that changes the same text in many corpus files. Its `fixture` is `realworld/*`, its `upstream` is a `/pattern/`, and its `workflint` is the text that replaces each match of the pattern in each line of the reference's output, so the rest of the line stays as the reference printed it. `$1` or `${name}` in `workflint` inserts a group of the pattern, and `$$` inserts a `$`. The entry for `platform-data-updates` replaces the list of labels in the 824 `unknown-runner-label` lines of 305 files:

```toml
[[difference]]
anchor = "platform-data-updates"
fixture = "realworld/*"
upstream = '/available labels are "windows-latest", ... config file \[runner-label\]/'
workflint = 'available labels are "windows-latest", ... config file [runner-label]'
```

Such an entry applies when it matches at least one line, and is a problem in the report when it matches none. Its pattern should end with `\[kind\]`, so that `--kind` skips it for other kinds. Use it only when the changed text is the same in every file. A difference that adds or removes lines, or that changes text that differs from file to file, needs an entry for each file.

`cargo xtask oracle --kind KIND` compares only the lines of the given kinds (`CONTRIBUTING.md#oracle`). It reads the kind of an `upstream` or `workflint` line, and of a `.out` line, from its brackets. A `/pattern/` line has the kind its pattern ends with, as `\[kind\]` or `\[(kind|kind)\]`, and a pattern line without one counts for every kind. The oracle uses only the `upstream` and `workflint` lines of the given kinds, and skips an entry that has neither, without a problem. An entry that replaces a line of one kind with a line of another kind applies one half: the half of the given kind.

## Rule kinds

workflint prints rule IDs in brackets, and the `.out` files have actionlint kinds (`docs/differences.md#rule-id-in-brackets`). The runner and `cargo xtask oracle` take a rule ID's kind from the `RuleMeta` in the rule registry of `workflint-rules`. `rule-kinds.toml` maps each rule ID of the parity checklist that has no rule yet to its kind:

```toml
shellcheck-issue = "shellcheck"
```

The test `rule_kinds::rule-kinds.toml` fails if a rule ID of the "Rule ID (kind)" column of `docs/parity-checklist.md` is in both the registry and the table, or in neither, or if its kind there differs from the checklist. It also fails if a registered rule or a table entry is missing from the checklist. So a pull request that adds a rule moves its rule IDs out of this file, and one that adds or renames a rule ID updates the checklist too.
