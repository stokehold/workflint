# Conformance suite

The conformance suite runs every fixture in this directory through the `workflint` binary and compares the output with the fixture's expected errors, using the rules of actionlint's own tests (ADR 0004). The runner is the `conformance` test target of the `xtask` package, in `xtask/tests/conformance.rs`, with its code in `xtask/src/conformance/`. `cargo xtask oracle` (M1.10) uses the same output parsing, matching and allow-list.

`cargo test --workspace` and `cargo nextest run --workspace` both run it. To run only the suite:

```sh
cargo nextest run -p xtask --test conformance
```

Each fixture is one test, named after its path, such as `actionlint::err/action_metadata_syntax_validation.yaml`. Three more tests check the data files described below.

## Contents

| Path | What it holds |
|---|---|
| `actionlint/` | The reference's test data, vendored by `cargo xtask vendor-testdata`. Do not edit. |
| `workflint/` | workflint's own fixtures, with the same layout as `actionlint/testdata/`. |
| `expected-failures.toml` | Fixtures that do not pass yet, with the milestone expected to make them pass. |
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

This is the command line the runner uses. M5 builds the CLI to accept it, and the reference binary accepts it today.

For each fixture, the runner creates a temporary directory and builds a project in `<temp>/project`:

- For `err`, `examples` and `ok`, it copies the `.github` directory next to the fixture, if there is one, and copies the fixture to `test.yaml`. The reference's tests lint these with the fixture's directory as the project root, and the `.github` directory holds the local actions and reusable workflows that the fixtures call.
- For `projects`, it copies the project directory.
- In both cases it creates `.git` and `.github/workflows` directories, so that both workflint and the reference take `<temp>/project` as the project root.

It writes an empty file to `<temp>/config.yaml`. Then it runs the binary in `<temp>/project`:

```sh
workflint -oneline -no-color -shellcheck=<path> -pyflakes=<path> -config-file <config> <files>...
```

- `<config>` is `<temp>/project/actionlint.yaml` for a project that has one, and `<temp>/config.yaml` otherwise. Passing a config file means the CLI reads no config from the workflint repository or from the user's config directory.
- `<files>` is `test.yaml`, or a project's workflow files as relative paths with `/` separators, such as `workflows/nested/piyo.yaml`, in the order of Go's `filepath.Walk`. actionlint's CLI does not accept a directory.
- `-shellcheck=` and `-pyflakes=` take the tool's absolute path, or nothing to turn the tool off. As in the reference's tests:
  - `err` and `examples` fixtures turn a tool on only if the file name contains `shellcheck` or `pyflakes`.
  - `ok` fixtures turn on every tool found on `PATH`.
  - `projects` turn both off.
  - A fixture whose file name contains a tool's name is skipped, with a message, if the tool is not on `PATH`.
- Standard input is empty. Exit codes 0 and 1 are normal. Any other exit code fails the fixture and the runner prints standard error.

The runner builds the binary with `cargo build -p workflint` before the first fixture in each test process.

## Matching

The runner compares the output with the expected lines as `checkErrors` in the reference's `linter_test.go` does:

1. Each line of standard output must have the form `path:line:col: message [rule-id]`. A line that does not parse fails the fixture.
2. Backslashes in the path become `/`.
3. The rule ID becomes its actionlint kind, from `rule-kinds.toml`. An unknown rule ID fails the fixture.
4. The errors are sorted by path, line, column and message, as the reference sorts them. workflint sorts by rule ID before message at one position (`docs/differences.md#sort-order`), and this step undoes that.
5. The number of errors must equal the number of expected lines.
6. The n-th expected line must equal the n-th error, written as `path:line:col: message [kind]`. A line that starts and ends with `/` is a regular expression instead, which must match anywhere in the error. The `regex` crate reads these patterns, except that a `{` or `}` that does not form a repetition such as `{2,5}` is escaped first, because Go reads such braces as literals.

`projects/local_action_invalid.out` has a pattern that expects the relative path `"testdata/projects/local_action_invalid/no_name/action.yaml"`. The reference's Go test passes a relative project root, but a CLI that finds the project root prints an absolute path, so the reference binary fails that line too. The task that implements `missing-action-name` (M4) decides how to handle it.

## Expected failures

`expected-failures.toml` lists each fixture that does not pass yet, with the milestone expected to make it pass:

```toml
"err/action_metadata_syntax_validation" = "M4"
"ok/minimal.yaml" = "M5"
```

The milestone is the latest one among the `docs/parity-checklist.md` rows that cite the fixture, or M5 if no row cites it.

- A listed fixture that fails passes its test and prints `expected failure until <milestone>` with the first line of the reason.
- A listed fixture that passes fails its test, with a message that says to remove its line. The pull request that makes a fixture pass removes the line.
- A fixture that is not listed must pass.
- The test `expected_failures::expected-failures.toml` fails if a line names a fixture that does not exist.

To see the full reason a listed fixture fails, remove its line and run its test.

## Allowed differences

`allowed-differences.toml` lists the expected lines that workflint replaces, removes, or adds on purpose. It has one `[[difference]]` table per changed line:

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
| `fixture` | The fixture ID. Required. |
| `upstream` | An expected line of the fixture's `.out` file, copied exactly, that workflint does not report. |
| `workflint` | The line workflint reports instead or in addition, in the same form as `.out` lines: with the actionlint kind in brackets, or a `/pattern/`. |

An entry with both `upstream` and `workflint` replaces a line. With only `upstream` it removes one, and with only `workflint` it adds one. The runner inserts each `workflint` line before the first expected line that sorts after it by path, line, column and message. It sorts a pattern line by the position at its start, if it has one, and places the new line after pattern lines at the same position.

The runner fails:

- the test `allowed_differences::allowed-differences.toml` if an anchor is not in `docs/differences.md`, a fixture does not exist, an entry has neither `upstream` nor `workflint`, or an `upstream` line is not in the fixture's `.out` file.
- a fixture that is not in `expected-failures.toml` if it also passes without one of its entries. Remove such an entry.

Every entry needs a matching section in `docs/differences.md`. Write that section first, then the entry.

## Rule kinds

workflint prints rule IDs in brackets, and the `.out` files have actionlint kinds (`docs/differences.md#rule-id-in-brackets`). `rule-kinds.toml` maps each rule ID to its kind:

```toml
undefined-variable = "expression"
```

It holds the "Rule ID (kind)" column of `docs/parity-checklist.md`. The test `rule_kinds::rule-kinds.toml` fails if the two disagree, so a pull request that adds or renames a rule ID in the checklist updates this file too. M4.1 replaces the table with the rule registry.
