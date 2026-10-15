# 03 — actionlint lint rules: parity reference for the Rust port

Source snapshot: `.reference/actionlint` @ `011a6d1` (2026-04-19). All `file:line` references are relative to that checkout.
Scope: `rule.go`, `pass.go`, all `rule_*.go` except `rule_expression.go`/`rule_if_cond.go` (covered elsewhere, but mapped
here where docs reference them), helpers they depend on (`glob.go`, `process.go`, `action_metadata.go`,
`reusable_workflow.go`, `quotes.go`, `all_webhooks.go`, `popular_actions.go`), `docs/checks.md`, `scripts/check-checks`.

---

## 1. Rule framework

### 1.1 Pass / Visitor (`pass.go`)

- `Pass` interface (`pass.go:10-21`): `VisitStep`, `VisitJobPre`, `VisitJobPost`, `VisitWorkflowPre`, `VisitWorkflowPost`.
  Every callback returns `error`, which means "internal/fatal error". A lint finding is **not** returned as an error.
- `Visitor.Visit` (`pass.go:50-89`) walks depth first:
  1. `VisitWorkflowPre` for **all** passes (in registration order),
  2. for each job: `VisitJobPre` (all passes), then each step `VisitStep` (all passes), then `VisitJobPost` (all passes)
     (`pass.go:91-149`),
  3. `VisitWorkflowPost` for all passes.
- **Job order is nondeterministic.** `Workflow.Jobs` is `map[string]*Job` keyed by lowercased job ID (`ast.go:972-973`),
  and `pass.go:67` ranges over it. Go randomizes map iteration. Output stays stable only because errors are sorted at
  the end (see 1.4). Some messages still depend on visit order (job-needs cycle choice, runner-label conflict partner,
  which file reports a cached metadata error). See section 6.
- The first pass that returns an error aborts the whole visit (`pass.go:57-59` and others).
- Debug timing goes to the debug writer: `[Visitor] %s took %vms` (`pass.go:45-47`).

### 1.2 `RuleBase` and `Rule` (`rule.go`)

- `RuleBase{name, desc, errs []*Error, dbg io.Writer, config *Config}` (`rule.go:10-16`). `NewRuleBase(name, desc)`
  (`rule.go:20`). Built-in rules build the struct literal directly.
- No-op default visitor methods (`rule.go:28-40`), so a rule overrides only the hooks it needs.
- `Error(pos, msg)` / `Errorf(pos, fmt, ...)` (`rule.go:44-54`) append `errorAt`/`errorfAt` (`error.go:48-64`). The `Kind`
  is set to the **rule name**, so the kind is always the rule's `name` field.
- `Debug(fmt, ...)` (`rule.go:58-64`) prints `[<rule name>] <msg>\n` only when `EnableDebug(w)` was given a non-nil writer.
- `SetConfig(*Config)` / `Config()` (`rule.go:89-97`). The linter calls `SetConfig` only when a config exists
  (`linter.go:608-612`), so rules must treat a nil config as the default.
- The `Rule` interface (`rule.go:100-108`) is `Pass` + `Errs()`, `Name()`, `Description()`, `EnableDebug`, `SetConfig`,
  `Config()`.

### 1.3 How rules are instantiated and run (`linter.go:547-645`)

- `Parse(content)` runs first. If it returns a nil workflow (broken YAML), **no rules run**. Only syntax errors are
  reported (`linter.go:547-555`).
- Fresh rule instances are created **per file** in this fixed order (`linter.go:557-573`):
  `matrix, credentials, shell-name, runner-label, events, job-needs, action, env-var, id, glob, permissions,
  workflow-call, expression, deprecated-commands, if-cond`. Then `shellcheck` is added if `-shellcheck` is non-empty and
  the executable resolves, and `pyflakes` likewise (`linter.go:574-593`). If resolution fails, the rule is silently
  disabled with a verbose log line: `Rule "shellcheck" was disabled: <err>`.
- The order matters for parity. `action` runs before `expression`, and `workflow-call` (JobPre) runs before `expression`.
  Both use the shared caches (`LocalActionsCache`, `LocalReusableWorkflowCache`), which report a lookup or parse error only
  on the **first** lookup. So these errors carry kind `action` / `workflow-call`, not `expression`
  (`rule_expression.go:334-337, 366-369, 531-534`).
- `LinterOptions.OnRulesCreated func([]Rule) []Rule` (`linter.go:89-93, 594-596`) can add or remove rules per file. It is
  the **only** way to disable rules. There is no CLI flag or config key for it (`command.go:136-148`).
- The debug writer and config are pushed to every rule (`linter.go:602-612`). Then `v.Visit(w)` runs, `rule.Errs()` are
  concatenated (`linter.go:619-623`), and rules are registered with the error formatter for `{{kind}}`/SARIF metadata
  (`linter.go:625-629`, `error.go:312-321`). The parser kind `syntax-check` is pre-registered with description
  "Checks for GitHub Actions workflow syntax" (`error.go:258`).

### 1.4 Error post-processing

- `filterErrors` (`linter.go:632, 649-672`) runs **before** `Filepath` is set and before sorting.
- `err.Filepath = path` (`linter.go:634-636`).
- Sorting uses `compareErrors` on (filepath, line, column, message) (`error.go:161-172`). **Kind is not part of the key.**
- Dedup uses `slices.CompactFunc(equalsErrors)` (`linter.go:639`, `error.go:174-179`). Adjacent errors with equal
  (filepath, line, col, message) collapse **even if their kinds differ**. This is mainly for YAML aliases that re-check the
  same anchor.
- Output format: `path:line:col: message [kind]` (`error.go:40-42`). The snippet indicator `^~~~` covers the run of
  non-space characters after the column, using rune display widths (`error.go:135-159`).

### 1.5 `-ignore` and `paths.<glob>.ignore`

- CLI: `-ignore` is repeatable. Each value is compiled with Go `regexp` (RE2) in `NewLinter` (`linter.go:152-158`).
  Error: `invalid regular expression for ignore pattern %q: %s`.
- Config: `paths: {<doublestar glob>: {ignore: [regex...]}}` (`config.go:47-52, 68-70`). `IgnorePatterns.UnmarshalYAML`
  requires a sequence (`config.go:30-44`). `PathConfigs(path)` collects **every** matching glob, with the path converted
  by `ToSlash` (`config.go:74-87`). Glob validity is checked at config parse time (`config.go:97-101`).
- Matching is `regexp.MatchString(err.Message)` (`config.go:20-27`). It is **unanchored**, **message only** (the
  `[kind]` is not included, nor are file or position), and the first match wins.
- Debug lines: `Error %q is ignored due to -ignore command line option` / `... due to the "ignore" config in the config
  file`. Verbose summary: `Filtered N error(s) ...`.
- Syntax errors from the parser are filtered the same way.
- There are **no inline suppression comments** (no `# actionlint-disable`) and no per-rule disable. `-ignore 'SC2086:'`
  is the documented workaround for shellcheck codes (`docs/usage.md:100`, `docs/config.md:37-43`).

### 1.6 Rule IDs (kinds): the complete list

| Kind | Struct / file | Description string (verbatim, used in SARIF/`allKinds`) | Hooks | Config used |
|---|---|---|---|---|
| `syntax-check` | parser (`parse.go:74,78,1574,1592`) | `Checks for GitHub Actions workflow syntax` | n/a | none |
| `matrix` | `RuleMatrix` `rule_matrix.go:11-18` | `Checks for matrix combinations in "matrix:"` | JobPre | none |
| `credentials` | `RuleCredentials` `rule_credentials.go:13-20` | `Checks for credentials in "services:" configuration` | JobPre | none |
| `shell-name` | `RuleShellName` `rule_shell_name.go:23-31` | `Checks for shell names used for scripts in "run:"` | WfPre, JobPre/Post, Step | none |
| `runner-label` | `RuleRunnerLabel` `rule_runner_label.go:128-136` | `Checks for GitHub-hosted and preset self-hosted runner labels in "runs-on:"` | JobPre | `self-hosted-runner.labels` |
| `events` | `RuleEvents` `rule_events.go:20-27` | `Checks for workflow trigger events at "on:"` | WfPre | none |
| `job-needs` | `RuleJobNeeds` `rule_job_needs.go:37-45` | `Checks for job IDs in "needs:". Undefined IDs and cyclic dependencies are checked` | JobPre, WfPost | none |
| `action` | `RuleAction` `rule_action.go:307-315` | `Checks for popular actions released on GitHub, local actions, and action calls at "uses:"` | Step | none (uses `LocalActionsCache`) |
| `env-var` | `RuleEnvVar` `rule_env_var.go:11-18` | `Checks for environment variables configuration at "env:"` | WfPre, JobPre, Step | none |
| `id` | `RuleID` `rule_id.go:17-24` | `Checks for duplication and naming convention of job/step IDs` | JobPre/Post, Step | none |
| `glob` | `RuleGlob` `rule_glob.go:10-17` | `Checks for glob syntax used in branch names, tags, and paths` | WfPre, JobPre | none |
| `permissions` | `RulePermissions` `rule_permissions.go:31-38` | `Checks for permissions configuration in "permissions:". Permission names and permission scopes are checked` | WfPre, JobPre | none |
| `workflow-call` | `RuleWorkflowCall` `rule_workflow_call.go:18-28` | `Checks for reusable workflow calls. Inputs and outputs of called reusable workflow are checked` | WfPre, JobPre | none (uses `LocalReusableWorkflowCache`) |
| `expression` | `rule_expression.go:38` | (other report) | all | `config-variables` |
| `deprecated-commands` | `RuleDeprecatedCommands` `rule_deprecated_commands.go:17-24` | `Checks for deprecated "set-output", "save-state", "set-env", and "add-path" commands at "run:"` | Step | none |
| `if-cond` | `rule_if_cond.go:16` | (other report) | JobPre, Step | none |
| `shellcheck` | `RuleShellcheck` `rule_shellcheck.go:29-40` | `Checks for shell script sources in "run:" using shellcheck` | all | CLI `-shellcheck`, env `SHELLCHECK_OPTS` |
| `pyflakes` | `RulePyflakes` `rule_pyflakes.go:38-48` | `Checks for Python script when "shell: python" is configured using Pyflakes` | all | CLI `-pyflakes` |

