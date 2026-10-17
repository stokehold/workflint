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
| uses value | The `uses:` of a step, which names an action, or of a job, which names a reusable workflow. The parser classifies it as the reference's `action` and `workflow-call` rules read it: local, repository, containing an expression, or invalid, and for a step also a Docker image (`StepUses`, `JobUses`). |
| typed decoding | Reading YAML into fixed types with the rules of go-yaml's `Unmarshal`, which the reference uses for local action metadata, the `on.workflow_call` section of local reusable workflows, and its config file. A bool also takes the YAML 1.1 forms such as `yes`, exact duplicate keys are errors, and unknown keys are ignored. `workflint-syntax` does it for the first two. |
| expression | The text inside `${{ }}`. |
| context | A top-level expression variable such as `github`, `env`, `matrix`, or `steps`. |
| object filter | `.*` in an expression, as in `labels.*.name`, which applies the rest of the expression to each element of an array or each value of an object (`ExprKind::ObjectFilter`). |
| type environment | The types of the contexts at one place in a workflow, which the type checker checks expressions against (`TypeEnv`). It has the global contexts and the types of `matrix`, `steps`, `needs`, `secrets`, `inputs`, and `jobs` that the workflow, job, or step sets. |
| built-in function | A function that expressions can call, such as `contains` or `format`, with its overloads (`BUILTIN_FUNCTIONS`). Calls match the name without regard to case. |
| overload | One signature of a built-in function: its parameter types, whether the last parameter repeats, and its return type. A call takes the first overload that accepts its arguments. |
| untrusted input | A context property that someone outside the repository can set, such as `github.event.issue.title` or `github.head_ref`, which can inject code when a script reads it in `${{ }}` (`UNTRUSTED_INPUTS`). Paths write array elements as `*`, as in `github.event.commits.*.message`. |
| workflow key | The path of a place in a workflow where expressions can appear, as GitHub's context availability table writes it, such as `jobs.<job_id>.steps.if` (`WORKFLOW_KEYS`). |
| special function | A built-in function that only some workflow keys allow: `always`, `cancelled`, `failure`, `hashFiles`, and `success` (`SPECIAL_FUNCTIONS`). |
| availability | Which contexts and special functions a given workflow key allows (`Availability`). A key that the table does not list allows none. |
| platform data | Facts about GitHub that change over time: runner labels, permissions, events, contexts, popular actions (ADR 0011). |
| rule | One check with a stable ID and rule metadata, listed in the registry in `workflint-rules`. |
| rule metadata | A rule's `RuleMeta`: its rule ID, kind, category, default severity, and docs URL. |
| parse-stage rule | A rule whose diagnostics a parsing crate reports while parsing, such as `yaml-syntax-error` in `workflint-yaml`. The parsing crate defines its rule metadata, and the registry lists it (ADR 0024). |
| checker rule | A rule such as `undefined-property`, whose diagnostics `TypeEnv::check` in `workflint-expr` reports when a rule in `workflint-rules` calls it. `workflint-expr` defines its rule metadata, and the registry lists it, as for a parse-stage rule (ADR 0028). |
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
| AST | The typed tree of a workflow that `workflint-syntax` builds from the span tree (`Workflow`). It borrows its strings from the span tree. |
| string leaf | A key or string value in the AST (`Str`): the decoded value of a YAML scalar and the node it comes from, which gives its span, tag, style, and source map. |
| template | A string in a place where GitHub evaluates `${{ }}`, such as `run`, `name`, or an `env` value, with each placeholder parsed (`Template`). These are the places that actionlint's `expression` rule checks. Strings elsewhere are plain string leaves. |
| placeholder | One `${{ }}` in a template: its byte range in the decoded value, and its parsed expression or the parse error (`Placeholder`). An `if` condition written without `${{ }}` is one implicit placeholder that covers the whole value. |
| field | A key with a fixed name in a section, such as `name` or `steps` of a job, and its value. It is absent, invalid, or valid (`Field`). |
| expression value | A value that may be a literal or a single `${{ }}` expression, such as `continue-on-error`. It is absent, invalid, a literal, or an expression (`Value`). The expression (`Expr`) holds its parsed placeholder. |
| raw value | A YAML value in a matrix row or an `include` or `exclude` item, kept as written: a scalar with its tag and style, a sequence, or a mapping (`RawValue`, the reference's `RawYAMLValue`). The tag and style let rules tell `1` from `"1"`. |
| invalid value | A value that is written but has the wrong type or is otherwise unusable. The parser reports it once, and the AST marks it (`Field::Invalid`, `Value::Invalid`) so that rules skip it. |
| short form | A string written where a mapping is also accepted, such as `concurrency: ci` or `container: node:24`. The string fills one field of the mapping form, and that field has no key. |
| keyed collection | Entries keyed by names that compare case-insensitively, such as jobs, `env`, or `with` (`Keyed`). It keeps source order, and each entry keeps its key as written and its folded key. |
| folded key | A key lowercased as Go's `strings.ToLower` does. Keyed collections compare folded keys, and lookup returns the first entry with a name. |
| section | A mapping of a workflow with a fixed set of keys, such as the workflow root, a job, or `defaults`. Messages call it `"name" section`. |
| mapping walker | `Parser::parse_mapping` in `workflint-syntax`, the reference's `parseMapping`. It returns the entries of a mapping and reports a null or empty section, a node of the wrong kind, a key that is not a string, a merge key, and a duplicate key. |
| key table | The keys of a section, each with the function that parses its value (`KeySpec`). The unexpected-key message lists the table's keys. |
| YAML node limit | The 50,000 YAML nodes, aliases expanded, that GitHub reads in a workflow before it fails with `Maximum YAML nodes exceeded`. Past it, the parser reports `too-many-yaml-nodes` and parses nothing else. |
| ignore comment | A `# workflint: ignore[...]` YAML comment that suppresses diagnostics (ADR 0009). |
| project root | The directory that contains `.github/workflows` and a version control marker (ADR 0010). |
| external tool | shellcheck, ruff, or pyflakes, run as a subprocess (ADR 0012). |
| milestone | A roadmap stage, M0 to M9, or P for after 1.0. Each one is also a GitHub milestone. |
| task | One unit of roadmap work, sized for one pull request, tracked as a GitHub issue (ADR 0020). |
