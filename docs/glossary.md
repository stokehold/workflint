# Glossary

Use these words with these meanings in code, docs, and pull requests.

| Term | Meaning |
|---|---|
| reference | rhysd/actionlint at commit `011a6d1`, the frozen parity target (ADR 0004). |
| conformance suite | The tests in `tests/conformance/` that run the reference's fixtures through workflint. |
| oracle diff | `cargo xtask oracle`, which runs workflint and the reference binary on the same files and compares output. |
| allowed difference | An entry in `tests/conformance/allowed-differences.toml`, explained in `docs/differences.md`. |
| corpus | The 1,503 real-world workflow files from the reference's `testdata/realworld/dataset.zip`. |
| workflow | A YAML file in `.github/workflows/` that GitHub runs. |
| action metadata | An `action.yml` or `action.yaml` file that defines an action. |
| composite action | An action whose metadata has `runs.using: composite` and a list of steps. |
| reusable workflow | A workflow with an `on.workflow_call` trigger, called from a job's `uses:`. |
| expression | The text inside `${{ }}`. |
| context | A top-level expression variable such as `github`, `env`, `matrix`, or `steps`. |
| availability | Which contexts and functions a given workflow key allows. |
| platform data | Facts about GitHub that change over time: runner labels, permissions, events, contexts, popular actions (ADR 0011). |
| rule | One check with a stable ID, registered in `workflint-rules`. |
| rule ID | The kebab-case name of a rule, such as `undefined-property`. Users write it in config and ignore comments. |
| kind | The actionlint error kind a rule maps to, such as `expression` or `syntax-check`. Used for conformance and accepted in config. |
| category | The rule set a rule belongs to: `correctness` (on by default), `pedantic`, `style`, `security-strict`, or `policy` (off by default). |
| severity | `error`, `warning`, or `info`. |
| diagnostic | One finding: rule, severity, message, span, optional labels and fix. |
| span | A file ID plus a byte range. |
| source map | The mapping from a decoded scalar's characters back to source bytes, used for positions inside `run:` scripts and quoted strings. |
| ignore comment | A `# workflint: ignore[...]` YAML comment that suppresses diagnostics (ADR 0009). |
| project root | The directory that contains `.github/workflows` and a version control marker (ADR 0010). |
| external tool | shellcheck, ruff, or pyflakes, run as a subprocess (ADR 0012). |
| milestone | A roadmap stage, M0 to M9, or P for after 1.0. |