### 1.7 Message-formatting conventions (needed for byte-exact parity)

- `%q` means Go `strconv.Quote`: double quotes; escapes `"`, `\`, control characters (`\n`, `\t`, `\x00`, `\u00ad`
  style); printable Unicode is kept as-is. Rust `{:?}` differs (`\u{..}`), so the port needs a `go_quote()` helper.
- `quotes(ss)` joins Go-quoted strings with `", "` (`quotes.go:39-60`). `sortedQuotes` sorts first, **in place, which
  mutates the caller's slice** (`quotes.go:62-65`). `quotesAll(...)` concatenates several lists (`quotes.go:67-90`).
- `Pos.String()` gives `line:%d,col:%d` (`ast.go:20-22`).
- Go-specific error texts are embedded in messages. Examples: `strconv.ParseFloat: parsing "teen": invalid syntax`
  (events), robfig/cron errors such as `expected exactly 5 fields, found 4: [0 */3 * *]` (Go `%s` of `[]string` prints
  `[a b c]`), the `url.Parse` error text, `path.Match` errors (`syntax error in pattern`), the YAML library's type
  errors, and `os` errors (`open /x: no such file or directory`).
- `%g` is used for seconds in the cron interval message.

---

## 2. Rules in detail

### 2.1 `action` (`rule_action.go`)

`VisitStep` (`rule_action.go:318-344`) applies only to `ExecAction` steps with `uses:`. It **skips any `uses` containing
`${{ }}`**. Dispatch:
- `./...` → `checkLocalAction`
- `docker://...` → `checkDockerAction`
- anything else → `checkRepoAction`

**Repo actions** (`rule_action.go:347-392`):
- No `@` → `invalidActionFormat(..., "ref is missing")`. No `/` before `@` → `"owner is missing"`.
- Empty owner, repo or ref → `"owner and repo and ref should not be empty"`. This check does **not** return, so lookup
  continues.
- Template (`rule_action.go:395`):
  `specifying action %q in invalid format because %s. available formats are "{owner}/{repo}@{ref}" or "{owner}/{repo}/{path}@{ref}"`
- `.github/my-actions/x` (missing `./`) falls through to repo parsing and gets "ref is missing" (docs/checks.md:1746).
- Lookup in `PopularActions[spec]` is an **exact, case-sensitive key** such as `actions/checkout@v4`. If missing,
  `OutdatedPopularActionSpecs[spec]` is checked (`rule_action.go:377-379`):
  `the runner of %q action is too old to run on GitHub Actions. update the action's version to fix this issue`.
  Otherwise there is a debug line only. `meta.SkipInputs` skips the input checks (`rule_action.go:384-387`).
- SHA-pinned (`@<sha>`) and patch-pinned (`@v4.1.0`) specs are **not checked** (docs/checks.md:1873-1874).

**Docker actions** (`rule_action.go:515-540`):
- The tag is split at the first `:` after `docker://`, and `url.Parse(uri)` is applied. `url.Parse` almost never fails.
- `URI for Docker container %q is invalid: %s (tag=%s)`
- `tag of Docker action should not be empty: %q` (trailing `:`).

