# Upstream issue and PR coverage

This file records how workflint handles every open issue and every open pull request in [rhysd/actionlint](https://github.com/rhysd/actionlint). The snapshot date is 2026-10-15. Upstream `main` was at commit `011a6d1`, three commits after the v1.7.12 release. At that point upstream had 154 open issues and 55 open pull requests. The research behind each row is in [research/05-issues.md](research/05-issues.md), and the raw dumps are in `.reference/research/raw/`.

## How to use this file

Each upstream item appears exactly once, in the section that fits it best. The sections follow the categories in the research notes. Some issues fit two sections. The research notes list those twice, but this file lists them once.

When a milestone closes an item, replace the milestone cell with `done in <version>` and link the workflint pull request that closed it. If a decision changes, edit the handling cell and say why in the pull request. Do not delete rows. A row that workflint will never act on stays in the table with "out of scope" or "no change" as its handling.

The handling cell starts with one of these words:

| Handling | Meaning |
|---|---|
| Fix | Upstream behaves wrongly. workflint implements the correct behaviour. |
| By design | workflint's architecture removes the problem, so no specific work item exists. |
| Feature | workflint adds a check, option or output that upstream lacks. |
| Data | The fix is a change to generated platform data such as labels, permissions, contexts or action metadata. |
| No change | Upstream behaves correctly. workflint keeps that behaviour, sometimes with a better message or a test. |
| Out of scope | workflint will not do this. The cell gives the reason. |

The milestone cell holds one of the following values:

| Milestone | Content |
|---|---|
| M1 | Foundations: diagnostics, the YAML layer with spans, comments and scalar source maps, the conformance suite and benchmarks. |
| M2 | Workflow syntax and AST. |
| M3 | Expressions. |
| M4 | Rules and data at actionlint parity. |
| M5 | Linter, config, external tools, CLI and output. Closing M5 ships 0.1.0 at actionlint parity. |
| M6 | Release pipeline, built in parallel from M1. |
| M7 | Data generators and new GitHub syntax. |
| M8 | `action.yml` and composite action linting. |
| M9 | Rule control: severity, categories, inline ignores, strict config, and the first rules of each opt-in category. Closing M9 ships 1.0. |
| P | After 1.0. |
| none | No milestone, because the item is out of scope or needs no change. |

When a workflint issue covers a row, the milestone cell links it after the milestone, as in `M4 (#144)`. Item numbers written as `rhysd/actionlint#N` are upstream issues and pull requests.

The last column names the rule ID or the crate that owns the work, using the crate names from [ADR 0006](adr/0006-workspace-and-crates.md). Rule IDs are working names in kebab-case, and the rule registry in M9 fixes the final names. Following [ADR 0008](adr/0008-rule-registry-and-defaults.md), each rule maps to a kind, which is the actionlint error kind such as `expression`, `action` or `runner-label`. Each rule also belongs to a category. The `correctness` category is on by default, and the `pedantic`, `style`, `security-strict` and `policy` categories are off by default. A rule with no category in the last column is a `correctness` rule.

The handling cell cites an ADR in `docs/adr/` where one records the decision.

## False positives

These items report valid workflows that actionlint rejects. A few of the reports turn out to be correct rejections.

| Item | Title | Handling | Milestone | Rule or component |
|---|---|---|---|---|
| [rhysd/actionlint#701](https://github.com/rhysd/actionlint/issues/701) | `workflow_call` secrets type applied to jobs on other triggers | Fix. The `secrets` context is closed only when `workflow_call` is the only trigger. See [differences.md](differences.md#secrets-context-mixed-triggers). | M4 ([#144](https://github.com/stokehold/workflint/issues/144)) | `undefined-property` |
| [rhysd/actionlint#703](https://github.com/rhysd/actionlint/pull/703) | PR: allow repository secrets in mixed-trigger workflows | Fix. Same change as rhysd/actionlint#701. | M4 ([#144](https://github.com/stokehold/workflint/issues/144)) | `undefined-property` |
| [rhysd/actionlint#148](https://github.com/rhysd/actionlint/issues/148) | Mix inherited with declared secrets | No change. When `workflow_call` is the only trigger, an undeclared secret stays an error, as in the reference, because workflint cannot know whether a caller passes `secrets: inherit`. The `known-secrets` config list from rhysd/actionlint#609 declares the secrets a workflow inherits, and an inline ignore covers a single use. When the workflow has other triggers, rhysd/actionlint#701 applies. | M4 ([#144](https://github.com/stokehold/workflint/issues/144)) | `undefined-property` |
| [rhysd/actionlint#407](https://github.com/rhysd/actionlint/issues/407) | Reusable workflow calls: needs outputs, dynamic matrix and secrets | Fix. A matrix built with `fromJSON()` has type `any`, the secrets part is rhysd/actionlint#701, and inline ignores cover the rest. | M4 ([#145](https://github.com/stokehold/workflint/issues/145)) | `workflint-rules` |
| [rhysd/actionlint#343](https://github.com/rhysd/actionlint/issues/343) | `jobs.<id>.result` rejected in `workflow_call` outputs | Fix. The `jobs` context has both `outputs` and `result`. | M4 ([#144](https://github.com/stokehold/workflint/issues/144)) | `undefined-property` |
| [rhysd/actionlint#724](https://github.com/rhysd/actionlint/pull/724) | PR: define `jobs.<job_id>.result` in `workflow_call` outputs | Fix. Same change as rhysd/actionlint#343. | M4 ([#144](https://github.com/stokehold/workflint/issues/144)) | `undefined-property` |
| [rhysd/actionlint#225](https://github.com/rhysd/actionlint/issues/225) | Quoted `'true'` passed to a string input is typed as bool | Fix. Values in `with:` take their type from the YAML tag and quote style, so a quoted scalar is a string. | M4.10 ([#147](https://github.com/stokehold/workflint/issues/147)) | `workflint-rules` |
| [rhysd/actionlint#250](https://github.com/rhysd/actionlint/issues/250) | Numeric string treated as a number in a property access | Fix. Same tag-aware typing as rhysd/actionlint#225, applied to matrix values. | M4 ([#145](https://github.com/stokehold/workflint/issues/145)) | `workflint-rules` |
| [rhysd/actionlint#727](https://github.com/rhysd/actionlint/pull/727) | PR: treat a matrix scalar tagged `!!str` as a string | Fix. Same change as rhysd/actionlint#250. | M4 ([#145](https://github.com/stokehold/workflint/issues/145)) | `workflint-rules` |
| [rhysd/actionlint#517](https://github.com/rhysd/actionlint/issues/517) | Multiline `if: \|` reported as always true | No change. A block scalar ends with a line break, and GitHub's runner does not trim it, so it reads `${{ ... }}` plus the line break as a string template, which is always true. The reference documents this in `docs/checks.md#if-cond-constant`, and two of its fixtures pin it. See [differences.md](differences.md#investigated-and-kept). | M4 ([#149](https://github.com/stokehold/workflint/issues/149)) | `if-always-true` |
| [rhysd/actionlint#367](https://github.com/rhysd/actionlint/issues/367) | `${{ success() }} \|\| ${{ failure() }}` reported as always true | No change to the diagnostic, which is correct. The message suggests `${{ success() \|\| failure() }}`. See [differences.md](differences.md#if-cond-suggestion). | M4 ([#149](https://github.com/stokehold/workflint/issues/149)) | `if-always-true` |
| [rhysd/actionlint#443](https://github.com/rhysd/actionlint/issues/443) | `github.head_ref` in `if:` flagged as untrusted | No change. Untrusted input is reported only in `run:` and in the `script` input of `actions/github-script`. The regression test `github_head_ref_in_a_condition_is_not_untrusted_input` in `crates/workflint-rules/tests/expression.rs` covers `if:`. | M4 ([#143](https://github.com/stokehold/workflint/issues/143), [#184](https://github.com/stokehold/workflint/pull/184)) | `untrusted-input` |
| [rhysd/actionlint#499](https://github.com/rhysd/actionlint/issues/499) | `key=value` runner labels from third-party runner services | Fix. Labels that contain expressions are evaluated or skipped, and the data includes presets for well-known runner providers. | M4 ([#169](https://github.com/stokehold/workflint/issues/169), [#170](https://github.com/stokehold/workflint/issues/170)) | `unknown-runner-label` |
| [rhysd/actionlint#509](https://github.com/rhysd/actionlint/issues/509) | Outdated-action false positives on GitHub Enterprise Server | Feature. Target profiles for GHES and other platforms select different action and context data. | P | target profiles |
| [rhysd/actionlint#518](https://github.com/rhysd/actionlint/issues/518) | Local JS action entrypoint is built by an earlier step | Fix. A missing entrypoint file is a warning, because a build step may create it. | M4 ([#155](https://github.com/stokehold/workflint/issues/155)) | `action-file-not-found` |
| [rhysd/actionlint#522](https://github.com/rhysd/actionlint/issues/522) | Dynamic outputs of a general-purpose local action | Feature. Config can mark an action as having dynamic outputs, which gives its `outputs` an open type. See ADR 0011. | M5 | `workflint-linter`, `undefined-property` |
| [rhysd/actionlint#530](https://github.com/rhysd/actionlint/issues/530) | Any program on `$PATH` can be a shell | No change. GitHub needs a built-in shell name or a `{0}` command template, and the message mentions the `{0}` form. See [differences.md](differences.md#custom-shell-hint). | M4 ([#160](https://github.com/stokehold/workflint/issues/160)) | `invalid-shell-name` |
| [rhysd/actionlint#511](https://github.com/rhysd/actionlint/issues/511) | `runs.env` in composite actions | No change. GitHub documents `runs.env` for Docker actions only. | none | `workflint-syntax` |
| [rhysd/actionlint#513](https://github.com/rhysd/actionlint/pull/513) | PR: allow `runs.env` in composite actions | Out of scope. It contradicts the documented schema, see rhysd/actionlint#511. | none | `workflint-syntax` |
| [rhysd/actionlint#357](https://github.com/rhysd/actionlint/pull/357) | PR: allow `if:` under event triggers | Out of scope. GitHub's workflow schema has no `on.<event>.if` key. | none | `unexpected-key` |

## False negatives and missing checks

| Item | Title | Handling | Milestone | Rule or component |
|---|---|---|---|---|
| [rhysd/actionlint#734](https://github.com/rhysd/actionlint/issues/734) | `workflow_run.head_branch` not reported as untrusted | Data. The untrusted-input list gains `workflow_run.head_branch`, `display_title`, the message, author, and committer of `head_commit`, and the description and owner login of `head_repository`. See [differences.md](differences.md#workflow-run-untrusted-input). Users can extend the list in config from M7.10 (ADR 0011). | M4 ([#168](https://github.com/stokehold/workflint/issues/168)) | `untrusted-input` |
| [rhysd/actionlint#737](https://github.com/rhysd/actionlint/pull/737) | PR: detect untrusted `workflow_run` head branch and owner login | Data. Same change as rhysd/actionlint#734, which also adds the paths of `head_commit` and `display_title` that the pull request leaves out. | M4 ([#168](https://github.com/stokehold/workflint/issues/168)) | `untrusted-input` |
| [rhysd/actionlint#748](https://github.com/rhysd/actionlint/issues/748) | Reusable workflow call jobs accept `services:` | Fix. A reusable workflow call job takes the reference's list of allowed keys, and `services` is reported like the other keys of a job with steps. See [differences.md](differences.md#reusable-call-services). | M2 | `unexpected-key` |
| [rhysd/actionlint#656](https://github.com/rhysd/actionlint/issues/656) | Runner label checks with expressions are fragile | Fix. The check evaluates `runs-on` to the set of values it can take, using matrix values, `format()` and `&&`/`\|\|`. | M4 ([#169](https://github.com/stokehold/workflint/issues/169)) | `unknown-runner-label` |
| [rhysd/actionlint#634](https://github.com/rhysd/actionlint/issues/634) | `exclude:` values checked against `include:` values | Fix. `exclude` entries are matched against `matrix:` rows only, because GitHub applies `include` after `exclude`. | M4 ([#158](https://github.com/stokehold/workflint/issues/158)) | `unmatched-exclude-value`, `unknown-exclude-key` |
| [rhysd/actionlint#630](https://github.com/rhysd/actionlint/issues/630) | Invalid value types in `matrix.include` and `matrix.exclude` | Feature. A warning when an include or exclude value has a different type than the matrix row. | P | `pedantic` category |
| [rhysd/actionlint#622](https://github.com/rhysd/actionlint/issues/622) | Outdated `actions/cache` pinned by SHA not flagged | Feature. The outdated-action check reads the trailing `# vX.Y.Z` comment to resolve a SHA pin. | P | `action-outdated` |
| [rhysd/actionlint#586](https://github.com/rhysd/actionlint/issues/586) | Report everything GitHub's code scanning queries report for workflows | Out of scope. workflint is a correctness linter, and security auditing beyond actionlint's checks is not part of 1.0. See ADR 0002. | none | none |
| [rhysd/actionlint#552](https://github.com/rhysd/actionlint/issues/552) | Check permissions passed to local reusable workflows | Feature. The caller must grant every scope the callee requests, which needs cross-file analysis. | P | cross-file analysis |
| [rhysd/actionlint#670](https://github.com/rhysd/actionlint/pull/670) | PR: caller and callee permission mismatch, `assume-default-permissions` | Feature. Same change as rhysd/actionlint#552. | P | cross-file analysis |
| [rhysd/actionlint#538](https://github.com/rhysd/actionlint/issues/538) | Concurrency group deadlocks between caller and callee | Feature. A warning when a caller and a local callee use the same `concurrency.group` expression. | P | cross-file analysis |
| [rhysd/actionlint#514](https://github.com/rhysd/actionlint/issues/514) | Local action used before the repository is checked out | Feature. An opt-in warning when no checkout step precedes a `./` action in the job, with configurable checkout actions. | M9 | `local-action-before-checkout`, `pedantic` category |
| [rhysd/actionlint#221](https://github.com/rhysd/actionlint/issues/221) | Require a checkout step before a local action | Feature. Same rule as rhysd/actionlint#514. | M9 | `local-action-before-checkout`, `pedantic` category |
| [rhysd/actionlint#441](https://github.com/rhysd/actionlint/issues/441) | Require `\|` before a multiline `run:` | Feature. A warning when a plain or folded `run:` scalar spans several source lines, because YAML joins the commands into one line. | M9 | `multiline-plain-run` |
| [rhysd/actionlint#417](https://github.com/rhysd/actionlint/issues/417) | Detect `$ {{ x }}` with a space after `$` | Feature. A warning on `$ {{`, `${ {` and `{{ ... }}` without `$`. | M9 | `malformed-placeholder` |
| [rhysd/actionlint#169](https://github.com/rhysd/actionlint/issues/169) | Unclosed and unopened expressions caught inconsistently | Fix. The expression scanner warns at a `}}` that has no opening `${{`, which GitHub keeps as text. | M4 ([#164](https://github.com/stokehold/workflint/issues/164)) | `unmatched-closing-braces` |
| [rhysd/actionlint#385](https://github.com/rhysd/actionlint/issues/385) | Treat workflow inputs as untrusted | Feature. An opt-in rule flags any `${{ }}` in a script except an allow-list of contexts. See ADR 0002. | M9 | `untrusted-input-strict`, `security-strict` category |
| [rhysd/actionlint#213](https://github.com/rhysd/actionlint/issues/213) | Treat `workflow_dispatch` inputs as untrusted | Feature. Same rule as rhysd/actionlint#385. | M9 | `untrusted-input-strict`, `security-strict` category |
| [rhysd/actionlint#96](https://github.com/rhysd/actionlint/issues/96) | Warn on `${{ inputs.* }}` in `run:` | Feature. Same rule as rhysd/actionlint#385. | M9 | `untrusted-input-strict`, `security-strict` category |
| [rhysd/actionlint#332](https://github.com/rhysd/actionlint/pull/332) | PR: treat `env.*` interpolation in scripts as untrusted | Feature. Same rule as rhysd/actionlint#385, never in the default set. | M9 | `untrusted-input-strict`, `security-strict` category |
| [rhysd/actionlint#345](https://github.com/rhysd/actionlint/issues/345) | `${{ env.FOO }}` in `run:` can be an injection | Feature. Taint tracking from an untrusted value through `env` into a script. | P | `untrusted-input` |
| [rhysd/actionlint#365](https://github.com/rhysd/actionlint/issues/365) | Misspelled `steps.cache.outputs.primary-key` not caught | Data. The action metadata generator covers every major version still in use, including `actions/cache/restore@v3`. | M7 | `workflint-data` |
| [rhysd/actionlint#362](https://github.com/rhysd/actionlint/issues/362) | "Too many workflows referenced" limit | Feature. Count nested reusable workflow calls and report GitHub's documented limit. | P | cross-file analysis |
| [rhysd/actionlint#356](https://github.com/rhysd/actionlint/issues/356) | Check the length of a `run:` statement | Feature. An error when an expression or template exceeds GitHub's length limit, after the limit is verified. | M9 | `expression-too-long` |
| [rhysd/actionlint#336](https://github.com/rhysd/actionlint/issues/336) | Inputs passed to a local reusable workflow that only `workflow_dispatch` declares | Fix. The check uses `workflow_call.inputs` only, and the message adds a hint when the input exists under `workflow_dispatch`. See [differences.md](differences.md#dispatch-input-hint). The `workflint-rules` test `an_input_of_workflow_dispatch_gets_a_hint` covers it. | M4 ([#162](https://github.com/stokehold/workflint/issues/162), [#190](https://github.com/stokehold/workflint/pull/190)) | `undefined-workflow-input` |
| [rhysd/actionlint#383](https://github.com/rhysd/actionlint/issues/383) | Validate `with:` for remote reusable workflows | Feature. Needs the remote file, which online mode will fetch and cache. | P | online mode |
| [rhysd/actionlint#277](https://github.com/rhysd/actionlint/issues/277) | `timeout-minutes` used in composite action steps | Feature. The composite step schema rejects keys that GitHub does not support there. | M8 | `unexpected-key` |
| [rhysd/actionlint#265](https://github.com/rhysd/actionlint/issues/265) | Check that local actions exist | Feature. A warning, because a submodule or an earlier step can create the action at run time. | M9 | `local-action-missing` |
| [rhysd/actionlint#246](https://github.com/rhysd/actionlint/issues/246) | `gh` used without `GITHUB_TOKEN` or `GH_TOKEN` | Feature. Needs the built-in bash parser. | P | bash parser |
| [rhysd/actionlint#83](https://github.com/rhysd/actionlint/issues/83) | `workflow_run.workflows` names must exist | Feature. A project-wide index of workflow names, checked when all workflows are in the lint set. | P | cross-file analysis |
| [rhysd/actionlint#74](https://github.com/rhysd/actionlint/issues/74) | `env` keys cannot reference siblings at the same level | Feature. A warning when `${{ env.X }}` refers to a key defined in the same `env:` mapping, because the value is empty at that point. | M9 | `env-sibling-reference` |
| [rhysd/actionlint#39](https://github.com/rhysd/actionlint/issues/39) | Validate `github.event.*` properties | Feature. Event payload types generated from the official webhook schemas, scoped by the workflow's triggers. | P | `workflint-expr` |

## Stale platform data

New GitHub syntax and new values for labels, permissions, events, contexts and action inputs. Until M7, `workflint-data` holds tables ported from actionlint. The M7 generators then produce the data from official sources, and scheduled refresh PRs keep it current. [ADR 0011](adr/0011-platform-data.md) describes the generators and the refresh process.

| Item | Title | Handling | Milestone | Rule or component |
|---|---|---|---|---|
| [rhysd/actionlint#693](https://github.com/rhysd/actionlint/issues/693) | Parallel steps | Feature. The AST models `background`, `wait`, `wait-all`, `cancel` and `parallel` steps. | M7 | `workflint-syntax` |
| [rhysd/actionlint#694](https://github.com/rhysd/actionlint/pull/694) | PR: parse parallel steps | Feature. Same change as rhysd/actionlint#693. | M7 | `workflint-syntax` |
| [rhysd/actionlint#695](https://github.com/rhysd/actionlint/pull/695) | PR: `parallel-steps` rule for `wait` and `cancel` references | Feature. `wait` and `cancel` must name an earlier background step, and only allowed steps may appear inside `parallel`. | M7 | `parallel-step-reference` |
| [rhysd/actionlint#711](https://github.com/rhysd/actionlint/issues/711) | `$/` self-repository `uses:` syntax | Fix. `uses:` is a typed value with a self-repository variant that resolves like `./`. | M7 | `workflint-syntax`, `action` kind |
| [rhysd/actionlint#760](https://github.com/rhysd/actionlint/issues/760) | `$/` for reusable workflows and actions | Fix. Same change as rhysd/actionlint#711. | M7 | `workflint-syntax`, `workflow-call` kind |
| [rhysd/actionlint#732](https://github.com/rhysd/actionlint/pull/732) | PR: support `$/` self-repository `uses:` | Fix. Same change as rhysd/actionlint#711. | M7 | `workflint-syntax` |
| [rhysd/actionlint#745](https://github.com/rhysd/actionlint/pull/745) | PR: `cache-mode` | Data. `cache-mode` is accepted at workflow, job and reusable-call job level, with its allowed values. | M7 | `workflint-data` |
| [rhysd/actionlint#682](https://github.com/rhysd/actionlint/issues/682) | `ubuntu-26.04` runner label | Data. Runner labels are generated from GitHub's runner documentation. | M7 | `workflint-data` |
| [rhysd/actionlint#761](https://github.com/rhysd/actionlint/issues/761) | Missing `ubuntu-26.04-arm` label | Data. Same generator as rhysd/actionlint#682. | M7 | `workflint-data` |
| [rhysd/actionlint#683](https://github.com/rhysd/actionlint/pull/683) | PR: `ubuntu-26.04` runner | Data. Same change as rhysd/actionlint#682. | M7 | `workflint-data` |
| [rhysd/actionlint#723](https://github.com/rhysd/actionlint/pull/723) | PR: Ubuntu 26.04 runner labels | Data. Same change as rhysd/actionlint#682. | M7 | `workflint-data` |
| [rhysd/actionlint#743](https://github.com/rhysd/actionlint/pull/743) | PR: `ubuntu-26.04`, dependency bump and CI fixes | Data. The label part is rhysd/actionlint#682, and the Go dependency and CI parts do not apply to workflint. | M7 | `workflint-data` |
| [rhysd/actionlint#739](https://github.com/rhysd/actionlint/issues/739) | `xcode-27` and `xcode-27-xlarge` labels | Data. Same generator as rhysd/actionlint#682. | M7 | `workflint-data` |
| [rhysd/actionlint#744](https://github.com/rhysd/actionlint/pull/744) | PR: `xcode-27` labels | Data. Same change as rhysd/actionlint#739. | M7 | `workflint-data` |
| [rhysd/actionlint#688](https://github.com/rhysd/actionlint/issues/688) | `windows-2025-vs2026` reported as unknown | Data. v1.7.12 already knows this label, and the generated data includes it. | M7 | `workflint-data` |
| [rhysd/actionlint#710](https://github.com/rhysd/actionlint/pull/710) | PR: `windows-11-vs2026-arm` label | Data. Same generator as rhysd/actionlint#682. | M7 | `workflint-data` |
| [rhysd/actionlint#337](https://github.com/rhysd/actionlint/issues/337) | Larger runner labels such as `ubuntu-latest-4-cores` | Data. Larger runner patterns are in the label data, and config key `runner-labels` covers custom names, with `self-hosted-runner.labels` kept as an alias. | M7 | `workflint-data`, `workflint-linter` |
| [rhysd/actionlint#657](https://github.com/rhysd/actionlint/issues/657) | `concurrency.queue` key | Fix. Accept `queue: single \| max` and report `max` combined with `cancel-in-progress: true`. | M7 | `concurrency-queue` |
| [rhysd/actionlint#680](https://github.com/rhysd/actionlint/issues/680) | `queue` rejected in `concurrency` | Fix. Same change as rhysd/actionlint#657. | M7 | `concurrency-queue` |
| [rhysd/actionlint#687](https://github.com/rhysd/actionlint/issues/687) | `queue: max` in a concurrency group | Fix. Same change as rhysd/actionlint#657. | M7 | `concurrency-queue` |
| [rhysd/actionlint#654](https://github.com/rhysd/actionlint/pull/654) | PR: support `concurrency.queue` | Fix. Same change as rhysd/actionlint#657. | M7 | `concurrency-queue` |
| [rhysd/actionlint#757](https://github.com/rhysd/actionlint/issues/757) | `job.workflow_*` identity properties | Data. Context types are generated, including `job.workflow_ref`, `workflow_sha`, `workflow_repository` and `workflow_file_path`. | M7 | `workflint-data` |
| [rhysd/actionlint#705](https://github.com/rhysd/actionlint/issues/705) | `job.workflow_repository` not defined | Data. Same change as rhysd/actionlint#757. | M7 | `workflint-data` |
| [rhysd/actionlint#647](https://github.com/rhysd/actionlint/issues/647) | Four new `job.workflow_*` properties | Data. Same change as rhysd/actionlint#757. | M7 | `workflint-data` |
| [rhysd/actionlint#696](https://github.com/rhysd/actionlint/pull/696) | PR: four `job.workflow_*` properties | Data. Same change as rhysd/actionlint#757. | M7 | `workflint-data` |
| [rhysd/actionlint#707](https://github.com/rhysd/actionlint/pull/707) | PR: documented job workflow properties | Data. Same change as rhysd/actionlint#757. | M7 | `workflint-data` |
| [rhysd/actionlint#671](https://github.com/rhysd/actionlint/pull/671) | PR: `github.job_workflow_sha` | Data. The property is null in expressions, so workflint keeps it out of the typed context and suggests `job.workflow_sha` instead. | M7 | `undefined-property` |
| [rhysd/actionlint#713](https://github.com/rhysd/actionlint/issues/713) | `vulnerability-alerts` permission | Data. Permission scopes come from the official schema, each with its allowed access levels. | M7 | `workflint-data` |
| [rhysd/actionlint#666](https://github.com/rhysd/actionlint/pull/666) | PR: `vulnerability-alerts` permission | Data. Same change as rhysd/actionlint#713. | M7 | `workflint-data` |
| [rhysd/actionlint#672](https://github.com/rhysd/actionlint/issues/672) | `code-quality` permission | Data. Same generator as rhysd/actionlint#713. | M7 | `workflint-data` |
| [rhysd/actionlint#720](https://github.com/rhysd/actionlint/issues/720) | Support `code-quality` permission | Data. Same change as rhysd/actionlint#672. | M7 | `workflint-data` |
| [rhysd/actionlint#658](https://github.com/rhysd/actionlint/pull/658) | PR: allow `code-quality` permission | Data. Same change as rhysd/actionlint#672. | M7 | `workflint-data` |
| [rhysd/actionlint#674](https://github.com/rhysd/actionlint/pull/674) | PR: `code-quality` permission | Data. Same change as rhysd/actionlint#672. | M7 | `workflint-data` |
| [rhysd/actionlint#686](https://github.com/rhysd/actionlint/issues/686) | `copilot-requests` permission | Data. Same generator as rhysd/actionlint#713. | M7 | `workflint-data` |
| [rhysd/actionlint#689](https://github.com/rhysd/actionlint/pull/689) | PR: `copilot-requests` permission | Data. Same change as rhysd/actionlint#686. | M7 | `workflint-data` |
| [rhysd/actionlint#726](https://github.com/rhysd/actionlint/issues/726) | `merge_group` activity type `destroyed` | Data. Activity types are generated from the webhook schema and the trigger docs. | M7 | `workflint-data` |
| [rhysd/actionlint#740](https://github.com/rhysd/actionlint/pull/740) | PR: allow `merge_group` type `destroyed` | Data. Same change as rhysd/actionlint#726. | M7 | `workflint-data` |
| [rhysd/actionlint#669](https://github.com/rhysd/actionlint/issues/669) | `create-github-app-token@v3` input `client-id` rejected | Data. The action metadata generator refreshes on a schedule and honours `deprecationMessage`. | M7 | `workflint-data` |
| [rhysd/actionlint#648](https://github.com/rhysd/actionlint/issues/648) | `create-github-app-token@v3` inputs changed | Data. Same change as rhysd/actionlint#669. | M7 | `workflint-data` |
| [rhysd/actionlint#668](https://github.com/rhysd/actionlint/pull/668) | PR: update metadata for `create-github-app-token@v3` | Data. Same change as rhysd/actionlint#669. | M7 | `workflint-data` |
| [rhysd/actionlint#652](https://github.com/rhysd/actionlint/pull/652) | PR: fix stale `create-github-app-token` metadata | Data. The dataset is keyed by major version and by exact version. | M7 | `workflint-data` |
| [rhysd/actionlint#718](https://github.com/rhysd/actionlint/pull/718) | PR: regenerate the popular actions dataset | Data. The generator runs on a schedule, opens a PR on change, and alerts on failure. | M7 | `workflint-data` |
| [rhysd/actionlint#537](https://github.com/rhysd/actionlint/issues/537) | `download-artifact@v4` input `artifact-ids` | Data. Upstream `main` already has it, and the ported dataset includes it. The test `download_artifact_v4_has_the_artifact_ids_input` checks it. | M4 ([#142](https://github.com/stokehold/workflint/issues/142)) | `workflint-data` |
| [rhysd/actionlint#416](https://github.com/rhysd/actionlint/issues/416) | `dorny/paths-filter@v3` input `predicate-quantifier` | Data. Upstream `main` already has it, and the ported dataset includes it. The test `paths_filter_v3_has_the_predicate_quantifier_input` checks it. | M4 ([#142](https://github.com/stokehold/workflint/issues/142)) | `workflint-data` |
| [rhysd/actionlint#255](https://github.com/rhysd/actionlint/issues/255) | `download-artifact` output `download-path` | Data. The action's own manifest now declares it, and the ported dataset includes it. The test `download_artifact_has_the_download_path_output` checks it. | M4 ([#142](https://github.com/stokehold/workflint/issues/142)) | `workflint-data` |
| [rhysd/actionlint#346](https://github.com/rhysd/actionlint/issues/346) | `actions/labeler@v4` output `all-labels` | Data. The generator keeps metadata for old majors that are still in use. | M7 | `workflint-data` |

## YAML and parsing

| Item | Title | Handling | Milestone | Rule or component |
|---|---|---|---|---|
| [rhysd/actionlint#753](https://github.com/rhysd/actionlint/issues/753) | `.nan` float values panic | By design. The parser returns errors as values and never panics on input. `.nan` gets the message of other invalid floats, see [differences.md](differences.md#nan-panic). | M2 | `workflint-syntax` |
| [rhysd/actionlint#747](https://github.com/rhysd/actionlint/issues/747) | Container `volumes` parsed as ports | Fix. The typed AST and its tests cover every container field. | M2 | `workflint-syntax` |
| [rhysd/actionlint#519](https://github.com/rhysd/actionlint/issues/519) | Cryptic "did not find expected key" on the wrong line | Feature. The message stays actionlint's, at the position in [differences.md](differences.md#yaml-error-positions). M5.8 adds a hint line under it that names the likely cause, such as a mis-indented list item. | M5 | `yaml-syntax-error` |
| [rhysd/actionlint#330](https://github.com/rhysd/actionlint/issues/330) | Wrong column for expression errors in a multiline `run:` | By design. Each scalar builds, on demand, a source map from decoded text back to file positions. | M1 | `workflint-yaml` |
| [rhysd/actionlint#709](https://github.com/rhysd/actionlint/pull/709) | PR: guard odd-length mappings in the action metadata parser | By design. The `action.yml` parser shares the YAML layer, which has no index panics. | M8 | `workflint-syntax` |
| [rhysd/actionlint#735](https://github.com/rhysd/actionlint/pull/735) | PR: update the Go YAML library to rc.6 | By design. workflint uses its own YAML layer with a stable API. | M1 | `workflint-yaml` |
| [rhysd/actionlint#730](https://github.com/rhysd/actionlint/pull/730) | PR: update the Go YAML library and its error API | By design. Same reason as rhysd/actionlint#735. | M1 | `workflint-yaml` |
| [rhysd/actionlint#673](https://github.com/rhysd/actionlint/pull/673) | PR: update the Go YAML library to rc.4 | By design. Same reason as rhysd/actionlint#735. | M1 | `workflint-yaml` |

## Expression engine and types

| Item | Title | Handling | Milestone | Rule or component |
|---|---|---|---|---|
| [rhysd/actionlint#440](https://github.com/rhysd/actionlint/issues/440) | Ternary with a falsy value-if-true | Feature. A warning on `a && <falsy literal> \|\| b`, which always returns `b`. | M9 | `unsound-ternary` |
| [rhysd/actionlint#373](https://github.com/rhysd/actionlint/issues/373) | Mis-terminated `if:` expression across lines | Fix. `unmatched-closing-braces` reports the stray `}}` and `if-always-true` reports the always-true condition, as the fixture `workflint/err/unmatched_closing_braces` shows. | M4 ([#164](https://github.com/stokehold/workflint/issues/164)) | `unmatched-closing-braces`, `if-always-true` |
| [rhysd/actionlint#110](https://github.com/rhysd/actionlint/issues/110) | Quote check missed when `${{ }}` is mixed into `if:` | Fix. The always-true check reports this case. The `workflint-rules` test `quotes_mixed_with_a_placeholder_are_always_true` covers it. | M4 ([#149](https://github.com/stokehold/workflint/issues/149)) | `if-always-true` |
| [rhysd/actionlint#349](https://github.com/rhysd/actionlint/issues/349) | `${{ insert }}` as a mapping key | Feature. Accept `${{ insert }}` as a key in `env`, `with` and matrix, as the runner does. | M4 ([#165](https://github.com/stokehold/workflint/issues/165)) | `workflint-expr` |
| [rhysd/actionlint#434](https://github.com/rhysd/actionlint/issues/434) | Redundant branch checks in `if:` | Feature. Literal types for `github.ref` derived from `on:` filters, reported as an opt-in warning. | P | `pedantic` category |
| [rhysd/actionlint#69](https://github.com/rhysd/actionlint/issues/69) | Redundant `github.event_name` comparisons | Feature. Literal types for `github.event_name` derived from the triggers allow constant folding. | P | `pedantic` category |
| [rhysd/actionlint#68](https://github.com/rhysd/actionlint/issues/68) | Always-false `github.event_name` comparisons | Feature. Same machinery as rhysd/actionlint#69, reported as a warning. | P | `workflint-expr` |
| [rhysd/actionlint#172](https://github.com/rhysd/actionlint/issues/172) | Property lookup case sensitivity | By design. Property lookup is case-insensitive everywhere, with tests for camelCase outputs. | M4 ([#146](https://github.com/stokehold/workflint/issues/146)) | `workflint-expr`, `workflint-rules` |
| [rhysd/actionlint#733](https://github.com/rhysd/actionlint/pull/733) | PR: integer literals above int32 | Fix. Number literals follow JSON grammar and use f64 semantics, as the runner does. | M3 | `workflint-expr` |
| [rhysd/actionlint#708](https://github.com/rhysd/actionlint/pull/708) | PR: accept `+` in the exponent of number literals | Fix. Same change as rhysd/actionlint#733. | M3 | `workflint-expr` |

## Feature requests

| Item | Title | Handling | Milestone | Rule or component |
|---|---|---|---|---|
| [rhysd/actionlint#46](https://github.com/rhysd/actionlint/issues/46) | Lint action metadata files | Feature. `action.yml` is a first-class document kind, and composite steps reuse the workflow step checks. | M8 | `workflint-syntax`, `workflint-rules` |
| [rhysd/actionlint#401](https://github.com/rhysd/actionlint/issues/401) | Composite action support | Feature. Same change as rhysd/actionlint#46. | M8 | `workflint-syntax`, `workflint-rules` |
| [rhysd/actionlint#350](https://github.com/rhysd/actionlint/issues/350) | Not usable for developers of reusable actions | Feature. Same change as rhysd/actionlint#46. | M8 | `workflint-syntax`, `workflint-rules` |
| [rhysd/actionlint#323](https://github.com/rhysd/actionlint/issues/323) | Action linting | Feature. Same change as rhysd/actionlint#46. | M8 | `workflint-syntax`, `workflint-rules` |
| [rhysd/actionlint#366](https://github.com/rhysd/actionlint/pull/366) | PR: lint actions | Feature. Same change as rhysd/actionlint#46, and the PR is a reference for the document model. | M8 | `workflint-syntax`, `workflint-rules` |
| [rhysd/actionlint#237](https://github.com/rhysd/actionlint/issues/237) | Inline ignore comments | Feature. `# workflint: ignore[rule-id, ...] -- reason` as a trailing comment covers its line, and on its own line covers the next line or block. `ignore-file`, `unused-ignore`, `invalid-ignore` and `# actionlint:ignore` for migration complete it. See ADR 0009. | M9 | `workflint-linter`, `unused-ignore` |
| [rhysd/actionlint#375](https://github.com/rhysd/actionlint/pull/375) | PR: inline ignore comments | Feature. Same change as rhysd/actionlint#237. See ADR 0009. | M9 | `workflint-linter` |
| [rhysd/actionlint#516](https://github.com/rhysd/actionlint/issues/516) | Warnings as well as errors | Feature. Each rule has a default severity that config can override, and `--fail-on` sets the exit code. See ADR 0008. | M9 | `workflint-rules` |
| [rhysd/actionlint#75](https://github.com/rhysd/actionlint/issues/75) | Unique ID for each check | Feature. Fine-grained kebab-case rule IDs, each mapped to an actionlint kind, and config accepts the kind in place of a rule ID. See ADR 0008. | M9 | `workflint-rules` |
| [rhysd/actionlint#71](https://github.com/rhysd/actionlint/issues/71) | Link findings to their documentation | Feature. Each rule ID has a documentation page at `docs/rules/<id>.md`, linked from text output and used as the SARIF `helpUri`. See ADR 0008. | M9 | `workflint-rules` |
| [rhysd/actionlint#49](https://github.com/rhysd/actionlint/issues/49) | Require `timeout-minutes` | Feature. An opt-in policy rule with configurable bounds. | M9 | `require-job-timeout`, `policy` category |
| [rhysd/actionlint#581](https://github.com/rhysd/actionlint/pull/581) | PR: configurable `timeout-minutes` rule | Feature. Same rule as rhysd/actionlint#49. | M9 | `require-job-timeout`, `policy` category |
| [rhysd/actionlint#358](https://github.com/rhysd/actionlint/issues/358) | Autofix | Feature. Diagnostics carry optional edits marked safe or unsafe, and `--fix` applies them. | P | autofix |
| [rhysd/actionlint#229](https://github.com/rhysd/actionlint/issues/229) | LSP support | Feature. A `workflint lsp` subcommand on the same library. | P | LSP |
| [rhysd/actionlint#73](https://github.com/rhysd/actionlint/issues/73) | VS Code extension | Feature. A thin client for the LSP server. | P | LSP |
| [rhysd/actionlint#198](https://github.com/rhysd/actionlint/issues/198) | Options to enforce security practices such as SHA pinning and minimal permissions | Feature. Opt-in policy rules for pinning and required permissions. | M9 | `policy` category |
| [rhysd/actionlint#435](https://github.com/rhysd/actionlint/issues/435) | Detect actions pinned to `main` or `master` | Feature. An opt-in pinning rule that requires a commit SHA or an exact version, with allow-lists. | M9 | `require-pinned-uses`, `policy` category |
| [rhysd/actionlint#524](https://github.com/rhysd/actionlint/issues/524) | Require exact versions for third-party actions | Feature. Same rule as rhysd/actionlint#435, in exact-version mode. | M9 | `require-pinned-uses`, `policy` category |
| [rhysd/actionlint#436](https://github.com/rhysd/actionlint/pull/436) | PR: optional rule requiring commit hash pins | Feature. Same rule as rhysd/actionlint#435, in SHA mode. | M9 | `require-pinned-uses`, `policy` category |
| [rhysd/actionlint#738](https://github.com/rhysd/actionlint/issues/738) | Apply SHA pinning to `uses:` inside composite actions | Feature. The pinning rule also runs on local `action.yml` files once M8 lints them. | M9 | `require-pinned-uses`, `policy` category |
| [rhysd/actionlint#728](https://github.com/rhysd/actionlint/issues/728) | Warn on missing `permissions:` | Feature. An opt-in policy rule. | M9 | `require-permissions`, `policy` category |
| [rhysd/actionlint#525](https://github.com/rhysd/actionlint/issues/525) | Explicit permissions for each job | Feature. Same rule as rhysd/actionlint#728, with a job-level mode. | M9 | `require-permissions`, `policy` category |
| [rhysd/actionlint#469](https://github.com/rhysd/actionlint/issues/469) | Require that a specific action is used | Feature. An opt-in policy rule with glob patterns. | P | `required-actions`, `policy` category |
| [rhysd/actionlint#474](https://github.com/rhysd/actionlint/pull/474) | PR: required actions rule | Feature. Same rule as rhysd/actionlint#469. | P | `required-actions`, `policy` category |
| [rhysd/actionlint#450](https://github.com/rhysd/actionlint/issues/450) | Naming convention for inputs and outputs | Feature. An opt-in style rule, kebab-case by default. | P | `input-naming`, `style` category |
| [rhysd/actionlint#501](https://github.com/rhysd/actionlint/issues/501) | Always wrap `if:` in `${{ }}` | Feature. An opt-in style rule. | M9 | `if-explicit-expression`, `style` category |
| [rhysd/actionlint#374](https://github.com/rhysd/actionlint/issues/374) | Warn when `shell:` is not explicit | Feature. An opt-in style rule, because the default shell differs between `sh` and `bash -e`. | M9 | `explicit-shell`, `style` category |
| [rhysd/actionlint#564](https://github.com/rhysd/actionlint/issues/564) | Suggest pinning `*-latest` runners and flag outdated images | Data. Retired runner images are an error by default, judged against the data snapshot date. Advice against `*-latest` is a post-1.0 pedantic rule. See ADR 0011. | M7 | `workflint-data` |
| [rhysd/actionlint#609](https://github.com/rhysd/actionlint/issues/609) | `secrets` allow-list like `config-variables` | Feature. A `known-secrets` config list. See ADR 0010. | M5 | `workflint-linter` |
| [rhysd/actionlint#649](https://github.com/rhysd/actionlint/pull/649) | PR: `config-secrets` allow-list | Feature. Same change as rhysd/actionlint#609. | M5 | `workflint-linter` |
| [rhysd/actionlint#655](https://github.com/rhysd/actionlint/issues/655) | No option to disallow the default runner labels | Feature. An opt-in policy rule that accepts only labels listed in config. | M9 | `allowed-runner-labels`, `policy` category |
| [rhysd/actionlint#478](https://github.com/rhysd/actionlint/issues/478) | Restrict the default self-hosted runner labels | Feature. Same rule as rhysd/actionlint#655. | M9 | `allowed-runner-labels`, `policy` category |
| [rhysd/actionlint#676](https://github.com/rhysd/actionlint/issues/676) | Merge multiple config files | Feature. `extends` in config. See ADR 0010. | P | `workflint-linter` |
| [rhysd/actionlint#675](https://github.com/rhysd/actionlint/issues/675) | Global config from `$XDG_CONFIG_HOME` | Feature. Config discovery falls back to `$XDG_CONFIG_HOME/workflint/config.yaml`. See ADR 0010. | M5 | `workflint-linter` |
| [rhysd/actionlint#706](https://github.com/rhysd/actionlint/pull/706) | PR: user-global config from `$XDG_CONFIG_HOME` | Feature. Same change as rhysd/actionlint#675. | M5 | `workflint-linter` |
| [rhysd/actionlint#623](https://github.com/rhysd/actionlint/issues/623) | Config in `[tool.actionlint]` of `pyproject.toml` | Out of scope. workflint reads one YAML config format, `.github/workflint.yaml`. See ADR 0010. | none | `workflint-linter` |
| [rhysd/actionlint#667](https://github.com/rhysd/actionlint/issues/667) | Ignore generated `*.lock.yml` workflows | Feature. Config `exclude` globs skip files before linting. | M5 | `workflint-linter` |
| [rhysd/actionlint#481](https://github.com/rhysd/actionlint/issues/481) | Ignoring errors through `paths` does not work | Feature. Path globs match against the path relative to the project root, and workflint warns when a `paths` glob matches no linted file. See ADR 0010. | M5 | `workflint-linter` |
| [rhysd/actionlint#554](https://github.com/rhysd/actionlint/issues/554) | Invented config keys are silently ignored | By design. An unknown key in a workflint config is a fatal error, an unknown key in an actionlint config is a warning, and a JSON Schema is published. See ADR 0010. | M9 | `workflint-linter` |
| [rhysd/actionlint#691](https://github.com/rhysd/actionlint/issues/691) | `--summary` aggregate count | Feature. A summary line with counts by severity. | M5 | `workflint` binary |
| [rhysd/actionlint#352](https://github.com/rhysd/actionlint/issues/352) | Custom rules | Feature. Declarative rules in config first, plugins later. | P | `workflint-rules` |
| [rhysd/actionlint#256](https://github.com/rhysd/actionlint/issues/256) | Expose the parsed `uses:` type in the API | By design. The typed `uses:` value is part of the public library API. | M2 | `workflint-syntax` |
| [rhysd/actionlint#300](https://github.com/rhysd/actionlint/issues/300) | Shell completion | Feature. Completions for bash, zsh, fish and PowerShell, and a man page. | M5 | `workflint` binary |
| [rhysd/actionlint#716](https://github.com/rhysd/actionlint/pull/716) | PR: rewrite the CLI for native shell completion | Feature. Same change as rhysd/actionlint#300. | M5 | `workflint` binary |
| [rhysd/actionlint#698](https://github.com/rhysd/actionlint/issues/698) | Forgejo `uses: https://...` URLs | Feature. URL `uses:` values are accepted only under a Forgejo or Gitea target profile. | P | target profiles |

## Performance and concurrency

| Item | Title | Handling | Milestone | Rule or component |
|---|---|---|---|---|
| [rhysd/actionlint#184](https://github.com/rhysd/actionlint/issues/184) | `panic: WaitGroup is reused` | By design. Scoped threads and a bounded subprocess pool replace the shared wait group. | M5 | `workflint-linter` |

## Distribution and install

| Item | Title | Handling | Milestone | Rule or component |
|---|---|---|---|---|
| [rhysd/actionlint#117](https://github.com/rhysd/actionlint/issues/117) | Official GitHub Action | Feature. `stokehold/workflint-action` installs a pinned, checksum-verified release and writes annotations. See ADR 0015. | M6 | `stokehold/workflint-action` |
| [rhysd/actionlint#477](https://github.com/rhysd/actionlint/issues/477) | pre-commit hook does not install shellcheck | Feature. The hook warns when shellcheck is missing, and the docs show how to add shellcheck as a hook dependency. A bundled shellcheck may come later. See ADR 0012 and ADR 0014. | M6 | `stokehold/workflint-pre-commit` |
| [rhysd/actionlint#482](https://github.com/rhysd/actionlint/pull/482) | PR: shellcheck dependency in the pre-commit hooks | Feature. Same change as rhysd/actionlint#477. | M6 | `stokehold/workflint-pre-commit` |
| [rhysd/actionlint#555](https://github.com/rhysd/actionlint/issues/555) | Hook and action fail on ARM runners | By design. Every channel ships x86_64 and aarch64 builds for Linux, macOS and Windows. See ADR 0014. | M6 | release pipeline |
| [rhysd/actionlint#153](https://github.com/rhysd/actionlint/issues/153) | shellcheck crashes in the Docker image on an arm64 host | By design. The Docker image is multi-arch. | M6 | Docker image |
| [rhysd/actionlint#493](https://github.com/rhysd/actionlint/issues/493) | Checksum of the download script | Feature. The install scripts are attached to each release and verify SHA-256 checksums and build attestations. See ADR 0014. | M6 | `install.sh`, `install.ps1` |
| [rhysd/actionlint#520](https://github.com/rhysd/actionlint/issues/520) | Problem matcher JSON must be copied into repositories | By design. `--format github` writes workflow commands, so no problem matcher is needed. | M5 | `workflint` binary |
| [rhysd/actionlint#725](https://github.com/rhysd/actionlint/pull/725) | PR: document a Debian APT archive | No change. Distribution packages are not a v1 channel. | none | release pipeline |
| [rhysd/actionlint#722](https://github.com/rhysd/actionlint/pull/722) | PR: mention a meta-linter in the integration docs | No change. workflint keeps its CLI flags and output formats stable for tools that wrap it. | none | `workflint` binary |
| [rhysd/actionlint#729](https://github.com/rhysd/actionlint/pull/729) | PR: add a third-party release proof step | Out of scope. Releases use standard build attestations. | none | release pipeline |

## External linters

| Item | Title | Handling | Milestone | Rule or component |
|---|---|---|---|---|
| [rhysd/actionlint#702](https://github.com/rhysd/actionlint/issues/702) | shellcheck deadlock when stdin is written before start | By design. workflint spawns the process first, then writes stdin from its own thread while it reads stdout and stderr. See ADR 0012. | M5 | `workflint-linter` |
| [rhysd/actionlint#712](https://github.com/rhysd/actionlint/issues/712) | shellcheck deadlock on `run:` blocks over 64 KiB | By design. Same design as rhysd/actionlint#702, with a regression test that uses a 200 KiB script. | M5 | `workflint-linter` |
| [rhysd/actionlint#704](https://github.com/rhysd/actionlint/issues/704) | Intermittent shellcheck deadlock | By design. Child processes inherit no other file descriptors, and each process has a timeout. See ADR 0012. | M5 | `workflint-linter` |
| [rhysd/actionlint#189](https://github.com/rhysd/actionlint/issues/189) | No notice when pyflakes or shellcheck is missing | Fix. workflint prints one warning per run when a default tool is missing, and exits with code 3 when config or a flag requests a missing tool. See ADR 0012. | M5 | `workflint-linter` |
| [rhysd/actionlint#347](https://github.com/rhysd/actionlint/issues/347) | Different shellcheck behaviour between machines | Fix. Same warning as rhysd/actionlint#189. | M5 | `workflint-linter` |
| [rhysd/actionlint#545](https://github.com/rhysd/actionlint/issues/545) | Missing `fi` not reported | Fix. shellcheck parse errors always reach the output, and rhysd/actionlint#189 covers a missing shellcheck. | M5 | `shellcheck` |
| [rhysd/actionlint#486](https://github.com/rhysd/actionlint/issues/486) | `.shellcheckrc` not used | Feature. Config `shellcheck.rcfile` names an rc file. Without it, workflint passes `--norc` so results do not depend on the machine. See ADR 0012. | M5 | `shellcheck`, `workflint-linter` |
| [rhysd/actionlint#755](https://github.com/rhysd/actionlint/pull/755) | PR: `-shellcheck-rcfile` flag | Feature. Same change as rhysd/actionlint#486. | M5 | `shellcheck`, `workflint-linter` |
| [rhysd/actionlint#360](https://github.com/rhysd/actionlint/issues/360) | Point shellcheck errors at the script line | By design. The block scalar source map from M1 maps shellcheck positions back to the file. | M5 | `shellcheck` |
| [rhysd/actionlint#88](https://github.com/rhysd/actionlint/issues/88) | Errors do not show the line in the workflow | By design. Same source map as rhysd/actionlint#360. | M5 | `shellcheck` |
| [rhysd/actionlint#556](https://github.com/rhysd/actionlint/pull/556) | PR: match shellcheck errors to source lines | By design. Same source map as rhysd/actionlint#360. | M5 | `shellcheck` |
| [rhysd/actionlint#364](https://github.com/rhysd/actionlint/issues/364) | Playground does not run shellcheck | Feature. The WASM playground comes after 1.0 and needs a shellcheck build that runs there. | P | WASM playground |
| [rhysd/actionlint#439](https://github.com/rhysd/actionlint/issues/439) | Use ruff instead of pyflakes | Feature. Config `python.linter: auto` runs ruff with the pyflakes rule set when ruff is installed, and pyflakes otherwise. See ADR 0012. | M5 | `pyflakes` kind |
| [rhysd/actionlint#488](https://github.com/rhysd/actionlint/issues/488) | Lint `actions/github-script` scripts | Feature. A built-in JavaScript syntax check for `script:` inputs. | P | github-script check |
| [rhysd/actionlint#389](https://github.com/rhysd/actionlint/issues/389) | Type-check `actions/github-script` steps | Feature. Syntax checking only, because type checking needs a TypeScript toolchain. | P | github-script check |
| [rhysd/actionlint#244](https://github.com/rhysd/actionlint/issues/244) | TypeScript linter for scripts | Feature. Same syntax check as rhysd/actionlint#389. | P | github-script check |
| [rhysd/actionlint#461](https://github.com/rhysd/actionlint/issues/461) | Move the integration into shellcheck itself | Out of scope. It is a request to another project. | none | none |

## Other

Governance, project discovery, upstream infrastructure, questions and spam.

| Item | Title | Handling | Milestone | Rule or component |
|---|---|---|---|---|
| [rhysd/actionlint#719](https://github.com/rhysd/actionlint/issues/719) | Maintainer inactive, any active forks? | By design. Scheduled data refresh PRs and automatic patch releases keep workflint current. See ADR 0011. | M7 | `workflint-data` generators |
| [rhysd/actionlint#563](https://github.com/rhysd/actionlint/issues/563) | Is the project maintained? | By design. Same answer as rhysd/actionlint#719. | M7 | `workflint-data` generators |
| [rhysd/actionlint#699](https://github.com/rhysd/actionlint/issues/699) | New release? | By design. Merging a data refresh PR triggers a patch release. See ADR 0011. | M7 | `workflint-data` generators |
| [rhysd/actionlint#750](https://github.com/rhysd/actionlint/issues/750) | Project path checks accept sibling directories with a shared prefix | By design. Path checks compare path components, not string prefixes. | M5 | `workflint-linter` |
| [rhysd/actionlint#749](https://github.com/rhysd/actionlint/issues/749) | Non-scalar `ignore` entries match everything | By design. The typed config rejects an entry that is not a string. | M5 | `workflint-linter` |
| [rhysd/actionlint#242](https://github.com/rhysd/actionlint/issues/242) | Workflows not found without a `.git` directory | Fix. The project root is the nearest directory with `.github/workflows` and a VCS marker, falling back to the parent of `.github`. | M5 | `workflint-linter` |
| [rhysd/actionlint#752](https://github.com/rhysd/actionlint/pull/752) | PR: detect Jujutsu workspaces as project roots | Fix. `.jj` counts as a VCS marker, see rhysd/actionlint#242. | M5 | `workflint-linter` |
| [rhysd/actionlint#310](https://github.com/rhysd/actionlint/issues/310) | Can a list of files be passed? | Feature. The CLI accepts files, directories and globs, and expands globs itself. | M5 | `workflint` binary |
| [rhysd/actionlint#754](https://github.com/rhysd/actionlint/issues/754) | Popular actions reader drops a final unterminated record | Out of scope. It concerns upstream's Go generator, and workflint's generators run in CI with failure alerts. | none | none |
| [rhysd/actionlint#758](https://github.com/rhysd/actionlint/issues/758) | Generator test depends on live server responses | Out of scope. It concerns upstream's test suite. | none | none |
| [rhysd/actionlint#759](https://github.com/rhysd/actionlint/issues/759) | Update `x/net` for the vulnerability check | Out of scope. It concerns upstream's Go dependencies. | none | none |
| [rhysd/actionlint#515](https://github.com/rhysd/actionlint/issues/515) | Tests fail on Go 1.24.0 | Out of scope. It concerns upstream's Go toolchain. | none | none |
| [rhysd/actionlint#731](https://github.com/rhysd/actionlint/pull/731) | PR: bump `golang.org/x/net` | No change. It concerns upstream's Go dependencies, and workflint audits its own dependencies in CI. | none | none |
| [rhysd/actionlint#685](https://github.com/rhysd/actionlint/pull/685) | PR: bump Go and `golang.org/x/net` | No change. Same reason as rhysd/actionlint#731. | none | none |
| [rhysd/actionlint#684](https://github.com/rhysd/actionlint/pull/684) | PR: add Go static analysis and vulnerability tools | No change. Same reason as rhysd/actionlint#731. | none | none |
| [rhysd/actionlint#279](https://github.com/rhysd/actionlint/pull/279) | PR: add Dependabot | No change. Same reason as rhysd/actionlint#731. | none | none |
| [rhysd/actionlint#700](https://github.com/rhysd/actionlint/pull/700) | PR: bump the GitHub Actions group | No change. Same reason as rhysd/actionlint#731. | none | none |
| [rhysd/actionlint#369](https://github.com/rhysd/actionlint/issues/369) | Validate package names passed to package managers | Out of scope. The answer depends on the runner image and needs network access. | none | none |
| [rhysd/actionlint#677](https://github.com/rhysd/actionlint/issues/677) | Canary tokens for supply chain compromise | Out of scope. It is a vendor pitch. | none | none |
| [rhysd/actionlint#626](https://github.com/rhysd/actionlint/issues/626) | Free CI cost report | Out of scope. It is a vendor pitch. | none | none |

## Other ideas collected during research

These items are not upstream issues or PRs. They come from the research on other linters, forks and actions. Each row links to the research note that describes it.

| Idea | Milestone | Source |
|---|---|---|
| Each scalar builds, on demand, a map from decoded text to source spans for all five scalar styles, including `''` escapes, backslash escapes, folding, chomping and CRLF. Expressions, shellcheck and ruff positions, inline ignores and autofix all use this map. A test fixture is `'${{ ''foo'' }}'`. | M1 | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| Diagnostics store byte offsets and convert per consumer: characters for actionlint-compatible output, UTF-16 for LSP and SARIF. | M1 | [research/08-zizmor.md](research/08-zizmor.md) |
| A diagnostic can carry related spans, such as "first defined here" for a duplicate ID or each job in a `needs` cycle. | M1 | [research/08-zizmor.md](research/08-zizmor.md) |
| Every upstream actionlint test fixture runs as a parity test, and a real-world corpus run compares diagnostic positions with actionlint. | M1 | [research/09-jactionlint.md](research/09-jactionlint.md) |
| A malformed subtree produces one diagnostic and the rest of the file is still linted. A broken file never stops other files from being linted. | M2 | [research/08-zizmor.md](research/08-zizmor.md) |
| A type error inside an anchored subtree is reported at the anchored node, once for each alias that uses it, as in the reference, with a label at the alias. Identical diagnostics from one anchor are reported once. | M4 ([#167](https://github.com/stokehold/workflint/issues/167)) | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| Verify whether GitHub accepts `secrets: inherit` on the callee side under `on.workflow_call` before workflint accepts or rejects it. | M2 | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| Expression evaluation tests use behaviour vectors measured on hosted runners and the expression test cases from GitHub's language services. | P | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| Unknown contexts, properties and functions get "did you mean" suggestions, including `always` without parentheses and `.gihub`. The suggestion is a help text on the diagnostic, which the actionlint-compatible formats do not print. | M4 ([#166](https://github.com/stokehold/workflint/issues/166)) | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| The runner OS is inferred from prefixed custom labels such as `self-32vcpu-windows-2022`, so the default shell and the shellcheck dialect are correct. `runs-on` resolves to hosted, self-hosted or unknown. | M4 ([#170](https://github.com/stokehold/workflint/issues/170)) | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| An empty or comment-only config file means defaults, and an empty environment variable counts as unset. | M5 | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| One settings resolver merges defaults, config files, environment variables and CLI flags in that order, before any new flags are added. | M5 | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| Subcommands `check`, `rules`, `doctor` and `completion`. Output formats are text, JSON with a versioned schema, JSONL, SARIF and GitHub workflow commands. `-q` and `--silent` reduce output. | M5 | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| One run can write several outputs, for example console text, annotations, a job summary and a SARIF file, through a repeatable `--output <kind>=<path>` flag. | M5 | [research/10-github-action.md](research/10-github-action.md) |
| GitHub output escapes `%`, CR and LF, sorts annotations by severity and caps them per step with a notice that counts the rest. JSON positions are 1-based. | M5 | [research/10-github-action.md](research/10-github-action.md) |
| SARIF output has tight primary regions, related locations, `partialFingerprints`, the full rule list with help URLs, and an exit code that follows `--fail-on` rather than the format. | M5 | [research/10-github-action.md](research/10-github-action.md) |
| shellcheck runs once per distinct script and dialect in a run. An opt-in on-disk cache is keyed by script, dialect, arguments and shellcheck version. ADR 0012 adopts both. | M5 | [research/00-baseline.md](research/00-baseline.md) |
| File discovery respects `.gitignore`. | M5 | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| Release builds include Windows ARM64, and CI runs on a Windows ARM runner. | M6 | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| Releases ship build attestations and an SBOM. CI runs `cargo-deny` and `cargo-audit`, and Dependabot updates cargo dependencies. | M6 | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| The npm and PyPI packages use static Linux binaries. Benchmark the musl build with an alternative allocator early, because musl's default allocator is slow. | M6 | [research/08-zizmor.md](research/08-zizmor.md) |
| The official action is a thin composite wrapper with no nested `uses:`. Each action release embeds checksums for its default version, verifies an unknown newer version through attestations, and exposes `error-count`, `warning-count`, `sarif-file` and a JSON result file. SARIF upload is a documented second step. | M6 | [research/10-github-action.md](research/10-github-action.md) |
| A monitor watches the GitHub changelog and opens an issue for each Actions entry, so new syntax is noticed within days. | M7 | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| The schema data gains newer fields: root `description`, `cancel-timeout-minutes`, `snapshot` and `snapshot.if`, service `entrypoint` and `command`, an empty `services.<id>.image`, empty choice options and UTC time zone aliases. | M7 | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| The event data gains `pull_request` and `pull_request_target` type `stacked`, `issues` types `field_added` and `field_removed`, and the `image_version` event with its filters and types. | M7 | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| New permission scopes `drives` and `models`, with allowed access levels per scope. | M7 | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| New context properties `github.artifacts` and `github.artifacts_list`, and `github.retention_days` typed as a string. | M7 | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| Runner label data records retirement dates and moving aliases such as `ubuntu-latest` during a migration. Retirement is judged against the data snapshot date, not the wall clock, so results stay reproducible. | M7 | [research/09-jactionlint.md](research/09-jactionlint.md) |
| JavaScript action runtimes and their deprecation and removal dates are generated from the runner's source, which reports actions that still use Node 20. | M7 | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| The popular-actions generator finds major versions with `git ls-remote` and selects actions by dependents count. | M7 | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| A step that uses the cache action reports a warning when the effective `cache-mode` disables caching, following the mode through local reusable workflows. | M7 | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| The conformance suite pins GitHub's workflow and action schemas, the runner's schema and the YAML test suite by SHA-256, and records every intended difference in a baseline file. Automation bumps the pins weekly. | M7 | [research/09-jactionlint.md](research/09-jactionlint.md) |
| The `action.yml` schema is generated from the runner's action schema, including which contexts composite steps and input defaults may use. | M8 | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| The rule registry is the single source for rule IDs, kinds, categories, default levels, options and documentation. The rule list, config schema and docs derive from it, with a test that each derived file is up to date. | M9 | [research/09-jactionlint.md](research/09-jactionlint.md) |
| Rule IDs have aliases. A renamed rule keeps working in config, CLI flags and inline ignores, with a deprecation warning. | M9 | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| Disabled rules do not run. `unused-ignore` judges only rules that ran on that file. | M9 | [research/09-jactionlint.md](research/09-jactionlint.md) |
| `--select` and `--ignore` choose rules on the command line, and a selected rule overrides a config that disables it. | M9 | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| Config `ignores` entries select findings by rule, file, `uses`, job or step and carry a reason, instead of matching message text with a regex. | M9 | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| A malformed inline ignore directive is a diagnostic, and an ignore on a block header such as a job key covers the whole block. ADR 0009 adopts both. | M9 | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| An env var that a step defines and a later step reads through `${{ env.X }}` gets a warning, because step-level `env` is not visible to other steps. | M9 | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| An unquoted version-like float in `with:`, such as `python-version: 3.10`, gets a warning because YAML reads it as `3.1`. | M9 | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| Each new rule is run on the real-world corpus and its false positives are sampled before it joins the default set. | M9 | [research/09-jactionlint.md](research/09-jactionlint.md) |
| Pinning and other `uses:` policy lists share one glob matcher in which an exact entry beats a glob. | M9 | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| Autofix applies only edits that keep the meaning, splits fixes into safe and unsafe, and re-encodes replacement text for the scalar style it edits. | P | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| A baseline file records existing findings by a fingerprint without line numbers, so a repository can adopt a stricter category gradually. The same fingerprint feeds SARIF `partialFingerprints`. | P | [research/09-jactionlint.md](research/09-jactionlint.md) |
| `--changed-since <ref>` limits output to findings in changed lines. | P | [research/09-jactionlint.md](research/09-jactionlint.md) |
| Online mode authenticates per host through an `auth` config section, reports rate limits clearly, retries narrowly, and caches HTTP responses by RFC 9111. | P | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| Inline ignores in short-circuit and `case()` branches apply only to the branch that can run. | P | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| A script called from `run:` must have its executable bit set in the git index. | P | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| Two `actions/checkout` steps in the same `parallel` group race on the workspace. | P | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| A generic interface for external script linters, such as one for PowerShell. | P | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| Opt-in policy rules `required-actions` and `disallow-suppressions`, and pinning that reads a GitHub actions lock file once that format is stable. | P | [research/11-open-prs-and-kjanat.md](research/11-open-prs-and-kjanat.md) |
| A `dependabot.yml` check against its published schema. | P | [research/09-jactionlint.md](research/09-jactionlint.md) |

## Explicitly not doing

- `runs.env` in composite actions, from rhysd/actionlint#511 and rhysd/actionlint#513. GitHub documents `runs.env` for Docker actions only.
- `if:` under an event trigger, from rhysd/actionlint#357. GitHub's workflow schema has no `on.<event>.if` key.
- Vendor pitches rhysd/actionlint#677, rhysd/actionlint#626 and rhysd/actionlint#729. They ask workflint to promote a product.
- Security auditing beyond actionlint's checks in 1.0. This covers parity with GitHub's code scanning queries from rhysd/actionlint#586, cache poisoning, workflow command injection, unpinned container images inside `run:` commands, agentic-action configuration and supply-chain heuristics. workflint improves its existing untrusted-input and credentials checks with better data, and offers pinning, required permissions and required timeouts as opt-in policy rules. ADR 0002 records this scope.
- Validating package names in `apt` and similar commands, from rhysd/actionlint#369. The answer depends on the runner image and needs network access.
- Type-checking `actions/github-script` code with a TypeScript toolchain, from rhysd/actionlint#389 and rhysd/actionlint#244. The post-1.0 check covers syntax only.
- Config in TOML or `pyproject.toml`, from rhysd/actionlint#623. workflint reads YAML config only, as ADR 0010 records.
- Moving the shellcheck integration into shellcheck, from rhysd/actionlint#461. That is a request to another project.
- Upstream infrastructure items rhysd/actionlint#754, rhysd/actionlint#758, rhysd/actionlint#759, rhysd/actionlint#515 and the Go dependency PRs rhysd/actionlint#731, rhysd/actionlint#685, rhysd/actionlint#684, rhysd/actionlint#279 and rhysd/actionlint#700. They concern upstream's Go code and CI.
- Downloading data at run time or using the network by default. Data ships in the binary, as ADR 0011 records, and online mode is an opt-in feature after 1.0.
- Judging runner retirement by the wall clock. ADR 0011 compares dates with the data snapshot date, so two runs of the same binary on the same files give the same result.

## Counts

A script compared the item numbers in the main tables with `.reference/research/raw/open-issues.json` and `.reference/research/raw/open-prs.json`. The main tables list 154 issues and 55 pull requests, 209 rows in total. Each open issue and each open pull request appears exactly once, and no row refers to an item outside the snapshot.
