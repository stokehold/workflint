# ADR 0003: actionlint compatibility contract

- Status: accepted
- Date: 2026-10-15
- Amended by: ADR 0023

## Context

People run actionlint in CI scripts, pre-commit hooks, editor plugins, and problem matchers. They pass Go-style flags, keep a `.github/actionlint.yaml`, write `-ignore` regexes against message text, and parse the `path:line:col: message [kind]` output. If workflint breaks any of these, switching costs more than it saves.

actionlint also has confirmed bugs (`docs/research/01-parsing-ast.md`, `02-expressions.md`, `03-rules.md`, `04-cli-output-distribution.md`). Reproducing them on purpose would double the maintenance work and keep users exposed to them.

## Decision

workflint is compatible with actionlint by default, and fixes upstream bugs.

Command line:

- workflint accepts every actionlint flag in Go `flag` syntax: `-flag`, `--flag`, `-flag=value`, `-flag value`, `-bool=false`, and empty values such as `-shellcheck=`. The flags are `-ignore`, `-shellcheck`, `-pyflakes`, `-oneline`, `-format`, `-config-file`, `-init-config`, `-no-color`, `-color`, `-verbose`, `-debug`, `-version`, `-stdin-filename`, `-h`, and `-help`.
- Exit codes keep their meaning. 0 means no problems, 1 means problems found, 2 means an invalid command-line option, and 3 means a fatal error.
- `-` as the only argument reads stdin. With no arguments, workflint discovers the project and lints `.github/workflows`.
- The first line of `-version` output is the bare version number, because install scripts read it with `head -n 1`.
- New flags and subcommands (`workflint check`, `--format json|sarif|github`, `--fail-on`) are additive. A `-format` value without `{{` is rejected by actionlint today, so named formats cannot break existing users.

Configuration:

- workflint reads `.github/actionlint.yaml` and `.github/actionlint.yml` unchanged when no workflint config exists (ADR 0010).

Output:

- The default format keeps actionlint's layout: `path:line:col: message [rule-id]`, followed by the source line and a `^~~~` marker, with the same color codes.
- The bracketed tag is workflint's fine-grained rule ID, such as `[undefined-property]`, not actionlint's coarse kind such as `[expression]`. Users need the ID to write ignore comments (ADR 0009). Problem matchers and other parsers still match the line, because only the label inside the brackets changes.
- Message text is byte-identical to actionlint's wherever actionlint is correct. This lets the conformance suite reuse actionlint's fixtures (ADR 0004).
- `-format` accepts Go `text/template` syntax with actionlint's fields and functions, implemented by our own template engine crate, `workflint-gotemplate`.

Bugs:

- workflint fixes confirmed upstream bugs by default. There are no switches to bring a bug back.
- Every intentional difference is listed in `docs/differences.md` and in the conformance suite's allow-list. A difference that is not listed is a bug in workflint.

Name:

- The binary is `workflint`. Packages never install a binary named `actionlint`, because that would collide with an installed actionlint on `PATH`.

## Consequences

Users can replace `actionlint` with `workflint` in a CI script and get the same exit codes, flags, and layout. Parsers that read the bracketed kind will see new labels; `docs/differences.md` documents this.

The Go template engine is a real cost, since no maintained Rust crate covers the features actionlint users rely on, such as variable reassignment and `printf "%q"`. It is limited to the subset actionlint documents, plus what the conformance fixtures exercise.

## Alternatives considered

- Byte-exact output including bugs. Rejected because every fix would need a switch, and users would keep hitting known bugs by default.
- A clean break with a new CLI and formats. Rejected because adoption depends on being a drop-in replacement.
