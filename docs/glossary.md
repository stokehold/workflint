# Glossary

Use these words with these meanings in code, docs, and pull requests.

| Term | Meaning |
|---|---|
| reference | rhysd/actionlint at commit `011a6d1`, the frozen parity target (ADR 0004). |
| reference binary | The actionlint executable built from the reference by `cargo xtask reference`, at `.reference/bin/actionlint`. The oracle diff and the benchmarks run it. |
| conformance suite | The tests in `tests/conformance/` that run the reference's fixtures through workflint. |
| fixture | One test case of the conformance suite (`Fixture`): an `err`, `examples`, or `ok` workflow file, or a `projects` directory, with its expected errors in a `.out` file. `ok` fixtures expect no errors. |
| fixture ID | A fixture's path without the `.yaml` extension, such as `err/name` or `projects/name`. `ok` fixtures keep the extension, as in `ok/name.yaml`, and workflint's own fixtures get a `workflint/` prefix. `docs/parity-checklist.md`, `expected-failures.toml`, and the allow-list use these IDs. |
| expected failure | A fixture listed in `tests/conformance/expected-failures.toml` with the milestone expected to make it pass. Its test passes while the fixture fails, and fails once the fixture passes. |
| oracle diff | `cargo xtask oracle`, which runs workflint and the reference binary on the same files and compares output. |
| allowed difference | An entry in `tests/conformance/allowed-differences.toml` that changes one expected line of a fixture, explained in `docs/differences.md`. Only differences that change a fixture's or the corpus's output have entries (ADR 0023). |
| corpus | The 1,503 real-world workflow files from the reference's `testdata/realworld/dataset.zip`. |
| workflow | A YAML file in `.github/workflows/` that GitHub runs. |
| action metadata | An `action.yml` or `action.yaml` file that defines an action. |
| composite action | An action whose metadata has `runs.using: composite` and a list of steps. |
| reusable workflow | A workflow with an `on.workflow_call` trigger, called from a job's `uses:`. |
| expression | The text inside `${{ }}`. |
| context | A top-level expression variable such as `github`, `env`, `matrix`, or `steps`. |
| availability | Which contexts and functions a given workflow key allows. |
| platform data | Facts about GitHub that change over time: runner labels, permissions, events, contexts, popular actions (ADR 0011). |
| rule | One check with a stable ID and rule metadata, listed in the registry in `workflint-rules`. |
| rule metadata | A rule's `RuleMeta`: its rule ID, kind, category, default severity, and docs URL. |
| parse-stage rule | A rule whose diagnostics a parsing crate reports while parsing, such as `yaml-syntax-error` in `workflint-yaml`. The parsing crate defines its rule metadata, and the registry lists it (ADR 0024). |
| rule ID | The kebab-case name of a rule, such as `undefined-property`. Users write it in config and ignore comments. |
| kind | The actionlint error kind a rule maps to, such as `expression` or `syntax-check`. Used for conformance and accepted in config. |
| category | The rule set a rule belongs to: `correctness` (on by default), `pedantic`, `style`, `security-strict`, or `policy` (off by default). |
| severity | `error`, `warning`, or `info`. |
| diagnostic | One finding: rule, severity, message, span, optional labels and fix. |
| label | A secondary location of a diagnostic with a short note, such as "anchor defined here" (`Label`). |
| fix | The edits that fix the problem a diagnostic reports, with their applicability (`Fix`). A diagnostic has at most one. |
| text edit | One edit of a fix: a byte range of the file and the text that replaces it (`TextEdit`). |
| applicability | Whether a fix keeps the meaning of the file and can be applied without review (`Applicability::Safe`), or can change behaviour and needs review (`Applicability::Unsafe`). |
| span | A file ID plus a byte range. |
| line index | The byte offset of each line start in a `SourceFile`. It turns a byte offset into a 1-based line and a column counted in code points (`LineCol`). |
| header | The first line the renderers write for a diagnostic: `path:line:col: message [rule-id]`. The one-line format writes only this line. |
| snippet | The lines the default renderer writes after the header: a `|` line, the numbered source line, and the indicator. |
| indicator | The line under the source line in a snippet that marks the span: `^` under its first character and `~` under the rest. |
| event parser | The parser of the vendored libyaml-safer fork, `workflint_yaml::event::EventParser`. It turns YAML text into events, including comment events. |
| span tree | The tree that `workflint_yaml::parse` builds from the events of a file's first YAML document (`Document`): one arena of nodes with byte spans, tags, styles, anchors, and attached comments. |
| source map | The mapping from a decoded scalar's characters back to source bytes, used for positions inside `run:` scripts and quoted strings. `Node::source_map` builds it on demand (ADR 0022). |
| ignore comment | A `# workflint: ignore[...]` YAML comment that suppresses diagnostics (ADR 0009). |
| project root | The directory that contains `.github/workflows` and a version control marker (ADR 0010). |
| external tool | shellcheck, ruff, or pyflakes, run as a subprocess (ADR 0012). |
| milestone | A roadmap stage, M0 to M9, or P for after 1.0. Each one is also a GitHub milestone. |
| task | One unit of roadmap work, sized for one pull request, tracked as a GitHub issue (ADR 0020). |
