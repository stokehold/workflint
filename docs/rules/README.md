# Rules

Each rule has a page here, `docs/rules/<id>.md`, named after its rule ID (ADR 0008). The diagnostics of a rule end with its ID in brackets, such as `[unexpected-key]`, where actionlint prints the kind (see [Rule ID in brackets instead of the kind](../differences.md#rule-id-in-brackets)). The `docs_url` of the rule's `RuleMeta` links the page.

The test `rule_pages` in `crates/workflint-rules/tests/rule_pages.rs` reads every page. It runs each example through the YAML layer, the workflow parser, and the rules, as the binary does, and compares the result with the page. It also checks that every registered rule has a page, that every page belongs to a registered rule, and that the index below matches the registry.

## Page format

A page has these parts, in this order:

1. The title, `# <rule-id>`.
2. A table with the rule's kind, category, and default severity, as the registry has them:

   ```markdown
   | Kind | Category | Default severity |
   |---|---|---|
   | `syntax-check` | correctness | error |
   ```

3. The rule's description from its `RuleMeta`, with a final period, on one line. More paragraphs may follow to say what the rule reports.
4. A `## Why` section that says why the rule reports it, such as what GitHub does with the workflow.
5. `## Failing example`, or `## Failing examples` when there are several. Each example is a `yaml` block with a workflow, followed by a `text` block with the diagnostics that workflint gives for it in the one-line format (`-oneline`). The file is named `test.yaml`. Each example gives at least one diagnostic of the page's rule. It may give diagnostics of other rules only if the page links their pages, as in `[unexpected-node-kind](unexpected-node-kind.md)`. Prose may come between the examples.
   A rule that never reports, such as one that the parser makes impossible to reach, has a `## Unreachable` section in place of the failing examples. It says why the rule never reports. The test then rejects a failing example.
6. `## Passing example`, or `## Passing examples`, with one or more `yaml` blocks. Each workflow gives no diagnostic at all.
7. `## Differences from actionlint`, only when the rule behaves differently from actionlint. It lists links to the entries in [differences.md](../differences.md), and the test checks that each anchor exists.

An example that needs files beside the workflow, such as a reusable workflow that it calls, puts each file in a `yaml` block before the workflow, with the file's path from the project root after `file=`:

````markdown
```yaml file=.github/workflows/build.yaml
on: workflow_call
```
````

The test holds these files in memory, in a project whose root also holds `test.yaml`. They belong to the next example of the section only. A block that no example follows, or that comes between a workflow and its `text` block, fails the test. A message that names a file has its path from the project root, where the binary prints an absolute path.

## Index

The test `the_rules_index_matches_the_registry` compares this table with the registry and prints the expected table when they differ. A rule ID links its page when the page exists.

<!-- rules-index:start -->
| Rule | Kind | Category | Default severity | Description |
|---|---|---|---|---|
| [`activity-types-not-supported`](activity-types-not-supported.md) | `events` | correctness | error | `types` is set on an event that has no activity types |
| [`broken-reusable-workflow`](broken-reusable-workflow.md) | `workflow-call` | correctness | error | A called local reusable workflow has no valid `on.workflow_call` section |
| [`case-argument-count`](case-argument-count.md) | `expression` | correctness | error | `case()` has an even number of arguments, so it has no default value |
| [`choice-input-without-options`](choice-input-without-options.md) | `events` | correctness | error | A `choice` input of `workflow_dispatch` has no `options` |
| [`conflicting-filters`](conflicting-filters.md) | `events` | correctness | error | A filter and its `-ignore` filter are both set on one event |
| [`constant-condition`](constant-condition.md) | `if-cond` | correctness | error | An `if:` condition has the same value in every run |
| [`context-not-allowed`](context-not-allowed.md) | `expression` | correctness | error | An expression reads a context that its workflow key does not allow |
| [`default-not-in-options`](default-not-in-options.md) | `events` | correctness | error | The default of a `choice` input of `workflow_dispatch` is not one of its options |
| [`deprecated-action-input`](deprecated-action-input.md) | `action` | correctness | error | `with:` sets an input that the action has deprecated |
| [`duplicate-input-option`](duplicate-input-option.md) | `events` | correctness | error | A `choice` input of `workflow_dispatch` lists an option more than once |
| [`duplicate-key`](duplicate-key.md) | `syntax-check` | correctness | error | A mapping repeats a key |
| [`empty-section`](empty-section.md) | `syntax-check` | correctness | error | A mapping or sequence that must have entries is empty or null |
| [`empty-string`](empty-string.md) | `syntax-check` | correctness | error | A string that must not be empty is empty |
| [`empty-workflow`](empty-workflow.md) | `syntax-check` | correctness | error | The file has no YAML document, or only comments |
| [`expected-single-expression`](expected-single-expression.md) | `syntax-check` | correctness | error | A value that must be a literal or a single `${{ }}` expression is other text |
| [`filter-not-supported`](filter-not-supported.md) | `events` | correctness | error | A branch, tag, or path filter is set on an event that does not support it |
| [`function-not-allowed`](function-not-allowed.md) | `expression` | correctness | error | An expression calls a special function that its workflow key does not allow |
| [`glob-path-dot`](glob-path-dot.md) | `glob` | correctness | error | A path pattern is `.` or `..`, or starts with `./` or `../` |
| [`glob-path-whitespace`](glob-path-whitespace.md) | `glob` | correctness | error | A path pattern has white space at either end |
| [`hardcoded-password`](hardcoded-password.md) | `credentials` | correctness | error | `credentials.password` of a container or service is not a single `${{ }}` expression |
| [`if-always-true`](if-always-true.md) | `if-cond` | correctness | error | An `if:` condition has text around its `${{ }}`, so it is always true |
| [`incomplete-credentials`](incomplete-credentials.md) | `syntax-check` | correctness | error | `credentials` of a container or service lacks `username` or `password` |
| [`invalid-action-format`](invalid-action-format.md) | `action` | correctness | error | A step's `uses:` is not a local action, a Docker image, or `{owner}/{repo}@{ref}` |
| [`invalid-activity-type`](invalid-activity-type.md) | `events` | correctness | error | `types` lists an activity type that the event does not have |
| [`invalid-comparison`](invalid-comparison.md) | `expression` | correctness | error | A comparison has operands that GitHub cannot compare |
| [`invalid-config-variable-name`](invalid-config-variable-name.md) | `expression` | correctness | error | The name in `vars.NAME` breaks GitHub's naming rules for configuration variables |
| [`invalid-dispatch-input-type`](invalid-dispatch-input-type.md) | `syntax-check` | correctness | error | A `workflow_dispatch` input has an unknown `type` |
| [`invalid-docker-action`](invalid-docker-action.md) | `action` | correctness | error | A `docker://` action has an invalid URI or an empty tag |
| [`invalid-env-var-name`](invalid-env-var-name.md) | `env-var` | correctness | error | A name in `env` contains `&`, `=`, a space, or a tab |
| [`invalid-expression-syntax`](invalid-expression-syntax.md) | `expression` | correctness | error | An expression in `${{ }}` cannot be lexed or parsed |
| [`invalid-glob`](invalid-glob.md) | `glob` | correctness | error | A filter pattern is not valid glob syntax, or contains a newline |
| [`invalid-index-access`](invalid-index-access.md) | `expression` | correctness | error | An index has the wrong type, or its operand is neither an object nor an array |
| [`invalid-input-default`](invalid-input-default.md) | `events` | correctness | error | The default of a `number` or `boolean` input of `workflow_call` or `workflow_dispatch` does not match its type |
| [`invalid-json-literal`](invalid-json-literal.md) | `expression` | correctness | error | The string literal passed to `fromJSON()` is not valid JSON |
| [`invalid-literal-value`](invalid-literal-value.md) | `syntax-check` | correctness | error | A boolean, integer, or float value has the wrong tag or does not parse |
| [`invalid-number-literal`](invalid-number-literal.md) | `expression` | correctness | error | A number literal in an expression has no value as a double |
| [`invalid-object-filter`](invalid-object-filter.md) | `expression` | correctness | error | An object filter `.*` applies to a value without object elements |
| [`invalid-property-access`](invalid-property-access.md) | `expression` | correctness | error | An expression reads a property with a dot from a value that is not an object |
| [`invalid-ref-name`](invalid-ref-name.md) | `glob` | correctness | error | A branch or tag pattern has a character that a Git ref name cannot have |
| [`invalid-reusable-call-key`](invalid-reusable-call-key.md) | `syntax-check` | correctness | error | A job that calls a reusable workflow has a key that only a job with steps may have |
| [`invalid-reusable-workflow-ref`](invalid-reusable-workflow-ref.md) | `workflow-call` | correctness | error | A job's `uses:` is not `./path/to/workflow.yml` or `owner/repo/path/to/workflow.yml@ref` |
| [`invalid-runner-label-pattern`](invalid-runner-label-pattern.md) | `runner-label` | correctness | error | A runner label pattern in the config is not a valid glob |
| [`invalid-workflow-call-input-type`](invalid-workflow-call-input-type.md) | `syntax-check` | correctness | error | A `workflow_call` input has an unknown `type` |
| [`merge-key-unsupported`](merge-key-unsupported.md) | `syntax-check` | correctness | error | A mapping has a `<<` merge key, which GitHub Actions does not apply |
| [`missing-format-argument`](missing-format-argument.md) | `expression` | correctness | error | The format string of `format()` has a placeholder beyond its arguments |
| [`missing-required-action-input`](missing-required-action-input.md) | `action` | correctness | error | A step does not set an input that its action requires |
| [`missing-required-key`](missing-required-key.md) | `syntax-check` | correctness | error | A required section or key is missing |
| [`missing-required-workflow-input`](missing-required-workflow-input.md) | `workflow-call` | correctness | error | A call does not set an input that the local reusable workflow requires |
| [`missing-required-workflow-secret`](missing-required-workflow-secret.md) | `workflow-call` | correctness | error | A call does not pass a secret that the local reusable workflow requires |
| [`missing-workflow-run-workflows`](missing-workflow-run-workflows.md) | `events` | correctness | error | A `workflow_run` event has no `workflows` |
| [`non-positive-number`](non-positive-number.md) | `syntax-check` | correctness | error | A literal `timeout-minutes` or `max-parallel` is zero or less |
| [`object-in-template`](object-in-template.md) | `expression` | correctness | error | A `${{ }}` in a string evaluates to an object, an array, or null |
| [`options-on-non-choice-input`](options-on-non-choice-input.md) | `events` | correctness | error | A `workflow_dispatch` input that is not a `choice` has `options` |
| [`outdated-action`](outdated-action.md) | `action` | correctness | error | A popular action's version runs on a runtime that GitHub no longer has |
| [`recursive-alias`](recursive-alias.md) | `syntax-check` | correctness | error | An alias refers to a node that encloses it |
| [`required-input-with-default`](required-input-with-default.md) | `events` | correctness | error | A required `workflow_call` input has a default, which is never used |
| [`reusable-call-key-without-uses`](reusable-call-key-without-uses.md) | `syntax-check` | correctness | error | A job without `uses` has `with` or `secrets` |
| [`reusable-workflow-not-found`](reusable-workflow-not-found.md) | `workflow-call` | correctness | error | The file of a called local reusable workflow cannot be read |
| [`runner-label-conflict`](runner-label-conflict.md) | `runner-label` | correctness | error | Two `runs-on` labels of a job cannot be on the same runner |
| [`schedule-without-config`](schedule-without-config.md) | `syntax-check` | correctness | error | `schedule` is named in a string or a sequence item of `on`, so it has no `cron` entries |
| [`single-expression-required`](single-expression-required.md) | `expression` | correctness | error | A value that must be one `${{ }}` holds another number of them |
| [`step-without-run-or-uses`](step-without-run-or-uses.md) | `syntax-check` | correctness | error | A step has neither `run` nor `uses` |
| [`too-many-dispatch-inputs`](too-many-dispatch-inputs.md) | `events` | correctness | error | `workflow_dispatch` has more inputs than GitHub allows |
| [`too-many-yaml-nodes`](too-many-yaml-nodes.md) | `syntax-check` | correctness | error | The workflow has more YAML nodes, with its aliases expanded, than GitHub reads |
| [`undefined-action-input`](undefined-action-input.md) | `action` | correctness | error | `with:` sets an input that the action does not declare |
| [`undefined-config-variable`](undefined-config-variable.md) | `expression` | correctness | error | `vars.NAME` reads a configuration variable that the configuration does not list |
| [`undefined-function`](undefined-function.md) | `expression` | correctness | error | An expression calls a function that is not built in |
| [`undefined-property`](undefined-property.md) | `expression` | correctness | error | An expression reads a property that its strict object type does not have |
| [`undefined-variable`](undefined-variable.md) | `expression` | correctness | error | An expression reads a variable that is not a context |
| [`undefined-workflow-input`](undefined-workflow-input.md) | `workflow-call` | correctness | error | `with:` sets an input that the local reusable workflow does not declare |
| [`undefined-workflow-secret`](undefined-workflow-secret.md) | `workflow-call` | correctness | error | `secrets:` passes a secret that the local reusable workflow does not declare |
| [`unexpected-key`](unexpected-key.md) | `syntax-check` | correctness | error | A mapping has a key that its section does not define |
| [`unexpected-node-kind`](unexpected-node-kind.md) | `syntax-check` | correctness | error | A node has the wrong kind, such as a sequence where a mapping is expected |
| [`unknown-anchor`](unknown-anchor.md) | `syntax-check` | correctness | error | An alias refers to an anchor that no earlier node defines |
| [`unknown-event`](unknown-event.md) | `events` | correctness | error | `on` names an event that is not a GitHub webhook event |
| [`unknown-runner-label`](unknown-runner-label.md) | `runner-label` | correctness | error | A `runs-on` label is not a known runner label |
| [`untrusted-input`](untrusted-input.md) | `expression` | correctness | error | A script reads a value that someone outside the repository can set |
| [`untrusted-object-filter`](untrusted-object-filter.md) | `expression` | correctness | error | A script reads an object filter that extracts values that someone outside the repository can set |
| [`unused-anchor`](unused-anchor.md) | `syntax-check` | correctness | error | No alias refers to an anchor |
| [`unused-format-argument`](unused-format-argument.md) | `expression` | correctness | error | The format string of `format()` has no placeholder for an argument |
| [`workflows-filter-not-supported`](workflows-filter-not-supported.md) | `events` | correctness | error | `workflows` is set on an event other than `workflow_run` |
| [`wrong-argument-count`](wrong-argument-count.md) | `expression` | correctness | error | A call of a built-in function has the wrong number of arguments |
| [`wrong-argument-type`](wrong-argument-type.md) | `expression` | correctness | error | An argument of a built-in function has a type that its parameter does not accept |
| [`wrong-expression-type`](wrong-expression-type.md) | `expression` | correctness | error | A value written as one `${{ }}` has a type that its key does not accept |
| [`wrong-input-default-type`](wrong-input-default-type.md) | `expression` | correctness | error | A `boolean` or `number` input of `workflow_call` has a default of another type |
| [`yaml-syntax-error`](yaml-syntax-error.md) | `syntax-check` | correctness | error | The input is not valid YAML |
<!-- rules-index:end -->