**Local actions** (`rule_action.go:577-595`) go through `LocalActionsCache.FindMetadata` (`action_metadata.go:243-289`):
- No project, or no `action.yaml`/`action.yml` (in that order, `action_metadata.go:291-300`) → nil and **no error**. This
  is deliberate (#25, #40: submodules, actions checked out at runtime). The miss is cached.
- YAML parse failure → error `could not parse action metadata in %q: %s` (`action_metadata.go:281`). A single
  `yaml.TypeError` is unwrapped; multiple ones are joined with newlines stripped. The rule reports `err.Error()` at
  `uses` (`rule_action.go:580`). The failure is cached as nil, so it is **reported once per project** for the first
  lookup.
- Metadata validation runs only when `!cached`, i.e. the first time a spec is seen **in the whole project run, across
  files** (`rule_action.go:587-590`). Later uses (other steps or files) skip the metadata errors. Input checks run every
  time.
- Input parsing rules (`action_metadata.go:35-83`):
  - Keys are lowercased.
  - A duplicate input is an error: `input %q is duplicated`.
  - Unexpected keys (anything except `description`, `required`, `default`, `deprecationMessage`) produce a non-fatal
    `unexpected key %q for definition of input %q`. Because it is non-fatal, the metadata is still returned, but
    `yaml.Unmarshal` returns the error and FindMetadata fails.
  - `Required = required && default == nil`.
  - `Deprecated` = the `deprecationMessage` key exists (even if empty).
  - The message is `TrimSpace`d.
- Outputs are lowercased; a duplicate is `output %q is duplicated` (`action_metadata.go:98-115`).

Metadata checks (`checkLocalActionMetadata`, `rule_action.go:543-574`). All are reported at the `uses:` position:

| Check | Template |
|---|---|
| `name` empty | `name is required in action metadata %q` (path) |
| `description` empty | `description is required in metadata of %q action at %q` |
| icon not in `BrandingIcons` (case-insensitive) | `incorrect icon name %q at branding.icon in metadata of %q action at %q. see the official document to know the exhaustive list of supported icons: https://docs.github.com/en/actions/creating-actions/metadata-syntax-for-github-actions#brandingicon` |
| color not in `BrandingColors` | `incorrect color %q at branding.icon in metadata of %q action at %q. see the official document to know the exhaustive list of supported colors: https://docs.github.com/en/actions/creating-actions/metadata-syntax-for-github-actions#brandingcolor` (note: the message says `branding.icon` for the color check, an upstream bug that must be kept for parity) |
| deprecated input with empty message (`rule_action.go:479-491`) | `input %q is deprecated but "deprecationMessage" is empty in metadata of %q action at %q` |
| `runs.using` empty (`rule_action.go:497`) | `"runs.using" is missing in local action %q defined at %q` |
| invalid `using` (`rule_action.go:505`) | `invalid runner name %q at runs.using in %q action defined at %q. valid runners are "composite", "docker", "node20", and "node24". see https://docs.github.com/en/actions/creating-actions/metadata-syntax-for-github-actions#runs`. If the value starts with `node`, the JavaScript checks also run (`rule_action.go:507-510`) |
| missing required runs prop (`rule_action.go:398-400`) | `%q is required in "runs" section because %q is a %s action. the action is defined at %q` (Docker→`image`, Composite→`steps`, JavaScript→`main`) |
| disallowed runs prop (`rule_action.go:402-421`) | `%q is not allowed in "runs" section because %q is a %s action. the action is defined at %q` |
| file missing (`rule_action.go:423-432`) | `file %q does not exist in %q. it is specified at %q key in "runs" section in %q action` (checked for main/pre/post, image (local), pre-entrypoint/entrypoint/post-entrypoint) |
| local Docker image not named Dockerfile (`rule_action.go:440-442`) | `the local file %q referenced from "image" key must be named "Dockerfile" in %q action. the action is defined at %q` |
| `pre-if` without `pre` / `post-if` without `post` (`rule_action.go:467-474`) | `"pre" is required when "pre-if" is specified in "runs" section in %q action. the action is defined at %q` (same for post) |

Disallowed-prop sets:
- Docker: `main, pre, pre-if, post, post-if, steps` (`rule_action.go:447`)
- Composite: `main, pre, pre-if, post, post-if, image, pre-entrypoint, entrypoint, post-entrypoint, args, env`
  (`rule_action.go:455`)
- JavaScript: `steps, image, pre-entrypoint, entrypoint, post-entrypoint, args, env` (`rule_action.go:476`)

Docker image "on registry" prefixes: `docker://`, `gcr.io/`, `pkg.dev/`, `ghcr.io/`, `docker.io/`
(`rule_action.go:291-297`). Anything else is treated as a local path.

Input checks, `checkAction` (`rule_action.go:599-651`). `describe` is `"spec"` for popular actions and
`"Name" defined at "./spec"` for local ones:
- `input %q is not defined in action %s. available inputs are %s` (sorted quotes of the original-case names), reported
  at the `with:` key.
- `avoid using deprecated input %q in action %s` + optional `: <msg>`. The message is the deprecation message with
  trailing `.` and spaces trimmed, and `\s*\r?\n\s*` collapsed to a space (`rule_action.go:597, 615-628`). This is
  skipped when the input is also required.
- `missing input %q which is required by action %s. all required inputs are %s`, reported at `uses`
  (`rule_action.go:632-650`).
- Input-name matching uses the lowercased `exec.Inputs` keys vs the lowercased metadata keys.

Data:
- `BrandingColors`: white, black, yellow, blue, green, orange, red, purple, gray-dark (`rule_action.go:16-26`). The docs
  list omits `black`.
- `BrandingIcons`: the Feather icon set, about 255 entries (`rule_action.go:30-288`).
- `PopularActions` and `OutdatedPopularActionSpecs` (section 5).

### 2.2 `credentials` (`rule_credentials.go`)

- `VisitJobPre` checks `container` and each `services.<name>.container` (`rule_credentials.go:23-33`).
- It reports when `credentials.password` exists and is **not** a single whole-string expression. `IsExpressionAssigned`
  means: trimmed value starts with `${{`, ends with `}}`, and contains exactly one `${{` (`ast.go:57-69`).
- Template: `"password" section in %s should be specified via secrets. do not put password value directly`, where `%s`
  is `"container" section` or `"<name>" service` (`rule_credentials.go:42`). Reported at the password value.
- `username` is not checked.
- `${{ 'literal' }}` passes (false negative). `prefix${{ secrets.X }}` is reported.
- The parser separately requires both username and password: `both "username" and "password" must be specified in
  "credentials" section` [syntax-check] (`parse.go:1027`).
- With YAML anchors, the same anchored password is reported once per alias site, at the same position but with a
  different message (docs/checks.md:3086-3093).

### 2.3 `deprecated-commands` (`rule_deprecated_commands.go`)

- Regex (`rule_deprecated_commands.go:5`):
  `(?:::(save-state|set-output|set-env)\s+name=[a-zA-Z][a-zA-Z_-]*::\S+|::(add-path)::\S+)`
- Applied with `FindAllStringSubmatch` over each `run:` value (`rule_deprecated_commands.go:27-57`). There is **one error
  per match, all reported at the start of `run.Run` (the value)**, not at the matched line.
- Template: ``workflow command %q was deprecated. use `%s` instead: https://docs.github.com/en/actions/using-workflows/workflow-commands-for-github-actions``
- Replacements (`rule_deprecated_commands.go:37-44`):
  - set-output → `` echo "{name}={value}" >> $GITHUB_OUTPUT ``
  - save-state → `$GITHUB_STATE`
  - set-env → `$GITHUB_ENV`
  - add-path → `` echo "{path}" >> $GITHUB_PATH ``
- Edge cases from the tests (`rule_deprecated_commands_test.go:15-90`):
  - A match embedded mid-word is OK (`hello::set-output ...`).
  - Multiple matches on one line are all found.
  - Anything between the command and `name=` → no match.
  - Names may contain `_` and `-` but must start with a letter.
  - `myname=` → no match.
- False negatives:
  - Names containing **digits** (`name=foo1::` fails because `::` must follow `[a-zA-Z_-]*`).
  - An empty value (`::set-output name=x::` followed by nothing).
  - Composite-action `run:` blocks are not linted.
- Go `\s`/`\S` are ASCII-only. In Rust use `(?-u:\s)` for parity.

### 2.4 `env-var` (`rule_env_var.go`)

- Checks `env:` at workflow, job, job container, each service container, and step level (`rule_env_var.go:21-44`).
  Skipped when `env` is a whole `${{ }}` expression (`Env.Expression != nil`) or the key contains an expression (#312).
- It reports a name containing any of `&`, `=`, space or tab (`strings.ContainsAny(name, "&= \t")`,
  `rule_env_var.go:54`).
- Template: `environment variable name %q is invalid. '&', '=' and spaces should not be contained`, at the key position.
- Not checked: names starting with digits, `GITHUB_*` reserved names, newline characters, empty names.

### 2.5 `events` (`rule_events.go`)

`VisitWorkflowPre` dispatches on the event type (`rule_events.go:30-59`). `RepositoryDispatchEvent` has no checks; its
`types` are free-form. `ImageVersionEvent` has no checks here (its `versions` globs are checked by `glob`).

Schedule:
- `checkCron` (`rule_events.go:62-80`) uses robfig/cron v3 with `Minute|Hour|Dom|Month|Dow` (5 fields, **no
  descriptors**, so `@daily` gives `parser does not accept descriptors: @daily`).
- `invalid CRON format %q in schedule event: %s`
- Frequency: `start = Next(Unix(0))`, `next = Next(start)`. If `diff < 300s`:
  `scheduled job runs too frequently. it runs once per %g seconds. the shortest interval is once every 5 minutes`
- robfig quirks the port must mimic:
  - Fields are split by `strings.Fields`.
  - `*` and `?` are both wildcards.
  - Names (`jan`, `mon`) are case-insensitive.
  - `N/step` means `N-max/step`.
  - Day-of-week range is 0-6 (`7` fails: `end of range (7) above maximum (6): 7`).
  - The `TZ=`/`CRON_TZ=` prefix is accepted (GitHub would reject it).
  - The location is `time.Local`.
  - Error texts: `empty spec string`, `too many hyphens: %s`, `too many slashes: %s`,
    `beginning of range (%d) below minimum (%d): %s`, `end of range (%d) above maximum (%d): %s`,
    `beginning of range (%d) beyond end of range (%d): %s`, `step of range should be a positive number: %s`,
    `failed to parse int from %s: %s`, `negative number (%d) not allowed: %s`,
    `expected exactly %d fields, found %d: %s`, `provided bad location %s: %v` (robfig `parser.go`).
- `checkTimezone` (`rule_events.go:83-93`) rejects `""`, any-case `utc`, and `local`. Otherwise it uses
  `time.LoadLocation`. The CLI embeds tzdata (`cmd/actionlint/main.go:8`). Template:
  `invalid timezone %q in schedule event. it must be a valid IANA timezone name`. Rejecting `UTC` is a **likely false
  positive**: `UTC` is an IANA link name.

Webhooks, `checkWebhookEvent` (`rule_events.go:131-176`):
- Unknown hook (`AllWebhookTypes` lookup, case-sensitive): `unknown Webhook event %q. see https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#webhook-events for list of all Webhook event names`
- Types (`rule_events.go:178-202`):
  - Event has no activity types but `types` given: `"types" cannot be specified for %q Webhook event`.
  - Unknown type: `invalid activity type %q for %q Webhook event. available types are %s` (sorted quotes).
- `workflow_run` without `workflows`: `no workflow is configured for "workflow_run" event`. Other events with
  `workflows`: `"workflows" cannot be configured for %q event. it is only for workflow_run event`.
- Filter availability (`rule_events.go:158-175`): paths/paths-ignore → push, pull_request, pull_request_target;
  branches/branches-ignore → merge_group, push, pull_request, pull_request_target, workflow_run; tags/tags-ignore → push.
  - Not available: `%q filter is not available for %s event. it is only for %s %s` (list joined `, `, unquoted;
    `event` if one entry, `events` otherwise) (`rule_events.go:95-101`).
  - Both X and X-ignore set (non-empty): `both %q and %q filters cannot be used for the same event %q. note: use '!' to negate patterns`,
    reported at whichever key comes later (`rule_events.go:113-119`).
  - An empty filter (`branches: []`) counts as absent (`ast.go:128-130`).

`workflow_call` inputs (`rule_events.go:205-243`), skipped when the default contains `${{`:
- number: `input of workflow_call event %q is typed as number but its default value %q cannot be parsed as a float number: %s`
  (the Go ParseFloat error; out-of-range values like `1e400` also error, and `inf`/`NaN`/hex floats are accepted).
- boolean (case-insensitive true/false): `input of workflow_call event %q is typed as boolean. its default value must be true or false but got %q`
- required + default: `input %q of workflow_call event has the default value %q, but it is also required. if an input is marked as required, its default value will never be used`
  (this check runs even when the default has an expression).

`workflow_dispatch` (`rule_events.go:247-307`):
- choice without options: `input type of %q is "choice" but "options" is not set` (at the input name).
- duplicate option: `option %q is duplicated in options of %q input`.
- default not in options: `default value %q of %q input is not included in its options %q`. The options are joined
  `"a", "b"` and then `%q`-quoted again, giving `"\"Tama\", \"Mike\""`. This is ugly, but needed for parity.
- options on a non-choice input: `"options" can not be set to %q input because its input type is not "choice"`.
- number default: `type of %q input is "number" but its default value %q cannot be parsed as a float number: %s`.
- boolean default: `type of %q input is "boolean". its default value %q must be "true" or "false"`.
- `environment` type: unchecked (TODO at `rule_events.go:276-277`).
- more than 25 inputs: `maximum number of inputs for "workflow_dispatch" event is 25 but %d inputs are provided. see https://docs.github.com/en/actions/using-workflows/events-that-trigger-workflows#providing-inputs`
- Inputs with defaults that contain expressions are **not** skipped for dispatch (unlike workflow_call).

Related parser checks [syntax-check]:
- Unknown dispatch type (`parse.go:439`).
- The unexpected-key list for dispatch inputs shows only `description, required, default` (`parse.go:444`), which omits
  `type` and `options` (upstream bug).
- Unknown workflow_call type (`parse.go:567`), missing `type` (`parse.go:575`), missing output `value` (`parse.go:615`).
- `schedule event must be configured with mapping` (`parse.go:675`).
- Unexpected webhook filter keys (`parse.go:493-531`).

Data, `AllWebhookTypes` (`all_webhooks.go`, 39 entries). Empty `{}` means no types allowed; `nil` means custom
(repository_dispatch).
- branch_protection_rule {created, edited, deleted}
- check_run {created, rerequested, completed, requested_action}
- check_suite {completed}
- create, delete, deployment, deployment_status {}
- discussion {created, edited, deleted, transferred, pinned, unpinned, labeled, unlabeled, locked, unlocked,
  category_changed, answered, unanswered}
- discussion_comment {created, edited, deleted}
- fork, gollum, image_version {}
- issue_comment {created, edited, deleted}
- issues {opened, edited, deleted, transferred, pinned, unpinned, closed, reopened, assigned, unassigned, labeled,
  unlabeled, locked, unlocked, milestoned, demilestoned, typed, untyped}
- label {created, edited, deleted}
- merge_group {checks_requested}
- milestone {created, closed, opened, edited, deleted}
- page_build, public {}
- pull_request and pull_request_target {assigned, unassigned, labeled, unlabeled, opened, edited, closed, reopened,
  synchronize, converted_to_draft, locked, unlocked, enqueued, dequeued, milestoned, demilestoned, ready_for_review,
  review_requested, review_request_removed, auto_merge_enabled, auto_merge_disabled}
- pull_request_review {submitted, edited, dismissed}
- pull_request_review_comment {created, edited, deleted}
- push {}
- registry_package {published, updated}
- release {published, unpublished, created, edited, deleted, prereleased, released}
- repository_dispatch nil
- schedule, status {}
- watch {started}
- workflow_call, workflow_dispatch {}
- workflow_run {completed, requested, in_progress}

### 2.6 `glob` (`rule_glob.go`, `glob.go`)

- Ref globs: `branches`, `branches-ignore`, `tags`, `tags-ignore` of every `WebhookEvent`; `image_version.versions`;
  `jobs.<id>.snapshot.version` (`rule_glob.go:20-44`).
- Path globs: `paths`, `paths-ignore`.
- Empty values are skipped (the parser reports them).
- There is **no expression skip**.
- Position: the value position, `+1` if the scalar was quoted, `+ (err.Column - 1)` (`rule_glob.go:75-86`). This is
  wrong for double-quoted strings with escapes and for block scalars.
- Every message gets the suffix
  `. note: filter pattern syntax is explained at https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#filter-pattern-cheat-sheet`.

Validator (`glob.go`). Columns are **character** columns from Go `text/scanner` (only `Next`/`Peek` are used). The column
is set to 0 after a newline (`glob.go:47-55`).

Base templates:
- `invalid glob pattern. %s%s. %s`, where `%s` is `unexpected EOF` or `unexpected character %q` (Go rune quote,
  `'+'`), followed by ` while checking <what>` (`glob.go:57-69`).
- Ref-char errors: ``character %q is invalid for branch and tag names. %s. see `man git-check-ref-format` for more details. note that regular expression is unavailable``.
  Printable characters are formatted as `'%c'` (`glob.go:71-79`).
- Scanner error: `error while scanning glob pattern %q: %s` (`glob.go:85-87`).

Checks:

| Condition | what / why |
|---|---|
| empty pattern | `glob pattern cannot be empty` (`glob.go:200-203`) |
| ref starts with `/` | invalidRefChar('/', `ref name must not start with /`) (`glob.go:207-212`) |
| `!` alone | `! at first character (negate pattern)` / `at least one character must follow !` (`glob.go:213-219`) |
| `?` or `+` after a special character or at the start | `special character ? (zero or one)` or `special character + (one or more)` / `the preceding character must not be special character` (`glob.go:111-120`) |
| `[]` | `content of character match []` / `character match must not be empty` (`glob.go:124-128`) |
| unclosed `[` | `end of character match []` / `missing ]` (`glob.go:137-139`) |
| `[a-]` | `character range in []` / `end of range is missing` (`glob.go:153-156`) |
| `[9-1]` | `character range in []` / `start of range %q (%d) is larger than end of range %q (%d)` (`glob.go:160-164`) |
| `[x]` single character | `character match []` / `character match with single character is useless. simply use x instead of [x]` (`glob.go:169-171`) |
| `\r`, `\n` | why: `newline cannot be contained` (no what) (`glob.go:172-178`) |
| ref contains space, tab, `~`, `^`, `:` | `ref name cannot contain spaces, ~, ^, :, [, ?, *` (`glob.go:179-182`) |
| ref escapes `\[`, `\?`, `\*` | same why, but the reported character is `Peek()` **after** the escaped one, an upstream bug (`glob.go:97-101`) |
| ref `\` before a non-special character | invalidRefChar('\\', `only special characters [, ?, +, *, \\, ! can be escaped with \\`) (`glob.go:104-109`) |
| ref ends with `/` or `.` | `ref name must not end with / and .` (`glob.go:187-190`) |
| path has leading or trailing spaces | `leading and trailing spaces are not allowed in glob path` (col 0) (`glob.go:247-249`) |
| path `.`, `..`, `./x`, `../x` (after stripping `!`) | `'.' and '..' are not allowed in glob path` (#521) (`glob.go:251-255`) |

Path patterns allow `\` before ordinary characters (`glob.go:105`).

### 2.7 `id` (`rule_id.go`)

- Pattern `^[a-zA-Z_][a-zA-Z0-9_-]*$` (`rule_id.go:8`). It is applied to the job ID, **each `needs` entry**, and step IDs.
  Empty values or values with expressions are skipped (`rule_id.go:61-66`).
- Template: `invalid %s ID %q. %s ID must start with a letter or _ and contain only alphanumeric characters, -, or _`
  (`job`/`step`).
- Step ID duplicates are checked per job, lowercased (`rule_id.go:45-59`):
  `step ID %q duplicates. previously defined at %s. step ID must be unique within a job. note that step ID is case insensitive`
  (only the first duplicate is recorded).
- Duplicate job IDs come from the parser (`parse.go:355`, `key %q is duplicated in %s. previously defined at %s. note that this key is case insensitive`,
  [syntax-check]).

### 2.8 `job-needs` (`rule_job_needs.go`)

- `VisitJobPre`:
  - A duplicate need (lowercased) gives `job ID %q duplicates in "needs" section. note that job ID is case insensitive`
    at the need position (`rule_job_needs.go:58-70`).
  - A job ID duplicate gives `job ID %q duplicates. previously defined at %s. note that job ID is case insensitive`
    (`rule_job_needs.go:76-78`). This is **effectively unreachable**, because the parser drops duplicate job keys
    (`parse.go:350-357`).
- `VisitWorkflowPost`:
  - Unresolved needs: `job %q needs job %q which does not exist in this workflow`. Both IDs are lowercased, and the error
    is reported at the **job ID position** (not the needs entry) (`rule_job_needs.go:94-105`).
  - If any need is unresolved, **cycle detection is skipped**.
- Cycles: DFS with three-colour marking over **map iteration order** (`rule_job_needs.go:164-189`). Only the **first**
  cycle is reported (`rule_job_needs.go:110`). The message starts at the cycle node with the smallest position, which
  makes it deterministic for that cycle (`rule_job_needs.go:116-122`). Which cycle is found first is nondeterministic when
  there are several.
- Template: `cyclic dependencies in "needs" job configurations are detected. detected cycle is "a" -> "b" -> "a"`.

### 2.9 `matrix` (`rule_matrix.go`)

- Skipped if there is no strategy or matrix, or the matrix is a whole expression (`rule_matrix.go:22-24`).
- Duplicate row values use `RawYAMLValue.Equals`: strings exact and case-sensitive; arrays element-wise; objects by key
  set (`ast.go:521-627`). Rows whose values are `${{ }}` are skipped (`rule_matrix.go:46-70`). Template:
  `duplicate value %s is found in matrix %q. the same value is at %s`. `%s` is the RawYAML `String()`:
  - strings → Go-quoted,
  - arrays → `[a, b]`,
  - objects → `{"k": v, ...}` sorted.
- Exclude (`rule_matrix.go:137-216`):
  - Skipped if exclude is empty, or include contains any expression.
  - `"exclude" section exists but no matrix variation exists` (at the matrix position) when there are no rows and no
    include combinations.
  - Rows with an expression are ignored. Include assignments extend the known values per key.
  - Unknown key: `%q in "exclude" section does not exist in matrix. available matrix configurations are %s`.
  - Value not matching: `value %s in "exclude" does not match in matrix %q combinations. possible values are %s`. The
    values are joined `, ` **without** dedup (the docs show `"14", "14"`).
- `isYAMLValueSubset` (`rule_matrix.go:72-135`):
  - An exclude filter with an expression always matches (#414).
  - Object exclude values match as a **subset** of matrix objects (#249).
  - Arrays need equal length.
  - A matrix string containing an expression matches anything (#261).
- The include/exclude combination semantics are not fully simulated. For example, an exclude that removes every
  combination is not detected.

### 2.10 `permissions` (`rule_permissions.go`)

- Workflow and job level.
- A scalar must be `read-all` or `write-all`. Otherwise:
  `%q is invalid for permission for all the scopes. available values are "read-all", "write-all" or {}`
  (`rule_permissions.go:57-65`). `{}` is parsed as an empty mapping and is OK.
- Scope names are **case-sensitive** even though the parser dedups them case-insensitively (`parse.go:745-755`).
  Unknown scope: `unknown permission scope %q. all available permission scopes are %s` (sorted).
- Bad value: `%q is invalid as permission of scope %q. available values are %s` (table order, unsorted).
- Expressions are not special-cased, so `${{ }}` values are reported (GitHub does not allow them anyway).
- Data (`rule_permissions.go:5-22`): actions, artifact-metadata, attestations, checks, contents, deployments,
  discussions, issues, packages, pages, pull-requests, repository-projects, security-events, statuses →
  {read, write, none}; **id-token → {write, none}**; **models → {read, none}**.
- The docs example comment says `"none"` is valid for the whole-workflow value; the code does not accept it.

### 2.11 `runner-label` (`rule_runner_label.go`)

- `VisitJobPre` (`rule_runner_label.go:139-165`):
  - Exactly 1 label → `checkLabel`, which validates only.
  - Otherwise (multiple labels, or a whole-expression `LabelsExpr`) → per-job `compats` map and
    `checkLabelAndConflict`, which also checks conflicts.
  - A `runs-on: {group: ...}` alone is not checked. Group labels are checked like ordinary labels.
- `verifyRunnerLabel` (`rule_runner_label.go:196-233`):
  1. Lowercased lookup in `defaultRunnerOSCompats` (GitHub-hosted plus `linux`/`macos`/`windows`) returns the OS compat
     bitset.
  2. Case-insensitive match against `self-hosted`, `x64`, `arm`, `arm64` → compat invalid (no conflict tracking).
  3. Config `self-hosted-runner.labels` matched with Go **`path.Match(pattern, label)`**, which is **case-sensitive**.
     `*` does not match `/`; `[]` and `\` escapes are supported. A bad pattern gives
     `label pattern %q is an invalid glob. kindly check list of labels in actionlint.yaml config file: %v`.
  4. Otherwise: `label %q is unknown. available labels are %s. if it is a custom label for self-hosted runner, set list of labels in actionlint.yaml config file`,
     where `%s` = `quotesAll(GitHub-hosted list, other presets, OS presets, config labels)` in table order (not sorted).
- Expressions (`rule_runner_label.go:235-290`):
  - Only when the label is **exactly** `${{ matrix.<prop> }}` (parsed with the expression parser, must be an
    `ObjectDerefNode` on variable `matrix`).
  - The values are the string values of `matrix.<prop>` rows plus `include` assignments, skipping strings with
    expressions. Non-string values such as `[self-hosted, linux]` arrays are **skipped**.
  - Anything else (`foo-${{...}}`, `fromJSON`, `env.x`, `matrix.a.b`) is not checked.
- Conflicts (`rule_runner_label.go:292-326`):
  - A label conflicts when `c & comp == 0` with any registered compat.
  - Template: `label %q conflicts with label %q defined at %s. note: to run your job on each workers, use matrix`.
  - Matrix-expanded labels are alternatives. They are checked against earlier labels, then all registered
    (`checkCombiCompat`).
  - Iterating `rule.compats` (a map) makes the **partner label in the message nondeterministic** when several conflict.

Data, GitHub-hosted labels (`rule_runner_label.go:32-62`, in message order): windows-latest, windows-latest-8-cores,
windows-2025, windows-2025-vs2026, windows-2022, windows-11-arm, ubuntu-slim, ubuntu-latest, ubuntu-latest-4-cores,
ubuntu-latest-8-cores, ubuntu-latest-16-cores, ubuntu-24.04, ubuntu-24.04-arm, ubuntu-22.04, ubuntu-22.04-arm,
macos-latest, macos-latest-xlarge, macos-latest-large, macos-26-intel, macos-26-xlarge, macos-26-large, macos-26,
macos-15-intel, macos-15-xlarge, macos-15-large, macos-15, macos-14-xlarge, macos-14-large, macos-14.

Self-hosted presets: other = self-hosted, x64, arm, arm64; OS = linux, macos, windows (`rule_runner_label.go:65-77`).

Compat map (`rule_runner_label.go:79-112`):
- `ubuntu-slim`, `ubuntu-latest*`, `ubuntu-24.04*` → Ubuntu 24.04; `ubuntu-22.04*` → 22.04.
- `macos-latest` → macOS 15 (`-large` → 15L, `-xlarge` → 15XL); `macos-{14,15,26}[-large|-xlarge|-intel]` → their own
  bits.
- `windows-latest` and `-8-cores` → **Windows 2022** (probably stale; GitHub moved `windows-latest` to 2025).
- `linux` = both Ubuntu bits; `macos` = all macOS bits; `windows` = all Windows bits.

**Deprecated or removed labels are not handled specially.** Removed labels (ubuntu-20.04, ubuntu-18.04, windows-2019,
windows-2016, macos-13*, macos-12, macos-11, macos-10.x, `*-xl`) simply produce "label is unknown"
(CHANGELOG.md:105-109, 199-201, 289, 336, 356, 385-386, 543). This is an improvement opportunity: keep a "removed" table
with a dedicated message and a replacement suggestion.

### 2.12 `shell-name` (`rule_shell_name.go`)

- Checks workflow `defaults.run.shell` (always against the platform "any" set), job `defaults.run.shell`, and step
  `shell` against the job's platform (`rule_shell_name.go:34-65`).
- Skipped when the value contains `{0}` (custom shell) or `${{` (`rule_shell_name.go:74-81`).
- Comparison is **lowercased**.
- Platform (`rule_shell_name.go:149-178`), from literal `runs-on` labels only (not matrix/expressions):
  - `windows-*` or `windows` → Windows.
  - `macos-*`, `ubuntu-*`, `macos`, `linux` → macOS/Linux.
  - Mixed → any.
- Shell sets (`rule_shell_name.go:117-147`):
  - any = bash, pwsh, python, sh, cmd, powershell
  - Windows = bash, pwsh, python, cmd, powershell
  - macOS/Linux = bash, pwsh, python, sh
- Template: `shell name %q is invalid%s. available names are %s` (sorted). `%s` = ` on Windows` / ` on macOS or Linux`
  only when the shell is valid on some other platform.
- The docs comment "'sh' is only available on Windows" (docs/checks.md:1946) is a doc typo.

### 2.13 `shellcheck` (`rule_shellcheck.go`). See section 4.

Template: `shellcheck reported issue in this script: SC%d:%s:%d:%d: %s` (code, level, line-1, column, message without
trailing `.`), reported at `run.RunPos` (the `run:` **key**) (`rule_shellcheck.go:231`).

### 2.14 `pyflakes` (`rule_pyflakes.go`). See section 4.

Template: `pyflakes reported issue in this script: %s`, where `%s` is the rest of the line after `<stdin>:`
(e.g. `1:7: undefined name 'hello'`), at `run.RunPos` (`rule_pyflakes.go:168`).

### 2.15 `workflow-call` (`rule_workflow_call.go`, `reusable_workflow.go`)

- `VisitWorkflowPre`: if this workflow has `on.workflow_call`, it registers the metadata in the project cache under the
  spec `./<path relative to project root>`. This avoids re-parsing when another file calls it. It is a no-op if the spec
  is already cached (`rule_workflow_call.go:31-42`, `reusable_workflow.go:264-321`).
  - Inputs: `Required = required && default == nil`; types are mapped to Bool/Number/String/Any.
  - `workflowCallEventPos` is stored but **never used** (dead field).
- `VisitJobPre` applies to jobs with `uses:`. It skips empty values and expressions (`rule_workflow_call.go:45-77`).
  - Local format: starts with `./`, has no `@` after the prefix, and is non-empty. Any `.yml` path is accepted; there is
    no `.github/workflows` check.
  - Repo format: `owner/repo/<path>@<ref>` with non-empty segments (`rule_workflow_call.go:150-192`; tests at
    `rule_workflow_call_test.go:18-33`). `owner/repo/x.yml@@` is valid.
  - Invalid → `reusable workflow call %q at "uses" is not following the format "owner/repo/path/to/workflow.yml@ref" nor "./path/to/workflow.yml". see https://docs.github.com/en/actions/learn-github-actions/reusing-workflows for more details`.
    For an invalid `./...` value, nil is cached so that later lookups do not also say "could not read".
- Local calls (`rule_workflow_call.go:79-146`) use `FindMetadata` (`reusable_workflow.go:207-234`).
  - Unlike local actions, a **missing file is an error**: `could not read reusable workflow file for %q: %w`.
  - Parse errors: `error while parsing reusable workflow %q: %s` (newlines stripped). Inner errors include
    `"on:" is not found` and `"workflow_call" event trigger is not found in "on:" at line:%d, column:%d`
    (`reusable_workflow.go:323-368`).
  - Errors are cached as nil and reported once.
  - Missing required input: `input %q is required by %q reusable workflow` (at `uses`).
  - Undefined input: `input %q is not defined in %q reusable workflow. %s`, where `%s` is one of `no input is defined`,
    `defined input is %q`, or `defined inputs are <sorted quotes>`.
  - Secrets (skipped entirely for `secrets: inherit`): `secret %q is required by %q reusable workflow` and
    `secret %q is not defined in %q reusable workflow. %s` (`no secret is defined` / `defined secret is %q` /
    `defined secrets are ...`).
  - Input types are checked by the `expression` rule.

---

## 3. docs/checks.md catalogue: parity checklist

There are 38 TOC entries (docs/checks.md:8-45). "Impl" is where the code lives; "Kind" is the reported kind.

| # | Section (anchor) | Impl | Kind |
|---|---|---|---|
| 1 | Unexpected keys (`check-unexpected-keys`) | `parse.go:139-148` `unexpectedKey`, called throughout the parser | syntax-check |
| 2 | Missing required keys / key duplicates (`check-missing-required-duplicate-keys`) | dups `parse.go:350-357`; `steps`/`runs-on` missing `parse.go:1471,1474` | syntax-check |
| 3 | Unexpected empty mappings (`check-empty-mapping`) | `parse.go:154,176,320,368` | syntax-check |
| 4 | Unexpected mapping values (`check-mapping-values`) | `parse.go:183` (bool/float/expression), `parse.go:260` (int) | syntax-check |
| 5 | Syntax check for `${{ }}` (`check-syntax-expression`) | expr lexer/parser via `rule_expression.go` | expression |
| 6 | Type checks (`check-type-check-expression`) | `expr_sema.go` via `rule_expression.go` (incl. `env:` object expansion `rule_expression.go:410`) | expression |
| 7 | Contexts & builtin funcs (`check-contexts-and-builtin-func`) | `expr_sema.go` (functions, contexts, `format()`/`fromJSON()` special checks) | expression |
| 8 | `steps.<id>` typing (`check-contextual-step-object`) | `rule_expression.go:328-359` (local action outputs, `PopularActions` outputs, github-script loose) | expression |
| 9 | `matrix` typing (`check-contextual-matrix-object`) | rule_expression | expression |
| 10 | `needs` typing (`check-contextual-needs-object`) | rule_expression | expression |
| 11 | Comparison types (`check-comparison-types`) | expr_sema | expression |
| 12 | shellcheck (`check-shellcheck-integ`) | `rule_shellcheck.go` | shellcheck |
| 13 | pyflakes (`check-pyflakes-integ`) | `rule_pyflakes.go` | pyflakes |
| 14 | Script injection (`untrusted-inputs`) | `expr_insecure.go` via rule_expression (incl. `actions/github-script` `script` input) | expression |
| 15 | Job dependencies (`check-job-deps`) | `rule_job_needs.go` | job-needs |
| 16 | Matrix values (`check-matrix-values`) | `rule_matrix.go`; dup matrix keys in the parser | matrix / syntax-check |
| 17 | Webhook events (`check-webhook-events`) | `rule_events.go:131-202` + `all_webhooks.go`; unknown filter key `parse.go:493-531` | events / syntax-check |
| 18 | Workflow dispatch (`check-workflow-dispatch-events`) | `rule_events.go:247-307`; bad type `parse.go:439`; `inputs`/`github.event.inputs` typing in rule_expression | events / syntax-check / expression |
| 19 | Glob patterns (`check-glob-pattern`) | `rule_glob.go` + `glob.go` | glob |
| 20 | CRON & timezone (`check-cron-syntax-and-timezone`) | `rule_events.go:62-93` (robfig/cron) | events |
| 21 | Runner labels (`check-runner-labels`) | `rule_runner_label.go`; also `runs-on` expression type `rule_expression.go:219` | runner-label |
| 22 | Action format in `uses:` (`check-action-format`) | `rule_action.go:347-396, 515-540` | action |
| 23 | Local action inputs (`check-local-action-inputs`) | `rule_action.go:577-651` + `action_metadata.go` | action |
| 24 | Popular action inputs (`check-popular-action-inputs`) | `rule_action.go:375-392, 599-651` + `popular_actions.go` | action |
| 25 | Outdated popular actions (`detect-outdated-popular-actions`) | `rule_action.go:377-379` + `OutdatedPopularActionSpecs` | action |
| 26 | Shell names (`check-shell-names`) | `rule_shell_name.go` | shell-name |
| 27 | Job/step ID uniqueness (`check-job-step-ids`) | step: `rule_id.go:45-59`; job: parser `parse.go:355` | id / syntax-check |
| 28 | Hardcoded credentials (`check-hardcoded-credentials`) | `rule_credentials.go` | credentials |
| 29 | Env var names (`check-env-var-names`) | `rule_env_var.go` | env-var |
| 30 | Permissions (`permissions`) | `rule_permissions.go` (+ parse `parse.go:741-758`) | permissions |
| 31a | Reusable: workflow_call input definitions | `rule_events.go:205-243`; bad/missing type `parse.go:567,575`; expression-typed defaults `rule_expression.go:130,140` | events / syntax-check / expression |
| 31b | Reusable: workflow call syntax | allowed job keys `parse.go:1461,1479`; format + missing file `rule_workflow_call.go:71-75`, `reusable_workflow.go:221` | syntax-check / workflow-call |
| 31c | Reusable: types of `inputs.*`/`secrets.*` | rule_expression | expression |
| 31d | Reusable: outputs in reusable workflow | rule_expression (`jobs` context); missing `value` `parse.go:615` | expression / syntax-check |
| 31e | Reusable: inputs/secrets in workflow call | required/undefined `rule_workflow_call.go:91-143`; input types rule_expression (`rule_expression.go:525+`) | workflow-call / expression |
| 31f | Reusable: outputs of workflow call in downstream jobs | rule_expression (`getWorkflowCallOutputsType` `rule_expression.go:361+`) | expression |
| 32 | ID naming convention (`id-naming-convention`) | `rule_id.go:61-66` | id |
| 33 | Availability of contexts/special funcs (`ctx-spfunc-availability`) | rule_expression + generated `availability.go` | expression |
| 34 | Deprecated workflow commands (`check-deprecated-workflow-commands`) | `rule_deprecated_commands.go` | deprecated-commands |
| 35 | Constant conditions at `if:` (`if-cond-constant`) | `rule_if_cond.go:40-70` | if-cond |
| 36 | Action metadata syntax (`action-metadata-syntax`) | `rule_action.go:479-574` + `action_metadata.go:35-115,281` | action |
| 37 | Deprecated inputs usage (`deprecated-inputs-usage`) | `rule_action.go:479-491, 615-628` | action |
| 38 | YAML anchors (`yaml-anchors`) | `parse.go:110-131` (unused anchor, recursive alias), `parse.go:1571,1591` (`could not parse as YAML`) | syntax-check |

Checks that exist in code but are **not documented** in checks.md. These are parity items too:
- events:
  - `workflow_run` requires `workflows`; `workflows` only for workflow_run; `"types" cannot be specified`.
  - Dispatch duplicate options, options on non-choice inputs, more than 25 inputs.
  - workflow_call boolean default.
- glob: `image_version.versions` and `snapshot.version` ref globs; the full message table in section 2.6.
- action:
  - Docker URI/tag checks; "owner and repo and ref should not be empty".
  - Local Docker image must be named Dockerfile; `pre-if`/`post-if` requirements; invalid runs props per type; missing
    `runs.using`; local Docker image/entrypoint file existence.
- id: job IDs referenced in `needs` must follow the convention.
- runner-label: invalid glob in the config label list.
- matrix: `"exclude" section exists but no matrix variation exists`.
- job-needs: unknown job in needs.
- workflow-call: undefined secret / required secret; `secrets: inherit` handling.
- parser: `GitHub Actions does not support YAML merge key "<<"` (`parse.go:336`); credentials require both
  username and password (`parse.go:1027`).

Docs inaccuracies to keep in mind:
- The anchor `<a id="#check-deprecated-workflow-commands">` has a leading `#`, so the TOC link is broken
  (docs/checks.md:2734).
- The docs list valid `using` as `composite, docker, node20`, without node24 (docs/checks.md:2945).
- The branding colors list omits `black` (docs/checks.md:2952).
- The permissions example comment claims `"none"` is valid (docs/checks.md:2113).
- The shell-name comment typo (docs/checks.md:1946).

**`scripts/check-checks` as a golden corpus.** It is a state machine (`scripts/check-checks/main.go:157-306`) that, for
every `<a id>` + `## heading` section, takes each `Example input:` ```` ```yaml ```` block and runs the linter with
`StdinFileName: "test.yaml"`, `shellcheck`, `pyflakes`, and no colour (`main.go:20-52`). It replaces the following
`Output:` block with the real output, with the project root rewritten to `/path/to/repo`. It also regenerates
`[Playground](...)` links: zlib (best compression) + base64 of the input with comment lines removed (`main.go:129-155`).
`<!-- Skip update output -->` / `<!-- Skip playground link -->` opt out. Every example must produce at least one error.
CI runs it on Linux/macOS (`README.md:15-19`).

**Rust port:** write an equivalent test harness that parses checks.md and diffs our output against every non-skipped
`Output:` block. This gives about 50 byte-exact end-to-end cases. Add `testdata/examples/*.yaml|.out` (51 pairs),
`testdata/err` (190 files), `testdata/ok` (84) and `testdata/projects` (61). The skip-output sections (local
action/reusable workflow examples) need fixture projects.

---

## 4. External-process rules: shellcheck & pyflakes

### 4.1 Process management (`process.go`)

- Executable resolution (`process.go:123-137`): first `execabs.LookPath(exe)`. This does **not** search the current
  directory on Windows (a CHANGELOG security fix, CHANGELOG.md:1905). If that fails, the value is parsed as a command
  line with `go-shellwords`, and argv[0] is looked up (#483, e.g. `-shellcheck="go run .../shellcheck@latest"`). The
  extra args are prepended (`process.go:154-163`). If the lookup fails, the rule is disabled (not an error).
- Concurrency:
  - **One global** `concurrentProcess` per lint run, using `semaphore.NewWeighted(runtime.NumCPU())`
    (`linter.go:328-329, 453, 496`).
  - Every `run:` step spawns a goroutine immediately (`process.go:88-99`); the process starts only after acquiring the
    semaphore.
  - File reading is separately bounded by a NumCPU semaphore (`linter.go:330, 365-367`). Issue #3: macOS hang / too
    many open files.
  - Each rule instance owns an `errgroup.Group` (`process.go:143-149`). `VisitWorkflowPost` calls `cmd.wait()`, which
    blocks until that file's processes finish (`rule_shellcheck.go:101-104`, `rule_pyflakes.go:85-88`).
  - The global `proc.wait()` (WaitGroup) is called only after all file goroutines finish. This fixes a race between
    `wg.Add` and `wg.Wait` (`linter.go:392-398`, CHANGELOG.md:551).
- Execution (`process.go:24-65`):
  - `cmd.Stdin = strings.NewReader(script)` lets `exec.Cmd` pump stdin from a goroutine. **The earlier approach (writing
    to `StdinPipe()` before `Start`) deadlocked on darwin** when the pipe buffer filled under concurrency (issue #650,
    commit `fd33e9f`).
  - `cmd.Output()`, or `CombinedOutput()` for pyflakes.
  - Exit code < 0 (signal) → `%s was terminated. stderr: %q`.
  - Non-zero exit with empty stdout → `%s exited with status %d but stdout was empty. stderr: %q`.
  - Non-zero exit with stdout is treated as success (the linter found issues).
  - The environment is inherited, so **`SHELLCHECK_OPTS` works** (docs/checks.md:944-962).
- **Failure semantics:** callback errors propagate through errgroup → `VisitWorkflowPost` → `Visitor.Visit` →
  `Linter.check` → `fatal error while checking <file>: ...`. The whole run aborts (`linter.go:378-381`). A crashing or
  misconfigured shellcheck kills the lint. Other in-flight goroutines are not cancelled (errgroup without context).

### 4.2 shellcheck (`rule_shellcheck.go`)

Shell resolution (`rule_shellcheck.go:106-122`). Priority order:
1. step `shell`
2. job `defaults.run.shell`
3. workflow `defaults.run.shell`
4. `pwsh` if any **literal** `runs-on` label is `windows` or `windows-*` (`rule_shellcheck.go:70-80`)
5. `bash`

Matrix or expression `runs-on` falls back to bash, which gives known false positives on Windows matrix jobs. The docs
recommend an explicit `shell:` (docs/checks.md:920-942).

Shell → shellcheck dialect (`rule_shellcheck.go:163-173`): exactly `bash`/`sh`, or prefix `bash `/`sh ` (custom shells
such as `bash -e {0}`). Anything else (pwsh, python, `/bin/bash`, `Bash`, expressions) is **skipped**. The comparison
is case-sensitive.

Preprocessing:
- `sanitizeExpressionsInScript` (`rule_shellcheck.go:130-161`) replaces each `${{ ... }}` (from `${{` to the first
  following `}}`) with the same number of **bytes** of `_`.
  - Line numbers stay intact unless the expression spans newlines (the newlines are replaced, which shifts lines).
  - Multi-byte characters inside the expression shift character columns.
  - Underscores rather than spaces avoid syntax errors such as `if ${{ x }}; then` (CHANGELOG.md:2065).
- A prelude is prepended: `set -e` for sh, or `set -eo pipefail` for bash (the GitHub-documented flags).
  Script = `"<setup>\n<src>\n"` (`rule_shellcheck.go:198-202`).
- Args (`rule_shellcheck.go:193`):
  `--norc -f json -x --shell <sh> -e SC1091,SC2194,SC2050,SC2153,SC2154,SC2157,SC2043 -`. The rationale for each
  exclusion is at `rule_shellcheck.go:178-192`:
  - SC1091: sourced file not found.
  - SC2194, SC2050: constant word/expression created by placeholders.
  - SC2153, SC2154: env vars from `env:`.
  - SC2157: `-z ____`.
  - SC2043: `for x in ____`.

Output:
- `json.Unmarshal` into `[{line, column, level, code, message}]` (`rule_shellcheck.go:10-16, 210-213`).
- Parse failure → fatal `could not parse JSON output from shellcheck: %w: stdout=%q`.
- Line is reported as `line - 1` (for the prelude). The column is passed through.
- Errors are appended under a mutex (`rule_shellcheck.go:218-233`).
- **All findings are reported at the `run:` key position.** The in-script location appears only inside the message. The
  code comment explains that YAML block styles (`|`, `>`, chomping, indentation) make source mapping non-trivial
  (`rule_shellcheck.go:221-226`).
- `-f json` ignores `endLine`, `endColumn` and `fix`. `json1` would provide fix data.
- shellcheck columns treat tabs as width 8. Verify this; it affects in-script column parity.

Not covered:
- Composite actions' `run:` steps.
- `working-directory`.
- `shell: bash` set via an expression.

### 4.3 pyflakes (`rule_pyflakes.go`)

- Python detection (`rule_pyflakes.go:18-26, 105-115`):
  - `shell == "python"` or starts with `"python "` (e.g. `python {0}`). It is case-sensitive; `python3` and
    `/usr/bin/python` are **not** detected.
  - Priority: step, then job default, then workflow default. There is no runner-based default.
- Preprocessing: the same `${{ }}` → underscores, **no prelude**, so lines are not shifted (`rule_pyflakes.go:117-121`).
- Invocation: no args, script on stdin. Output is combined stdout+stderr, because syntax errors go to stderr (#411)
  (`rule_pyflakes.go:54-56`).
- Parsing (`rule_pyflakes.go:139-172`): repeatedly find `<stdin>:`, take the text up to `\n` (strip `\r`), and emit
  `pyflakes reported issue in this script: <rest>`. Text without `<stdin>:` is ignored, which skips the multi-line
  syntax-error caret blocks. A message without a trailing newline is fatal:
  `error message from pyflakes does not end with \n nor \r\n while checking script at %s. output: %q`.
- Errors are reported at `run.RunPos`, under a mutex.

### 4.4 Known issues / risks

| Issue | Where |
|---|---|
| stdin deadlock on darwin (fixed by using `cmd.Stdin`) | `process.go:27-31`, #650 |
| Too many processes or files causing a macOS hang | #3, semaphores (`linter.go:330`, `process.go:72-86`) |
| `wg.Add`/`wg.Wait` race | CHANGELOG.md:551, `linter.go:392-398` |
| Windows executable search in the cwd (security) | `execabs`, CHANGELOG.md:1905 |
| Windows matrix jobs checked as bash | docs/checks.md:920-942 |
| Fatal abort on tool failure | `process.go:50-56`, `rule_shellcheck.go:205-213` |
| check-checks unsupported on Windows | `scripts/check-checks/README.md:9` |
| Throughput: one process per `run:` step | `process.go:88` |

---

## 5. Data provenance & staleness

| Data | Location | Source | Maintenance | Observed churn |
|---|---|---|---|---|
| `PopularActions` (218 specs, with inputs/outputs/required/deprecated, skip flags) + `OutdatedPopularActionSpecs` (156) | `popular_actions.go` (generated; `//go:generate` at `action_metadata.go:14`) | `scripts/generate-popular-actions`: registry `popular_actions.json` (slug, tags, next, path, skip_inputs, skip_outputs, file_ext) → fetch `https://raw.githubusercontent.com/<slug>/<tag><path>/action.<ext>`. "Outdated" = `node<N>` with N < 20, plus a manual list (`scripts/generate-popular-actions/main.go:23-29, 74-87`). Manual metadata patches for dorny/paths-filter@v3 and download-artifact@v3-node20 (`main.go:200-221`) | Weekly CI (`.github/workflows/generate.yaml`, cron `21 1 * * 6`): `-d` detects new tags, then `go generate`, then an auto-PR | High: new major tags several times per year; outdated set grows as runtimes are retired |
| `AllWebhookTypes` | `all_webhooks.go` (generated; `rule_events.go:11`) | Scrapes the docs HTML page (`div[data-search=article-body] div.markdown-body`, h2 per event, first `table[aria-labelledby=<event>]`, "Activity types" column `<code>` items, `Not applicable` → `{}`, `Custom` → nil) (`scripts/generate-webhook-events/main.go:33-253`) | Same weekly CI | Medium: new types added (e.g. issues typed/untyped) |
| Context availability | `availability.go` (generated, `rule_expression.go:8`) | Docs scrape | weekly | (other report) |
| Runner labels + OS compat bitsets | `rule_runner_label.go:8-112` | Hand-maintained | Manual PRs | **Very high**: 7+ label changes in v1.7.8–v1.7.12 alone (CHANGELOG.md:13,35,105-109,133,196-204) |
| Permission scopes/levels | `rule_permissions.go:5-22` | Hand-maintained | Manual | Medium (models 2025, artifact-metadata v1.7.9) |
| Valid `runs.using` (`node20`, `node24`) | `rule_action.go:502-505` | Hand | Manual | Low–medium (node24 v1.7.8) |
| Branding icons/colors | `rule_action.go:14-288` | Hand (Feather icons) | Manual | Low |
| Docker registry prefixes | `rule_action.go:291-297` | Hand | — | Low |
| Shell names per OS | `rule_shell_name.go:117-147` | Hand | — | Very low |
| Deprecated commands regex | `rule_deprecated_commands.go:5` | Hand | — | Very low |
| Filter availability table | `rule_events.go:158-175` | Hand | — | Low |
| Dispatch max inputs (25), cron min interval (5 min) | `rule_events.go:77, 300` | Hand | — | Low (10 → 25 in Dec 2025) |
| shellcheck exclusions | `rule_shellcheck.go:193` | Hand | — | Low |
| IANA tz database | Go `time/tzdata` embedded in the CLI | Go toolchain | Toolchain bump | Low |

**Rust port implications.**
- Keep the generated data as **checked-in Rust source** (or checked-in JSON plus a `build.rs` that turns JSON into
  `phf`/sorted static slices). Do not fetch from the network in `build.rs`: it breaks reproducible and offline builds,
  crates.io packaging and docs.rs.
- Generators belong in `cargo xtask gen-popular-actions | gen-webhooks | gen-availability`. Run them from a weekly CI job
  that opens a PR, mirroring upstream. Reusing upstream's `popular_actions.json` registry format makes it easy to diff
  with upstream.
- Move hand-maintained tables (runner labels, permissions, runtimes) into **data files** (TOML/JSON) compiled in the same
  way, so updates are data-only diffs.
- Add a *removed/deprecated* runner-label table (label, removal date, suggested replacement) for better messages and
  autofixes.
- Optionally allow **runtime overrides/extensions** in config: extra known labels already exist; add extra permission
  scopes and extra webhook types for GHES. Optionally offer an opt-in `--fetch-actions` mode with an on-disk cache keyed
  by `owner/repo@ref`. That would resolve non-popular and SHA-pinned actions (a major false-negative area today), but it
  must never be the default.
- Embed the tz database through `chrono-tz` or `jiff`'s bundled tzdb, and decide on `UTC` acceptance (section 6).

---

## 6. Weak spots: false positives and negatives, nondeterminism

**Nondeterminism**
1. Jobs are visited in Go map order (`pass.go:67`, `ast.go:973`). Messages that depend on order:
   - which cycle `job-needs` reports when there are several (`rule_job_needs.go:110, 164-173`);
   - which conflicting label is named in `runner-label` (`rule_runner_label.go:293`);
   - which file or step reports a project-cached local-action parse/metadata error or reusable-workflow read/parse error
     (`action_metadata.go:243-289`, `rule_action.go:587`, `reusable_workflow.go:200-234`). These are reported **once
     per project run**, and when files are linted in parallel the reporting file is racy.
2. Dedup ignores kind (`error.go:174-179`).

**False negatives**
- `uses:` with an expression is skipped by `action` and `workflow-call`.
- Popular-action lookup is an exact, case-sensitive spec. SHA/patch pins and `@main` are unchecked
  (docs/checks.md:1873).
- A missing local action is silent (by design).
- Composite actions' `steps` are never linted (docs/checks.md:2958). Local action metadata is checked only when used
  from a workflow (docs/checks.md:2955).
- Deprecated commands with digits in the name, or with empty values, are missed. Errors are reported at the `run:` value
  start, not the line.
- The cron frequency check only compares the first two firings after the epoch. `10,40,42 * * * *` (2-minute gap later
  in the hour) passes (`rule_events.go:70-79`).
- robfig accepts the `TZ=`/`CRON_TZ=` prefix and the non-POSIX `N/step` form.
- repository_dispatch types are unchecked; the dispatch `environment` type is unchecked (TODO `rule_events.go:276`).
- Runner labels from `fromJSON`, prefixed/suffixed expressions, nested matrix props, or **array-valued matrix entries**
  are unchecked (`rule_runner_label.go:240-289`). `runs-on.group` is unchecked.
- shell-name, shellcheck and pyflakes platform detection uses literal labels only. Workflow-level `defaults.run.shell` is
  validated against the "any" set.
- Reusable workflow local format does not enforce `.github/workflows/` or a `.yml`/`.yaml` extension. Remote reusable
  workflows are not checked.
- Credentials with `${{ 'literal' }}` pass. Env var names starting with digits pass.
- pyflakes misses `python3` and absolute paths. shellcheck misses `/bin/bash`, `Bash`, and similar.
- Matrix include/exclude semantics are approximated. Exclusion of every combination is not detected.
- `ContainsExpression` uses the first `}}` index (`ast.go:47-50`), so `}} ${{ x }}` counts as "no expression".

**False positives / questionable**
- Timezone `UTC` is rejected (`rule_events.go:85`), although it is an IANA link.
- `windows-latest` mapped to Windows 2022 (`rule_runner_label.go:103-104`) is stale. It only affects conflict checks.
- Default-shell assumption of bash for matrix Windows jobs leads to bogus shellcheck findings (docs/checks.md:920-935).
- Placeholder sanitization can still create shellcheck noise; that is why 7 SC codes are globally disabled, which also
  hides genuine findings.
- Config label patterns are case-sensitive (`path.Match`) while built-in labels are case-insensitive.
- Glob columns are wrong for escaped double-quoted or block scalars (`rule_glob.go:78-84`). The ref escape error reports
  the wrong character (`glob.go:100`).

**Message bugs to replicate, or consciously fix behind a compat flag**
- The color check message says `branding.icon` (`rule_action.go:565`).
- Double-quoted options list in the dispatch default message (`rule_events.go:268`).
- Dispatch unexpected-key list missing `type`/`options` (`parse.go:444`).
- Docker-action message `(tag=%s)`.
- The `rule_deprecated_commands.go:7` comment says "set-state".

**Positions**
- All shellcheck and pyflakes findings point at `run:`. Deprecated commands point at the run value start. job-needs
  unknown-job points at the job, not the need. Action metadata errors all point at `uses:`. Exact source mapping is the
  biggest UX improvement available.

**Robustness**
- External-tool failure aborts the whole lint.
- The pyflakes parser relies on the `<stdin>:` prefix.
- Shellcheck JSON parse failure is fatal.

---

## 7. Rust design recommendations

**Rule trait & registry**
```rust
pub struct RuleMeta { id: RuleId, name: &'static str, description: &'static str,
                      default_severity: Severity, docs_anchor: &'static str, fixable: Fixability }
pub trait Rule: Send {
    fn meta(&self) -> &'static RuleMeta;
    fn workflow_pre(&mut self, _w: &Workflow, _cx: &mut Ctx) {}
    fn job_pre(&mut self, _j: &Job, _cx: &mut Ctx) {}
    fn step(&mut self, _s: &Step, _cx: &mut Ctx) {}
    fn job_post(&mut self, _j: &Job, _cx: &mut Ctx) {}
    fn workflow_post(&mut self, _w: &Workflow, _cx: &mut Ctx) {}
}
```
- `Ctx` holds the diagnostics sink (rule ID attached automatically), config, project caches, debug logger, and
  file/source info. Rules don't own `errs`, so there is no per-rule mutex.
- Keep `RuleId` as a `#[non_exhaustive]` enum whose string forms match upstream kinds exactly (`syntax-check`, `matrix`,
  ...). That keeps `-ignore`, `-format {{kind}}` and SARIF stable.
- Instantiate rule state **per file** (as upstream does). Keep the upstream registration order (section 1.3); it matters
  for which rule reports cache errors.
- Prefer a static `Vec<Box<dyn Rule>>` built from a const registry. The rule count is small, so dynamic dispatch overhead
  is negligible. An enum is possible if profiling says otherwise.
- Keep a `RuleFactory` hook equivalent to `OnRulesCreated` for library users.

**Determinism (improvement with parity-safe output)**
- Store jobs in an `IndexMap` in source order. Visit in source order, iterate any maps in sorted/source order, report
  cycles by smallest position, and pick the conflict partner by earliest position.
- For project caches, use per-key `OnceLock` results and **attach cached errors deterministically**. Either report them
  at every use (behaviour change), or report at the first use in sorted file order. Document the choice.
- Keep final sort (file, line, col, message) and dedup that ignores kind, for parity.

**Enabling/disabling & severity**
- Config:
  `rules: { shellcheck: off, runner-label: { severity: warning, ... } }`.
  CLI: `--disable <id>`, `--enable <id>`, `--severity <id>=<lvl>`. Keep `-ignore` regex semantics (unanchored, message
  only). Use `regex` with RE2-compatible syntax; Go and Rust RE2 differ slightly (Unicode classes), so validate.
- Inline suppressions, which upstream lacks: `# workflint-disable-next-line <id>[,id]`, `# workflint-disable <id>` per
  file. This needs comment spans from the YAML layer.
- Severity levels: error / warning / info (/ hint). Default **everything = error** for parity and the exit code. Map
  shellcheck `error|warning|info|style` and pyflakes findings to severities optionally.

**External tools**
- Bounded worker pool (NumCPU), shared across files.
- Spawn with piped stdin/stdout/stderr and write stdin from a separate thread or task while reading output. This avoids
  the #650 deadlock class: never write a large stdin before draining stdout.
- Make tool failures **non-fatal diagnostics** (kind `shellcheck`, "could not run ..."), with an opt-in strict mode.
- Speed:
  - Batch many scripts into one shellcheck invocation (temp files, multiple file args; JSON includes `file`).
  - Content-hash cache of results (in-memory, optionally on disk).
  - Pyflakes also accepts multiple files.
- Use `-f json1` to get `endLine`/`endColumn`/`fix`.
- Build a **source map** from script offsets to YAML positions for plain/literal (`|`, `|-`, `|+`) scalars, which covers
  most `run:` blocks. Report the precise position and fall back to the `run:` key for folded or quoted scalars.
- Keep the `${{ }}` → `_` sanitization byte-for-byte (including newline replacement) in parity mode. Optionally add an
  improved mode that keeps newlines and replaces by character width.
- Shell detection improvements: case-insensitive comparison, `/bin/bash`, `python3`, and matrix-aware OS inference using
  the same matrix resolution as runner-label.

**Data**
- Generated tables are checked in (section 5). Implement Go-compatible helpers: `go_quote`, Go `path.Match`, a robfig
  cron parser clone (5-field, same error strings), and Go `ParseFloat` acceptance and error text.

**Autofix potential** (via a `Fix { edits: Vec<TextEdit> }` on diagnostics; needs byte spans in the AST)

| Rule | Fix | Safety |
|---|---|---|
| deprecated-commands | Rewrite `echo "::set-output name=N::V"` to `echo "N=V" >> "$GITHUB_OUTPUT"` (and the state/env/path variants) | Safe for the simple `echo` forms |
| if-cond | Unwrap `if: ${{ x }} ` with extra characters / block scalar to a bare expression; drop `if: true` | Safe / suggestion |
| action (outdated) | Bump `owner/repo@vN` to the newest non-outdated major in the data set | Suggestion (breaking-change risk) |
| action (inputs) | Did-you-mean rename of an unknown `with:` key (edit distance to a known input) | Suggestion |
| action (deprecated input) | None by default; show the deprecation message | — |
| runner-label | Typo did-you-mean; replace a removed label with its successor (ubuntu-20.04 → ubuntu-latest) | Suggestion |
| permissions | Scope typo (`check` → `checks`); `write` → `write-all` | Suggestion |
| events | Event/activity-type typo did-you-mean; remove the conflicting `*-ignore` | Suggestion |
| glob | Strip leading `./`; trim spaces; `[x]` → `x` | Safe |
| matrix | Remove a duplicate row value | Safe |
| job-needs | Remove a duplicate `needs` entry | Safe |
| events (workflow_call) | Remove `default:` on a required input, or `required: true` | Suggestion |
| shell-name | Normalize case; did-you-mean | Safe / suggestion |
| shellcheck | Apply `json1` `fix` replacements through the source map (literal blocks only) | Safe where mapped |
| env-var, credentials, id, workflow-call, pyflakes | No mechanical fix (credentials: suggest moving to `secrets.*`) | — |
