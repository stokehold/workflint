# 01: Workflow parsing layer and data model (actionlint, Go). Research for the Rust port

Upstream revision studied: `v1.7.12-10-g011a6d1` (commit `011a6d15e749`, 2026-04-19), YAML dependency
`go.yaml.in/yaml/v4 v4.0.0-rc.3` (go.mod).

Files read in full: `parse.go`, `ast.go`, `pass.go`, `quotes.go`, `glob.go`, `action_metadata.go`,
`reusable_workflow.go`, `project.go`, `config.go`, `all_webhooks.go` and all their `_test.go` files.
I skimmed `popular_actions.go` and `scripts/generate-popular-actions/`. I read the relevant parts of
`linter.go`, `rule.go`, `error.go`, `rule_expression.go`, `rule_glob.go`, `rule_action.go` and
`rule_workflow_call.go`. I also read the testdata under `testdata/{action_metadata,reusable_workflow_metadata,config,find_project,projects}`
and the syntax-check related cases in `testdata/err`.

Method: besides reading the code, I ran experiments.

- `yamlprobe` (in a scratch directory, not committed) is a Go program linked against the reference checkout. It dumps `yaml.Node` trees and runs
  `actionlint.Parse`, `Linter.Lint` and the glob validators.
- `rsprobe` (saphyr-parser 0.2.2) and `lyprobe` (libyaml-safer 0.3.0) are Rust programs in a scratch directory that dump
  events and spans so the candidate crates can be compared with go-yaml.

Statements marked **(verified)** were observed in these experiments, not only inferred from the code.

---

## 1. The YAML library and exactly what actionlint relies on

### 1.1 Entry points

| Consumer | API used | Location |
|---|---|---|
| Workflow parsing | `yaml.Unmarshal(b, &yaml.Node)`, then a hand-written walk over the node tree | `parse.go:1601-1615` |
| Action metadata (`action.yml`) | `yaml.Unmarshal(b, &ActionMetadata)`: struct decoding plus custom `UnmarshalYAML(*yaml.Node)` for inputs and outputs | `action_metadata.go:35-115, 266` |
| Reusable workflow metadata | `yaml.Unmarshal(src, &struct{ On yaml.Node \`yaml:"on"\` })`, then `Node.Decode` into structs with custom unmarshalers | `reusable_workflow.go:323-368` |
| Config (`actionlint.yaml`) | `yaml.Unmarshal(b, &Config)`: struct decoding plus custom `IgnorePatterns.UnmarshalYAML` | `config.go:30-44, 91-103` |
| Popular-actions generator | `yaml.Unmarshal(body, &ActionMetadata)` | `scripts/generate-popular-actions/main.go:154` |

Only the workflow parser uses the raw `Node` API. The other three consumers use go-yaml's reflection
decoder, so a port has to reproduce **two** behaviours: the raw tree and the typed decoder, with its
YAML 1.1 bool compatibility and duplicate-key checks (§1.10).

### 1.2 `yaml.Node` fields actually used

`yaml.Node` (go-yaml `yaml.go:451-495`) has the fields `Kind, Style, Tag, Value, Anchor, Alias, Content,
HeadComment, LineComment, FootComment, Line, Column`. actionlint uses:

- **Kind**: `DocumentNode`, `SequenceNode`, `MappingNode`, `ScalarNode`, `AliasNode`. Names are printed in
  messages by `nodeKindName` (`parse.go:15-30`): `document`, `sequence`, `mapping`, `scalar`, `alias`.
  Alias kind does appear in messages, because recursive aliases are left unresolved (§1.4).
- **Tag**: the *resolved short tag* (`!!str`, `!!int`, `!!float`, `!!bool`, `!!null`, `!!map`, `!!seq`,
  `!!timestamp`, `!!binary`, `!!merge`, or a custom tag verbatim such as `!custom`). It is used for
  decisions and printed in messages (`... node with "!!int" tag`).
- **Value**: the decoded scalar text (unescaped, unquoted, block-folded). An alias node's `Value` is the
  alias name.
- **Style**: only the bit test `Style & (DoubleQuotedStyle|SingleQuotedStyle) != 0`, giving
  `String.Quoted` (`parse.go:37`). Literal (`|`), folded (`>`) and flow styles are **discarded**.
- **Anchor / Alias**: used for alias resolution and anchor diagnostics (`parse.go:91-134`).
- **Content**: children. Mappings are flattened `[k0, v0, k1, v1, ...]`.
- **Line / Column**: the only positional information. There is no byte offset and no end position.

Not used: all three comment fields. actionlint has **no** in-file suppression comments. Also unused:
`LongTag/ShortTag`, the document-level directives, and any end marks.

### 1.3 Position semantics (verified)

`posAt(n) = &Pos{n.Line, n.Column}` (`parse.go:32-34`). `Pos` is documented as 1-based (`ast.go:13-18`).

- **Line**: 1-based. CRLF counts as one line break, and a UTF-8 BOM is skipped without being counted
  (verified with a BOM+CRLF file: the first key is at `1:1`).
- **Column**: 1-based, counted in **Unicode code points** (runes), not bytes or display width. Verified:
  in `"日本": v` the value `v` is at column 7. A byte count would give 10. A tab counts as 1.
- **Which character a node points at**: the event *start mark*. In libyaml terms this is the beginning of
  the node **including its properties**:
  - A node with an anchor or tag points at the `&anchor` or `!tag` token. For example, `on: &anchor push`
    reports the scalar at `2:5`, which is the `&` (testdata `err/unused_anchors`).
  - A quoted scalar points at the opening quote.
  - A block scalar (`|`, `>`, `|-`, ...) points at the **indicator character**, not at the first content
    line. For example, `- run: |` gives the `|` column on the `run:` line.
  - An implicit null (`key:` with nothing after it) points at the column *right after the colon*. For
    example, `on:` gives `1:4` and `    permissions:` gives `12:17` (testdata `err/empty_on`,
    `err/issue170_empty_permissions`).
  - A block mapping points at its first key. A block sequence points at the first `-`. A flow collection
    points at `[` or `{`.
  - A document node with no content has `Line == Column == 0`. `parse()` patches this to `1:1`
    (`parse.go:1504-1509`).
- **Syntax-error positions are different and inconsistent** (§1.11): the column is 0-based and the line
  sometimes comes from a "context" mark.

Consequences:

1. Error rendering (`error.go:135-158`, `getIndicator`) uses `Column-1` as a **byte** index into the
   line (`line[start:]`). The caret is therefore misplaced, and it could even slice inside a UTF-8
   sequence, whenever multi-byte characters precede the error column. This is a latent bug.
2. Rules locate `${{ }}` inside strings by adding **byte** offsets of the decoded value to the node
   column (`rule_expression.go:735-765`, `rule_glob.go:74-84`). They add `+1` when `Quoted`. Verified
   wrong positions:
   - `run: |` block: an expression on line 8 is reported at `6:31` (the indicator line plus a byte offset).
     There is a `// TODO: Line number is not correct when the string contains newlines.` at
     `rule_expression.go:736`.
   - `run: "a\tb ${{ github.bar }}"`: off by one, because the escape `\t` is 2 source characters but
     1 decoded character.
   - `run: "日本 ${{ github.baz }}"`: off by 4 (bytes counted against a rune column).
   - Folded `>`: an expression on line 13 is reported at `11:20`.

   A Rust port should fix this by keeping raw source spans (§9.4).

### 1.4 Anchors and aliases

- go-yaml resolves alias names to `*Node` pointers while parsing. An **unknown anchor** is a fatal decode
  error. It is raised with `failf` (not `ParserError`), so actionlint hits the fallback branch
  (`parse.go:1588`). The result is the message `could not parse as YAML: yaml: unknown anchor 'X' referenced`
  at **0:0**. Note the doubled `yaml:` prefix and the missing position (testdata `err/undefined_anchor.out`,
  `examples/dangling_alias.out`). Only the first dangling alias is ever reported, because parsing aborts.
- After decoding, `parser.resolveAliases` (`parse.go:91-134`) makes a pre-order walk:
  - Every node with a non-empty `Anchor` gets a `usage{used, defined}` record, keyed by node pointer.
    Re-defining the same anchor name creates separate records, because the key is the node.
  - For each child that is an `AliasNode`, the target's record is marked `used`. If the target has
    finished visiting (`defined == true`), the alias in `Content` is **replaced by the target pointer**
    (`n.Content[i] = c.Alias`). This makes the tree a DAG: the same subtree is reachable from several
    parents.
  - If the target is an ancestor that is still being visited, the alias is **recursive**. It is reported
    as `recursive alias %q is found. anchor was declared at line:%d, column:%d` and the alias node is
    **left in place** (issue #610: avoids infinite recursion in `parseRawYAMLValue`). Later parse
    functions then see an `AliasNode` and produce kind errors, for example
    `"env" section is alias node but mapping node is expected` or
    `expected scalar node for string value but found alias node with "" tag`
    (testdata `err/recursive_anchors.out`).
  - After the walk, every anchor whose record is unused is reported:
    `anchor %q is defined but not used`, at the anchored node's position (the `&` token). Iteration is
    over a Go map, so the order is random. The errors are sorted later.
- Because aliased subtrees are re-parsed for every use, **errors inside an anchored subtree are reported
  at the anchor's original location**, once per use. `Linter.check` then sorts and `slices.CompactFunc`s
  identical errors (same file, line, column and message, `linter.go:638-639`, comment "Alias may duplicate
  errors"). See `err/dedup_errors`: 12 uses of `*step` give a single error at the anchor. Errors that
  depend on the *use site* are attributed to the anchor. For example, in `err/errors_in_anchor` the
  pull_request activity-type error is reported at `4:22` (the `&created` definition under `label:`).
- Aliases used as mapping keys are resolved the same way, because keys are just `Content` entries.

### 1.5 Merge keys (`<<`)

- go-yaml resolves a plain `<<` key to tag `!!merge` but leaves it as a plain key in the `Node` tree.
  It does not apply the merge.
- `parseMapping` rejects any key whose **value** is `<<`, including a quoted `"<<"`, which YAML treats as
  an ordinary string. The message is `GitHub Actions does not support YAML merge key "<<"`, at the key
  position (`parse.go:335-338`). The entry is skipped. Its value is not parsed and it is not counted for
  emptiness.
- In the struct-decoded files (`action.yml`, config) go-yaml's decoder **does** apply merges for
  struct-typed parts. The custom `ActionMetadataInputs.UnmarshalYAML` iterates `n.Content` raw, so a
  `<<` key under `inputs:` would become an input literally named `<<` (`action_metadata.go:49-80`).

### 1.6 Tags and the scalar resolver (verified, `resolve.go`)

go-yaml v4 resolves plain scalars as follows. This is *not* exactly YAML 1.2 core and *not* YAML 1.1.

| Input (plain) | Resolved tag |
|---|---|
| `true True TRUE false False FALSE` | `!!bool` (but see `parseBool` bug, §8) |
| `yes no on off y n Yes ON` ... | `!!str`. So `on:` is the string key `on`, which is critical for workflows. |
| empty, `~`, `null`, `Null`, `NULL` | `!!null` |
| `.nan .NaN .NAN`, `[+-].inf ...` | `!!float` |
| `-0`, `-0.0` | `!!float` |
| decimal, `0x1F`, `0o17`, `0b101`, with `_` separators (`1_000`), signed | `!!int` (`ParseInt(strings.ReplaceAll(s,"_",""), 0, 64)`) |
| `017` | `!!int` (decimal 17) |
| `1e3`, `+.5`, `1.5` | `!!float` (regex `^[-+]?(\.[0-9]+|[0-9]+(\.[0-9]*)?)([eE][-+]?[0-9]+)?$`) |
| `2024-01-01`, `2001-12-14T21:59:43Z`, ... | **`!!timestamp`** |
| `1:20` | `!!str` (no sexagesimal) |
| `<<` | `!!merge` |
| quoted or block scalars | always `!!str` |
| explicit `!!str 5` | `!!str` |
| explicit `!!int abc` | tag stays `!!int`, value `abc`. **Not validated** when decoding into `Node`. |
| explicit `!custom x` | `!custom`, kept verbatim |
| explicit `!!binary aGk=` | `!!binary` |

Where actionlint looks at `Tag`:

- `checkString` deliberately ignores the tag (`parse.go:168-180`). Any scalar, including `42`, `true`,
  `~`, `!!timestamp` or custom-tagged, is accepted as a string with its *source text* as value. So
  `run: ~` and `run: null` yield the script text `~` and `null` rather than an error. Only a truly empty
  scalar (`run:`) gives `string should not be empty`.
- `parseBool` accepts tags `!!bool` and `!!str` only. `parseInt` accepts `!!int` and `!!str`. `parseFloat`
  accepts `!!float`, `!!int` and `!!str`. `!!str` means "must be a single `${{ }}` expression". Anything
  else gives a tag error message, for example
  `expected scalar node for float value but found scalar node with "!!bool" tag`.
- `mayParseExpression` requires tag `!!str` (`parse.go:194-202`).
- `parseMapping` and `parseStringOrStringSequence` treat `ScalarNode` with tag `!!null` as "empty
  section" (`parse.go:229, 318`).
- Messages print the tag, so tag names are user-visible output.

Typed (struct) decoding into Go `bool` additionally accepts YAML 1.1 booleans
`y Y yes Yes YES on On ON` (and the false variants) (go-yaml `decode.go:696-699`). The project test
`projects/local_action_yaml_1.1_boolean` relies on `required: yes|on|no|off` in `action.yml` being accepted.

### 1.7 Scalar styles

Only "quoted or not" survives into the AST. The decoded `Value` of literal and folded block scalars has
already been folded, chomped and de-indented, so the AST has no way to map value offsets back to source
(§1.3). Rust should keep the style plus the raw source span of every scalar.

### 1.8 Comments

Comments are parsed by go-yaml (`HeadComment` and so on, verified) but never read. There is no
`# actionlint-disable` facility. Suppression is only by `-ignore` regex or the config `paths.*.ignore`
regex. This is a possible improvement for Rust, which needs comment spans from the YAML layer.

### 1.9 Multi-document streams

`yaml.Unmarshal` into a `Node` decodes **only the first document**. Later documents are silently ignored.
An empty stream or comments only gives `Kind == 0` and no `Content`. `parse()` reports
`workflow is empty` at `1:1` (`parse.go:1511-1514`). `---\n...\n---\non: push` gives a first document
that is a null scalar, so the result is
`"workflow" section should not be empty. please remove this section if it's unnecessary` plus
`"on"`/`"jobs" section is missing` (verified).

### 1.10 Duplicate keys and case sensitivity

- Decoding into `yaml.Node` performs **no** duplicate-key check (verified). actionlint does it itself in
  `parseMapping` (`parse.go:330-365`): `keys map[id]*Pos`, where `id` is `strings.ToLower(key)` if the
  section is case-insensitive. The **first** occurrence wins. A duplicate is reported as
  `key %q is duplicated in %s. previously defined at line:L,col:C[. note that this key is case insensitive]`
  and the duplicate entry is **skipped entirely**, so its value is never parsed and errors inside it are
  never reported (verified: a second job `a:` hides all errors in its body).
- Struct decoding (config, `action.yml`, reusable-workflow `on:` extraction) uses go-yaml's `uniqueKeys`
  (default true, `decode.go:781-789, 916-940`). Exact-match duplicate keys error with
  `mapping key "x" already defined at line N`.
- Unknown keys in struct decoding are ignored (`knownFields` is false). This applies to config and
  `action.yml` top level.
- The case-sensitivity of each section is a parser decision; see the table in §3.4.

### 1.11 YAML-level errors (`handleYAMLUnmarshalError`, `parse.go:1566-1596`)

- `*yaml.TypeError` (multiple `UnmarshalError{Err, Line, Column}`): each becomes
  `could not parse as YAML: <msg>` at its position. This branch is effectively unreachable when the target
  is a `yaml.Node`.
- `*yaml.ParserError{Message, Line, Column}` becomes one error `could not parse as YAML: <Message>`
  (without the `yaml: line N:` prefix that `Error()` would add). The line and column come from go-yaml's
  `parser.fail()` (`decode.go:110-139`):
  - `line = ContextMark.Line` if it is non-zero, otherwise `ProblemMark.Line`. libyaml marks are 0-based,
    and `+1` is added **only for scanner errors**.
  - `column = ContextMark.Column` if it is non-zero, otherwise `ProblemMark.Column`. This is **0-based and
    never +1**.

  The result is an odd mix. Some verified examples:

  | Input | go-yaml position | Real position (1-based) |
  |---|---|---|
  | `steps: -` | `2:7` | `2:8` |
  | `a: [1, 2⏎` | `1:3` | context `[` at `1:4`, problem at `2:1` |
  | `jobs: [⏎` at EOF | `2:0` | |
  | `\tjobs: {}` (tab indentation) | `2:4` | |

  `testdata/err/yaml_syntax_error.out` and `examples/broken_yaml.out` pin these quirky positions. I checked
  that this formula reproduces go-yaml's numbers exactly when it is applied to libyaml-safer's
  `problem_mark` and `context_mark` (§9.1).
- Any other error, for example `failf("unknown anchor ...")`, becomes `err.Error()` at `0:0`.
- On any YAML error `Parse` returns `(nil, errs)`. Exactly one error is reported (YAML parsing aborts),
  and **no rules run** (`linter.go:551`).
- Fuzz harness comment: "It panics when go-yaml panics" (`fuzz/check.go`). go-yaml can panic on some
  inputs. `actionlint.Parse` can also panic by itself (§8, NaN).

### 1.12 Summary of YAML-lib quirks that leak into behaviour

1. `on`, `yes` and `no` are strings in the tree, but typed decoding accepts YAML 1.1 booleans.
2. `!!timestamp` exists. A date-like plain scalar is not `!!str`, so `parseBool`, `parseInt` and
   `parseFloat` print `"!!timestamp" tag` errors. `mayParseExpression` is unaffected.
3. Ints accept `_`, `0x`, `0o` and `0b`, but actionlint then calls `strconv.Atoi`. `max-parallel: 1_000`
   or `0x10` therefore gives
   `invalid integer value: "1_000": strconv.Atoi: parsing "1_000": invalid syntax`, leaking Go's
   `strconv` error text (verified). `timeout-minutes: 0x10` gives
   `invalid float value: "0x10": strconv.ParseFloat: ...`.
4. Explicit tags are not validated, so `!!float NaN` reaches `strconv.ParseFloat` successfully, which
   triggers a panic (§8).
5. Unknown-anchor errors have no position and a doubled `yaml:` prefix.
6. Syntax-error columns are 0-based.
7. Only the first document is linted.

---

## 2. The AST (`ast.go`)

General conventions:

- Every leaf scalar is a `*String`, `*Bool`, `*Int` or `*Float` carrying a `*Pos`. `nil` means "key
  absent", or sometimes "invalid and already reported".
- Positions are heap pointers (`*Pos`) and are shared freely. An alias makes the same `*Pos` appear in
  several places.
- Maps keyed by name hold **lower-cased** keys when the section is case-insensitive. The original
  spelling is kept in the value's `Name *String`.
- Go maps are unordered. Source order is **lost** for jobs, env, with, outputs, matrix rows, secrets,
  services and permissions. The exception is `WorkflowCallEvent.Inputs`, which is a slice on purpose
  (`ast.go:319-322`).
- Sum types are Go interfaces (`Event`, `Exec`, `RawYAMLValue`) discriminated by type switch.

### 2.1 Primitive wrappers

| Type | Fields | Notes |
|---|---|---|
| `Pos` (`ast.go:13-33`) | `Line, Col int` | 1-based; Col in runes. `String()` gives `line:%d,col:%d`. `IsBefore`. |
| `String` (`36-43`) | `Value string; Quoted bool; Pos *Pos` | `Value` is the decoded text. `Quoted` is true for `'..'`/`".."` only (not block styles). `Pos` is the start of the node (the opening quote if quoted). |
| `Bool` (`72-89`) | `Value bool; Expression *String; Pos *Pos` | Exactly one of literal or `Expression`. `String()` gives the expression text or `"true"`/`"false"`. |
| `Int` (`92-99`) | `Value int; Expression *String; Pos *Pos` | |
| `Float` (`102-109`) | `Value float64; Expression *String; Pos *Pos` | |

Expression helpers:

- `ContainsExpression(s)` (`ast.go:47-50`): `i := Index(s,"${{"); i >= 0 && i < Index(s,"}}")`. This is
  quirky: `"}} ${{ x }}"` returns false because the *first* `}}` precedes `${{`.
- `isExprAssigned(s)` (`57-64`): `TrimSpace(s)` has prefix `${{`, suffix `}}`, and
  `Count(s,"${{") == 1`. `}}` may occur inside, which supports JSON literals
  (`ast_test.go:5-33`). `"${{...}}${{...}}"` gives false.

**How expressions are represented.** The workflow AST never parses expressions. A field "contains an
expression" only as a `String` whose `Value` contains `${{ ... }}`. The expression rule
(`rule_expression.go`) later scans strings with `strings.Index(s, "${{")`, lexes and parses each
occurrence with the expression parser (separate research area), and maps lexer positions to the source
by `col + byteOffset` (+1 if quoted). "Expression-only" positions (`Bool.Expression`, `Env.Expression`,
`Matrix.Expression`, and so on) are just `*String` whose whole trimmed value is one `${{ }}`.

### 2.2 Workflow root

`Workflow` (`ast.go:952-974`):

| Field | Type | Source | Notes |
|---|---|---|---|
| `Name` | `*String` | `name` | may be empty |
| `RunName` | `*String` | `run-name` | non-empty |
| `On` | `[]Event` | `on` | `nil` if the key is missing. Empty non-nil slice if it was present but yielded no events (e.g. `on:` null). |
| `Permissions` | `*Permissions` | `permissions` | |
| `Env` | `*Env` | `env` | |
| `Defaults` | `*Defaults` | `defaults` | |
| `Concurrency` | `*Concurrency` | `concurrency` | |
| `Jobs` | `map[string]*Job` | `jobs` | lower-cased job IDs |

The method `FindWorkflowCallEvent()` returns the first `*WorkflowCallEvent` (`977-984`).

### 2.3 Events (`Event` interface: `EventName() string`)

| Type | Fields | EventName |
|---|---|---|
| `WebhookEvent` (`136-163`) | `Hook *String` (the event key, original case); `Types []*String`; `Branches, BranchesIgnore, Tags, TagsIgnore, Paths, PathsIgnore *WebhookEventFilter`; `Workflows []*String`; `Pos` | `Hook.Value` |
| `WebhookEventFilter` (`120-130`) | `Name *String` (the filter key); `Values []*String` | `IsEmpty()` is nil-safe |
| `ScheduledEvent` (`176-186`) | `Schedules []*ScheduleEntry; Pos` | `"schedule"` |
| `ScheduleEntry` (`167-172`) | `Cron, Timezone *String` | entries without `cron` are dropped |
| `WorkflowDispatchEvent` (`227-237`) | `Inputs map[string]*DispatchInput` (lower-cased); `Pos` | `"workflow_dispatch"` |
| `DispatchInput` (`209-223`) | `Name, Description *String; Required *Bool; Default *String; Type WorkflowDispatchEventInputType; Options []*String` | Type enum: `None, String, Number, Boolean, Choice, Environment` (`190-205`) |
| `RepositoryDispatchEvent` (`241-251`) | `Types []*String; Pos` | `"repository_dispatch"` |
| `WorkflowCallEvent` (`318-337`) | `Inputs []*WorkflowCallEventInput` (ordered); `Secrets map[string]*WorkflowCallEventSecret`; `Outputs map[string]*WorkflowCallEventOutput`; `Pos` | `"workflow_call"`. `Secrets == nil` means the key was omitted. |
| `WorkflowCallEventInput` (`270-291`) | `Name, Description, Default *String; Required *Bool; Type WorkflowCallEventInputType; ID string` (lower-cased name) | Type enum: `Invalid, Boolean, Number, String`. `IsRequired()` is `Required != nil && Required.Value`. |
| `WorkflowCallEventSecret` (`295-303`) | `Name, Description *String; Required *Bool` | |
| `WorkflowCallEventOutput` (`307-314`) | `Name, Description, Value *String` | |
| `ImageVersionEvent` (`341-353`) | `Names, Versions []*String; Pos` | `"image_version"` |

The `Pos` of an event is the **key** position when it comes from a mapping, or the scalar or element
position for `on: push` and `on: [push]`.

### 2.4 Job-level and shared nodes

| Type | Fields | Notes |
|---|---|---|
| `Permissions` (`367-374`) | `All *String` (scalar form, e.g. `read-all`); `Scopes map[string]*PermissionScope` (lower-cased); `Pos` (key pos) | Exactly one of `All` or `Scopes`. |
| `PermissionScope` (`357-362`) | `Name, Value *String` | |
| `Defaults` (`389-394`) | `Run *DefaultsRun; Pos` | |
| `DefaultsRun` (`378-385`) | `Shell, WorkingDirectory *String; Pos` (the `run` key) | |
| `Concurrency` (`398-405`) | `Group *String; CancelInProgress *Bool; Pos` | scalar form gives `Group` only |
| `Environment` (`409-418`) | `Name, URL *String; Deployment *Bool; Pos` | scalar form gives `Name` only |
| `Env` (`731-737`) | `Vars map[string]*EnvVar` (lower-cased) **or** `Expression *String` | When a scalar is not an expression, both are nil. |
| `EnvVar` (`723-728`) | `Name, Value *String` | |
| `Output` (`826-831`) | `Name, Value *String` | job outputs |
| `Runner` (`835-844`) | `Labels []*String; LabelsExpr *String; Group *String` | `runs-on` |
| `Credentials` (`761-770`) | `Username, Password, Expression *String; Pos` | |
| `Container` (`774-800`) | `Image *String; Credentials *Credentials; Env *Env; Ports, Volumes []*String; Options, Command, Entrypoint *String; Pos` | `Volumes` is **never populated** (bug, §8). `Command` and `Entrypoint` are services only. |
| `Service` (`804-809`) | `Name *String; Container *Container` | |
| `Services` (`813-822`) | `Value map[string]*Service` (lower-cased) **or** `Expression *String`; `Pos` (value pos) | |
| `Strategy` (`709-720`) | `Matrix *Matrix; FailFast *Bool; MaxParallel *Int; Pos` | |
| `Matrix` (`691-705`) | `Rows map[string]*MatrixRow` (lower-cased); `Include, Exclude *MatrixCombinations`; `Expression *String`; `Pos` | `Pos` is the **key** position for the mapping form but the **value** position for the expression form (`parse.go:937 vs 941`). |
| `MatrixRow` (`641-648`) | `Name *String; Values []RawYAMLValue; Expression *String` | `Name` is **nil** for expression rows (`parse.go:951-953`). |
| `MatrixCombinations` (`670-687`) | `Combinations []*MatrixCombination` **or** `Expression *String` | `ContainsExpression()` |
| `MatrixCombination` (`661-665`) | `Assigns map[string]*MatrixAssign` (lower-cased) **or** `Expression *String` | |
| `MatrixAssign` (`652-657`) | `Key *String; Value RawYAMLValue` | |
| `RawYAMLValue` (`497-506`) | interface `Kind() / Equals() / Pos() / String()` | |
| `RawYAMLObject` (`509-550`) | `Props map[string]RawYAMLValue` (lower-cased); `pos` | `String()` sorts keys |
| `RawYAMLArray` (`553-603`) | `Elems []RawYAMLValue; pos` | |
| `RawYAMLString` (`606-636`) | `Value string; pos` | **All scalars become strings.** Type info (int, bool, null) is lost, as the comment at `607` admits. `Equals` compares text, so `1` and `"1"` are equal, and so are `1.0` and `1`. |
| `RawYAMLValueKind` (`484-493`) | numerically equal to `yaml.Kind` constants | |

### 2.5 Steps

`Step` (`741-757`): `ID, If, Name *String; Exec Exec; Env *Env; ContinueOnError *Bool; TimeoutMinutes *Float; Pos`.
`Pos` is the step mapping's position (its first key). `Exec` is nil when neither `run` nor `uses` is
present.

`Exec` interface (`431-434`) with `Kind() ExecKind` (`ExecKindAction=0`, `ExecKindRun=1`):

- `ExecRun` (`438-452`): `Run, Shell, WorkingDirectory *String; RunPos *Pos` (the `run` **key** position).
- `ExecAction` (`465-481`): `Uses *String; Inputs map[string]*Input` (lower-cased `with`);
  `Entrypoint, Args *String`. For `docker://` uses only, `with.entrypoint` and `with.args` are lifted out
  of `Inputs`.
- `Input` (`456-461`): `Name, Value *String`.

### 2.6 Job

`Job` (`893-948`): `ID *String` (original case); `Name *String`; `Needs []*String`; `RunsOn *Runner`;
`Permissions *Permissions`; `Environment *Environment`; `Concurrency *Concurrency`;
`Outputs map[string]*Output`; `Env *Env`; `Defaults *Defaults`; `If *String`; `Steps []*Step`;
`TimeoutMinutes *Float`; `Strategy *Strategy`; `ContinueOnError *Bool`; `Container *Container`;
`Services *Services`; `WorkflowCall *WorkflowCall`; `Snapshot *Snapshot`; `Pos` (the ID key position).

`WorkflowCall` (`865-877`): `Uses *String; Inputs map[string]*WorkflowCallInput;
Secrets map[string]*WorkflowCallSecret; InheritSecrets bool`. It is set only if `uses` is present **and**
no steps-only key is present (§3.6). `WorkflowCallInput` and `WorkflowCallSecret`: `Name, Value *String`.

`Snapshot` (`882-889`): `ImageName, Version, If *String`.

### 2.7 Things the AST does *not* store

- The raw YAML node, the source span, the end position, the scalar style (except quoted), and comments.
- The positions of most keys. For example `Job.Name` stores the value position only, `EnvVar.Name` stores
  the key, and `Env` has no position at all.
- The order of map-based collections.
- Unknown keys. They are reported and dropped.
- Typed matrix values (§2.4).

---

## 3. Parser behaviour (`parse.go`)

### 3.1 Architecture

- `parser{errors []*Error}` (`parse.go:69-71`). All errors have `Kind: "syntax-check"`
  (`parse.go:73-79`). The parser never stops. `Parse` returns the partial `*Workflow` plus all errors
  (`parse.go:1598-1615`).
- It is a recursive-descent walk over `yaml.Node`, with one function per grammar production
  (`parseJob`, `parseStep`, ...).
- The mapping iterator `parseMapping(where, n, allowEmpty, caseSensitive) iter.Seq[workflowMappingEntry]`
  (`parse.go:316-371`) is the core. It:
  1. treats a `!!null` scalar as empty. It reports `<where> should not be empty. please remove this
     section if it's unnecessary` unless `allowEmpty` is set, and yields nothing;
  2. reports `<where> is <kind> node but mapping node is expected` for non-mapping nodes and yields
     nothing;
  3. for each key, calls `parseString(key, false)`. A non-scalar key gives an error, an empty `""` key
     *is still yielded*, and the caller then usually reports `unexpected key ""` (verified);
  4. rejects `<<`;
  5. checks duplicates (first wins);
  6. yields `{id, key *String, val *yaml.Node}`;
  7. at the end, reports emptiness if everything was skipped and `!allowEmpty`.
- `where` is a `delayedSprintf`, a micro-optimisation that defers `fmt.Sprintf` (`parse.go:49-67`,
  "reduces allocations by 4.94%"). The labels are:
  - `parseSectionMapping(sec)` gives `"sec" section`;
  - `parseMappingAt(text)` gives the text verbatim;
  - for jobs, `"id" job`;
  - for matrix include/exclude elements, `element in "include" section`.

### 3.2 Leaf helpers (exact semantics)

| Helper | Accepts | On failure | Returns |
|---|---|---|---|
| `checkString(n, allowEmpty)` `168-180` | any `ScalarNode` (tag ignored); non-empty unless `allowEmpty` | `expected scalar node for string value but found %s node with %q tag` / `string should not be empty` | bool |
| `parseString(n, allowEmpty)` `204-209` | as above | the error above | **always non-nil**. On failure `&String{"", false, posAt(n)}`. Callers distinguish "missing" (nil) from "invalid" (empty). |
| `parseStringSequence(sec, n, allowEmpty, allowElemEmpty)` `211-224` | `SequenceNode` | `%q section must be sequence node but got %s node with %q tag`; `%q section should not be empty` | nil on kind error |
| `parseStringOrStringSequence` `226-236` | scalar gives a 1-element list; `!!null` with `allowEmpty` gives an empty list; otherwise a sequence | as above | |
| `parseBool` `238-256` | `!!bool`, or `!!str` that is a single expression | `expected bool value but found %s node with %q tag`; `expecting a single ${{...}} expression or boolean literal "true" or "false", but found plain text node` | `nil` on kind error. For a non-expression `!!str` it returns `&Bool{Expression: nil}`, i.e. **value false, no expression**. `Value = (n.Value == "true")`. |
| `parseInt` `258-285` | `!!int` (via `strconv.Atoi`) or a `!!str` expression | `expected scalar node for integer value ...`, `invalid integer value: %q: %s`, missing-expression error | nil on any failure |
| `parseFloat` `287-314` | `!!float`, `!!int` (via `strconv.ParseFloat`, NaN rejected), or a `!!str` expression | `expected scalar node for float value ...`, `invalid float value: %q: %s` | nil on failure. **Panics on NaN** (§8). |
| `parseExpression(n, expecting)` `186-192` | `isExprAssigned(n.Value)` (tag **not** checked) | `expecting a single ${{...}} expression or <expecting>, but found plain text node` | `*String` or nil |
| `mayParseExpression(n)` `194-202` | tag `!!str` and `isExprAssigned` | silent | `*String` or nil |
| `checkNotEmpty`, `checkSequence` `152-166` | | messages above | |
| `unexpectedKey(s, sec, expected)` `136-150` | | 1 expected: `expected "k" key for <sec> but got "x"`; more than 1: `unexpected key "x" for <sec>. expected one of <sorted quoted list>`; 0: `unexpected key "x" for <sec>`. If `sec` contains no space it is wrapped as `"sec" section`. | the list is sorted **in place** (`quotes.go:62-65`) |

`quotes.go` formats lists as Go `strconv.Quote`d, comma-separated strings. Rust must reproduce Go's quoting
rules: `\n`, `\t`, `\u00xx`, non-printables, and UTF-8 kept printable. `quotesAll` concatenates several
lists.

### 3.3 The grammar, production by production

The keys are listed in the parser's `switch`. The `allowEmpty` and case-sensitivity flags are given as
`(empty?, cs?)`.

**Workflow root** (`parse.go:1499-1557`). `parseSectionMapping("workflow", doc.Content[0], false, true)`.

- Keys:
  - `name`: string, may be empty;
  - `on`: events;
  - `permissions`;
  - `env`;
  - `defaults`;
  - `concurrency`;
  - `jobs`;
  - `run-name`: string, non-empty.
- Anything else is an unexpected key, listing the 8 valid keys.
- After the loop: if `On == nil`, report `"on" section is missing in workflow`; if `Jobs == nil`, report
  `"jobs" section is missing in workflow`. Both errors are at the **document** node position.

**`on`** (`690-736`):

- Scalar: `parseEventWithNoConfig`.
  - Empty gives a `string should not be empty` error and no event.
  - `schedule` gives `schedule event must be configured with mapping`.
  - `repository_dispatch`, `workflow_dispatch`, `workflow_call` and `image_version` give bare event structs.
  - Anything else gives `WebhookEvent{Hook}`. **Unknown event names are not checked here**; the
    `events` rule does that.
- Sequence: non-empty, each element as above.
- Mapping: `("on", false, true)`, **case-sensitive** event keys (`PUSH` is accepted by the parser and
  flagged by the `events` rule).
  - `schedule` gives `parseScheduleEvent`:
    - the value must be a non-empty sequence;
    - each element is a mapping `(…, false, true)` with keys `cron` and `timezone`;
    - entries without a valid `cron` are dropped silently, with no extra error beyond the empty-string one.
  - `workflow_dispatch` gives `(…, true, true)`:
    - the only key is `inputs`. Any other key gets the 1-expected form `expected "inputs" key for ...`.
    - `inputs` is `("inputs", true, false)`, case-insensitive with lower-cased ids.
    - Each input is `(…, true, true)` with keys:
      - `description` (string, may be empty);
      - `required` (bool);
      - `default` (string, may be empty);
      - `type`, one of `string number boolean choice environment`. Otherwise:
        `input type of workflow_dispatch event must be one of "string", "number", "boolean", "choice", "environment" but got %q`.
      - `options`, a non-empty sequence of non-empty strings.
    - The unknown-key message lists only `default, description, required` (bug: it omits `type` and
      `options`).
  - `repository_dispatch` gives `(…, true, true)`. The only key is `types`, a string or a non-empty
    sequence.
  - `workflow_call` gives `(…, true, true)` with keys:
    - `inputs`: `(…, true, false)`, ordered slice. Each input is `(…, true, true)` with keys
      `description`, `required`, `default` and `type`. `type` must be one of `boolean number string`,
      otherwise `invalid value %q for input type of workflow_call event. it must be one of "boolean", "number", or "string"`.
      A missing `type` is reported at the input name as `"type" is missing at %q input of workflow_call event`.
    - `secrets`: `(…, true, false)`; each secret takes `description` and `required`.
    - `outputs`: `(…, true, false)`; each output takes `description` and `value` (non-empty). A missing
      value gives `"value" is missing at %q output of workflow_call event`.
  - `image_version` gives `(…, true, true)` with keys `names` and `versions`, both non-empty sequences.
  - Any other key gives `parseWebhookEvent` with `(name, true, true)`:
    - keys `types`, `branches`, `branches-ignore`, `tags`, `tags-ignore`, `paths`, `paths-ignore` and
      `workflows`;
    - each value is a string or a **non-empty** sequence (`allowEmpty=false`, despite the stale comment at
      `parse.go:496-503` claiming that the filters may be empty);
    - `null` gives a `string should not be empty` error;
    - which filters are legal for which event is **not** checked here; the `events` rule does that.

**`permissions`** (`739-757`):

- Scalar: `All = parseString(n, false)`. So `permissions:` with null gives `string should not be empty`
  (testdata `issue170`).
- Otherwise a mapping `("permissions", true, false)`. Scopes are lower-cased. Values are non-empty
  strings and are **not validated here**; the `permissions` rule validates them.

**`env`** (`760-776`):

- Scalar: must be a single expression (`expecting a single ${{...}} expression or mapping value for "env" section`).
  Note that a null `env:` lands here and gets *that* message, not "should not be empty".
- Otherwise a mapping `("env", false, false)` with values as strings that may be empty.

**`defaults`** (`779-806`):

- `("defaults", false, true)`. The only key is `run`, which is `(false, true)` with keys `shell` and
  `working-directory` (both non-empty).
- If `run` is missing: `"defaults" section should have "run" section`.

**`concurrency`** (`809-831`):

- Scalar: `Group`.
- Mapping `(false, true)` with keys `group` and `cancel-in-progress` (bool).
- A missing group gives `group name is missing in "concurrency" section` at the key position.
- The new `queue:` key (upstream issue #657) is rejected.

**`environment`** (`834-858`):

- Scalar: `Name`.
- Mapping with keys `name`, `url` and `deployment` (bool).
- A missing name gives `name is missing in "environment" section`.

**`outputs`** (job, `861-871`): `("outputs", false, false)`, then `checkNotEmpty`. A fully-duplicate or
merged-only mapping can therefore report twice.

**`strategy`** (`988-1005`): `(false, true)` with keys:

- `matrix` (§3.5);
- `fail-fast` (bool);
- `max-parallel` (int; a literal value `<= 0` gives `value at "max-parallel" must be greater than zero: %v`).

**`container` / `services.<id>`** (`1035-1091`):

- Scalar: `Image`.
- Mapping `(sec, false, true)` with keys `image`, `credentials`, `env`, `ports`, `volumes` and `options`.
  `command` and `entrypoint` are accepted only for services.
- A missing image gives `"image" is missing in %q section` at the key position.
- `credentials` (`1007-1032`):
  - a `!!str` expression gives `Credentials{Expression}`;
  - otherwise a mapping with keys `username` and `password`;
  - if either is missing: `both "username" and "password" must be specified in "credentials" section`,
    and the function **returns nil**.
- `services` (`1093-1109`): a `!!str` expression, or a mapping `("services", false, false)` (lower-cased).

**`steps`** (`1202-1267`):

- A non-empty sequence. Each element is parsed with
  `parseMappingAt("element of \"steps\" section", n, false, true)`, and the entries are collected into a
  slice first.
- A first pass handles the common keys:
  - `id`, `if`: non-empty strings;
  - `name`: string, may be empty;
  - `env`;
  - `continue-on-error`: bool;
  - `timeout-minutes`: float `> 0`.

  It also determines the kind:
  - `uses` with prefix `docker://` gives docker, otherwise action. The prefix is read from `e.val.Value`
    without checking the node kind.
  - `run` gives the run kind.
  - **The last of `uses` or `run` in source order wins.** The other key then gets an unexpected-key error
    (verified), so the message depends on key order.
- A second pass then runs:
  - `parseStepExecAction`: keys `uses`, `with` (`("with", false, false)`, lower-cased; docker lifts out
    `entrypoint` and `args`) plus the common keys.
  - `parseStepExecRun`: keys `run`, `shell`, `working-directory` plus the common keys.
- Neither kind: `step must run script with "run" section or run action with "uses" section` at the step
  node.
- Step keys are **case-sensitive** (`RUN` is unexpected).

**`runs-on`** (`1270-1297`):

- A `!!str` single expression gives `LabelsExpr`.
- A scalar or sequence gives `Labels`. Elements can individually contain expressions; they are kept as
  label strings.
- A mapping `(false, true)` with keys `labels` (an expression via `mayParseExpression`, or a string or
  non-empty sequence) and `group` (string).

**`snapshot`** (`1299-1325`):

- Scalar: `ImageName`.
- Mapping with keys `image-name`, `version` and `if`. The mapping is parsed with
  `parseSectionMapping("on", …)`, a **copy-paste bug**: emptiness errors would say `"on" section`.
- A missing image name gives `"snapshot" section must have "image-name" configuration`.
- Other kinds give `"snapshot" section value must be string or mapping but found %s node`.

**`jobs`** (`1490-1496`): `("jobs", false, false)`. Job IDs are lower-cased for map keys. Duplicate job IDs
are detected case-insensitively.

**`job`** (`1328-1487`): `(sprintf("%q job", id), false, true)`.

| Key | Parsed as |
|---|---|
| `name` | string, may be empty |
| `needs` | a scalar gives a 1-element list; otherwise a non-empty sequence |
| `runs-on` | `parseRunsOn` |
| `permissions` | `parsePermissions` |
| `environment` | `parseEnvironment` |
| `concurrency` | `parseConcurrency` |
| `outputs` | `parseOutputs` |
| `env` | `parseEnv` |
| `defaults` | `parseDefaults` |
| `if` | non-empty string |
| `steps` | `parseSteps` |
| `timeout-minutes` | float `> 0` |
| `strategy` | `parseStrategy` |
| `continue-on-error` | bool |
| `container` | `parseContainer` |
| `services` | `parseServices` |
| `uses` | non-empty string |
| `with` | `("with", false, false)`; values are strings that may be empty |
| `secrets` | the scalar `inherit` gives `InheritSecrets`. Any other scalar gives `expected mapping node for secrets or "inherit" string node but found %q node`. A mapping is `(false, false)`. |
| `snapshot` | `parseSnapshot` |

### 3.4 Case-sensitivity table (the `caseSensitive` argument)

| Section | Keys case-sensitive? | Map keyed by lower-case? |
|---|---|---|
| workflow root, job body, step, strategy, defaults, defaults.run, concurrency, environment, container/service body, credentials, runs-on mapping, snapshot, schedule element, every event body, input/secret/output settings | **yes** (`NAME` is an unexpected key) | n/a |
| `on` mapping (event names) | yes (raw `Hook`) | no |
| `jobs` | no (dup detection is case-insensitive) | yes |
| `workflow_dispatch.inputs`, `workflow_call.inputs/secrets/outputs` | no | yes (`ID` lower-cased) |
| `permissions` scopes | no | yes |
| `env` (all levels) | no | yes |
| job `outputs` | no | yes |
| `matrix` keys, matrix row object values, include/exclude elements | no | yes |
| `services` | no | yes |
| step `with`, job `with`, job `secrets` | no | yes |

Lower-casing uses Go `strings.ToLower`, which is full-Unicode simple case mapping. For exact parity Rust
should use per-`char` `to_lowercase` with the special cases Go makes. Go maps `İ` (U+0130) to `i̇`
(two runes), like Rust's `str::to_lowercase`, so `str::to_lowercase` is *mostly* compatible. Recommendation:
use ASCII-only folding plus a Unicode fallback, and add a test.

Inconsistency: `parseReusableWorkflowMetadata` matches the event name `workflow_call`
**case-insensitively** (`reusable_workflow.go:343, 354, 360`), while the main parser keeps event names
case-sensitive.

### 3.5 Expressions where a mapping or sequence is expected

| Location | Scalar form handling |
|---|---|
| `env` (workflow/job/step/container) | must be a single expression (any tag), giving `Env.Expression` |
| `strategy.matrix` | single expression, giving `Matrix{Expression, Pos: value pos}` |
| matrix row (`os: ${{ fromJSON(x) }}`) | `MatrixRow{Expression}` (Name nil) |
| `matrix.include` / `matrix.exclude` | whole value, giving `MatrixCombinations.Expression`; each element may be a scalar expression, giving `MatrixCombination.Expression` (`expecting a single ${{...}} expression or mapping of matrix combination`) |
| `services` | `!!str` expression, giving `Services.Expression` |
| `container.credentials` | `!!str` expression, giving `Credentials.Expression` |
| `runs-on`, `runs-on.labels` | `!!str` expression, giving `LabelsExpr` |
| bool/int/float fields | `!!str` must be a single expression |
| `container`, `concurrency`, `environment`, `permissions`, `needs`, `snapshot` | the scalar is taken as the short form (image, group, name, all, the single need, image-name); expressions are simply part of that string |
| `steps`, `with`, job `with`/`secrets`, `outputs`, `strategy`, `defaults`, `on`, `jobs` | **no** expression form; a scalar gives a kind error |

`matrix` values are parsed into `RawYAMLValue` (`873-897`): scalars become strings, sequences become
arrays, mappings become `("matrix row value", true, false)` objects, and an `AliasNode` (only
recursive aliases remain) gives `unexpected alias node on parsing value in matrix row`.
Matrix row values must be a non-empty sequence (`"matrix values" section should not be empty`).

### 3.6 Reusable-workflow-call job validation (`parse.go:1457-1484`)

The steps-only keys are `runs-on`, `environment`, `outputs`, `env`, `defaults`, `steps`,
`timeout-minutes`, `continue-on-error` and `container`. **`services` is missing** (upstream issue #748).
The call-only keys are `uses`, `with` and `secrets`.

- If `uses` is present and a steps-only key was seen, report
  `when a reusable workflow is called with "uses", %q is not available. only following keys are allowed: "name", "uses", "with", "secrets", "needs", "if", and "permissions" in job %q`.
  Only the **last** such key is reported. `WorkflowCall` is then **not set**, so rules see a job with
  neither steps nor a call.
- If `uses` is absent:
  - `Steps == nil` gives `"steps" section is missing in job %q`. This also fires when `steps` was present
    but invalid, which produces two errors.
  - `RunsOn == nil` gives `"runs-on" section is missing in job %q`.
  - Each call-only key gives `%q is only available for a reusable workflow call with "uses" but "uses" is not found in job %q`
    (last one only).
- `strategy`, `concurrency`, `services` and `snapshot` are silently allowed with `uses`.

### 3.7 Error recovery strategy

- The parser never aborts after YAML decoding succeeds. Each production reports its error and returns
  either nil or a zero-ish node, and the parent continues.
- Duplicate keys and `<<` entries are skipped (their values are not parsed). Unknown keys are reported
  and their values are not parsed.
- Error order in `p.errors` follows the walk, but the linter sorts all errors by
  `(file, line, column, message)` and deduplicates them (`error.go:161-179`, `linter.go:638-639`), so the
  emission order does not matter.
- Partial ASTs are fed to all rules. Rules must be nil-tolerant everywhere.

---

## 4. Pass / visitor architecture (`pass.go`, `rule.go`, `linter.go`)

- `Pass` interface (`pass.go:10-21`): `VisitStep`, `VisitJobPre`, `VisitJobPost`, `VisitWorkflowPre` and
  `VisitWorkflowPost`, each returning `error`. The error is "internal error, cannot continue".
- `Rule` embeds `RuleBase` (`rule.go:10-80`), which provides no-op visits, `Error`/`Errorf`
  (rule-name-tagged `*Error`), `Errs()`, `Name()`, `Description()`, `EnableDebug`, and `SetConfig` and
  `Config` further down.
- `Visitor.Visit` (`pass.go:50-89`) does a single depth-first traversal and fans each callback out to all
  passes **in registration order**:

  ```
  for p in passes: p.VisitWorkflowPre(w)
  for j in w.Jobs (Go map => RANDOM order):
      for p in passes: p.VisitJobPre(j)
      for s in j.Steps (ordered): for p in passes: p.VisitStep(s)
      for p in passes: p.VisitJobPost(j)
  for p in passes: p.VisitWorkflowPost(w)
  ```

  The first non-nil error aborts the whole visit. Debug mode prints elapsed times per phase.
- Only three node granularities are visited. Rules dig into events, matrices and so on by themselves.
- The rule set is built per file in `linter.go:552-568`, in this order:
  `matrix, credentials, shell-name, runner-label, events, job-needs, action, env-var, id, glob,
  permissions, workflow-call, expression, deprecated-commands, if-cond`, then optional `shellcheck` and
  `pyflakes`. After that comes `OnRulesCreated` (a hook that can add or remove rules), then `SetConfig`.
  Rules are stateful (they accumulate per-workflow state across callbacks), so a fresh instance is
  created per file.
- **Parallelism**:
  - Across files, `LintFiles` runs one goroutine per file via `errgroup` (`linter.go:347-386`). File reads
    are bounded by a `semaphore` of `NumCPU` (issue #3, too many open files).
  - Parsing and visiting inside a file is sequential.
  - External processes (shellcheck, pyflakes) go through `concurrentProcess` with `NumCPU` workers.
    `proc.wait()` is called after `eg.Wait()` (`linter.go:388-395`).
  - Project lookup (`Projects.At`) and cache factories are not thread-safe, so they are resolved serially
    before spawning goroutines (`linter.go:353-362`).
- **Nondeterminism from map iteration.** Because jobs are visited in random order, any "report only the
  first time" logic in a rule depends on the visit order. For example, metadata-cache errors are returned
  only on the first lookup (§5). If two jobs in one file reference the same broken local
  action or reusable workflow, which job gets the error is random. Across files the same race exists
  between goroutines (testdata `projects/broken_reusable_workflow/README.md` documents this). Rust should
  iterate in source order (`IndexMap`) and make cache diagnostics deterministic.

---

## 5. Action metadata, popular actions, reusable-workflow metadata, caches

### 5.1 `action.yml` parsing (`action_metadata.go`)

`ActionMetadata` (`157-179`) has these fields:

- `Name` (yaml `name`);
- `Description`;
- `Inputs ActionMetadataInputs`;
- `Outputs ActionMetadataOutputs`;
- `Runs ActionMetadataRuns`;
- `Branding{Icon, Color}`;
- `SkipInputs` and `SkipOutputs` (only set by the popular-actions dataset);
- unexported `dir` and `file`, used by `Dir()` and `Path()`.

`ActionMetadataRuns` (`119-146`) has `using`, `main`, `pre`, `pre-if`, `post`, `post-if`,
`steps []any`, `image`, `pre-entrypoint`, `entrypoint`, `post-entrypoint`, `args []any` and
`env map[string]any`. Top-level unknown keys such as `author` are ignored. Duplicate exact keys are an
error (go-yaml `uniqueKeys`).

Inputs: `ActionMetadataInputs.UnmarshalYAML` (`35-84`).

- The value must be a mapping. Otherwise:
  `yaml: inputs must be mapping node but scalar node was found at line:%d, col:%d` (`expectedMapping`,
  `reusable_workflow.go:14-22`).
- Each value is decoded into `{required bool, default *string, deprecationMessage string}`. A non-mapping
  value gives a go-yaml TypeError `cannot unmarshal !!str ... into actionlint.actionInputMetadata`.
  `required` accepts YAML 1.1 bools.
- `id = ToLower(name)`. A duplicate gives `input %q is duplicated` (a fatal error).
- Allowed keys are `description`, `required`, `default` and `deprecationMessage`. Any other key gives
  `unexpected key %q for definition of input %q`. This error is *deferred*: the map is still assigned,
  then the error is returned. That matters only for the generator. For local actions any error rejects
  the action.
  - The key scan iterates `v.Content`, so for an **aliased** input (`bar: *input`) the scan sees nothing.
    The `deprecationMessage` detection and the unknown-key detection are skipped (quirk).
- `Required = required && default == nil`. `default:` with a null value counts as no default.
- `Deprecated` is true if the key `deprecationMessage` is present (even null or empty). The message is
  `TrimSpace`d.

Outputs: `ActionMetadataOutputs.UnmarshalYAML` (`98-115`). The value must be a mapping. Keys are
lower-cased, duplicates are an error, and the values are ignored completely.

Error-message shape for local actions (`FindMetadata`, `266-282`): a single `TypeError` is unwrapped to
`line N: <msg>`. Several errors are joined with their newlines removed:
`could not parse action metadata in %q: <msg>` (see `testdata/projects/broken_local_action.out`).

### 5.2 Local action cache

`LocalActionsCache` (`195-300`) holds `{mu RWMutex, proj *Project, cache map[spec]*ActionMetadata, dbg}`.
`FindMetadata(spec)` returns `(meta, cached bool, err)`:

1. If `proj == nil` or `spec` does not start with `./`, return `(nil, false, nil)`.
2. On a cache hit (including a cached **nil**), return `(m, true, nil)`.
3. Compute `dir = root/FromSlash(spec)`, then try **`action.yaml` first, then `action.yml`**
   (`readLocalActionMetadataFile`, `291-300`). The GitHub runner checks `action.yml` first; this is worth
   verifying, because the order differs when both files exist.
4. Not found: cache nil, return `(nil, false, nil)`. No error, on purpose: the action may come from a
   submodule or be checked out at run time (#25, #40).
5. Parse error: cache nil, and return the error **only this once**.
6. Success: set `file` and `dir`, cache it, return `(meta, false, nil)`.

The `cached` flag tells `RuleAction` whether to validate the metadata itself (`runs.using` and so on),
which happens only on the first lookup (`rule_action.go:577-597`). Metadata-level errors are therefore
reported once per project, at whichever step happened to load the action first. That step depends on
scheduling across files and on job map order within a file.

The cache key is the raw `spec` string, with no normalisation (`./a` and `./a/` are separate entries).
Locking is read-lock, then read the file, then write-lock. This **check-then-act is not single-flight**:
concurrent first lookups may parse twice and report the error twice. The test only asserts "at least
one" (`action_metadata_test.go:400-433`).

`LocalActionsCacheFactory` (`305-329`) keeps one cache per project root. It is not thread-safe and is used
serially. A null cache is returned for a nil project. `LintFile` and `Lint` create fresh caches per call
(`linter.go:455-457`, `498-500`).

### 5.3 Popular actions dataset (`popular_actions.go`, generated)

- `var PopularActions = map[string]*ActionMetadata{...}` (`popular_actions.go:7-5064`) has **218 specs**.
  The key is the exact string `owner/repo[/path]@ref`. Each value has `Name`, `Inputs` (with
  `{Name, Required, Deprecated, DeprecationMessage}`), `Outputs` (`{Name}`) and optional `SkipInputs` and
  `SkipOutputs`.
- `var OutdatedPopularActionSpecs = map[string]struct{}{...}` (`5068-5225`) has **156 specs** whose runner
  is obsolete (node < 20, or the hard-coded list `actions/{labeler,checkout,upload-artifact,download-artifact}@v1`).
  Using one gives `the runner of %q action is too old to run on GitHub Actions` (`rule_action.go:375-380`).
- File size is about 275 KB of Go source.
- Generated by `scripts/generate-popular-actions/main.go` (`//go:generate`, `action_metadata.go:14`)
  from `scripts/generate-popular-actions/popular_actions.json`. That file has **105 registries**, each
  `{slug, tags[], next, path, skip_inputs, skip_outputs, file_ext}`.
  - The generator fetches `https://raw.githubusercontent.com/{slug}/{tag}{path}/action.{yml|yaml}` with 5
    workers, decodes with the same `ActionMetadata` unmarshaler, and applies hand-written patches
    (`dorny/paths-filter@v3` adds an input; `actions/download-artifact@v3-node20` adds an output).
  - It can emit Go or JSONL. `-d` detects new releases via the `next` tags.
  - Upstream issue #759 reports that its JSONL reader drops a final unterminated record.
- Lookup is exact and **case-sensitive**, with no ref normalisation. **SHA-pinned actions**
  (`actions/checkout@<40-hex>`), which security policies commonly require, **never match**, so their
  inputs and outputs go unchecked. Owner and repo case variants also miss (GitHub owner and repo names
  are case-insensitive).
- Related tables: `actions/github-script@*` accepts any outputs (`rule_expression.go:347-351`).
  `AllWebhookTypes` (`all_webhooks.go`, generated by `scripts/generate-webhook-events` from the GitHub
  docs) maps 32 event names to activity types. `nil` means arbitrary types (`repository_dispatch`) and
  `{}` means no types.

### 5.4 Reusable workflow metadata (`reusable_workflow.go`)

- `ReusableWorkflowMetadata{Inputs, Outputs, Secrets}` (`159-163`), all maps lower-cased:
  - `Input{Name, Required (= required && default == nil), Type ExprType}`. Type is
    `boolean`→`BoolType`, `number`→`NumberType`, `string`→`StringType`, anything else or missing
    →`AnyType`. A null input value gives `AnyType`.
  - `Secret{Name, Required}`.
  - `Output{Name}`.
  - Inputs, secrets and outputs must be mappings: `yaml: on.workflow_call.inputs must be mapping node ...`.
- `parseReusableWorkflowMetadata(src)` (`323-368`):
  - It struct-decodes only the `on` key. The struct field tag is exact `on`, so a top-level `On:` would
    not be found.
  - `Line == 0 && Column == 0` means `"on:" is not found`.
  - Mapping: the first key equal to `workflow_call`, case-insensitively, is decoded with
    `Node.Decode(&ReusableWorkflowMetadata)`.
  - Scalar or sequence containing `workflow_call` gives empty metadata.
  - Otherwise: `"workflow_call" event trigger is not found in "on:" at line:%d, column:%d`.
  - A YAML syntax error returns go-yaml's error text (the comment says "Unreachable", but it is reachable
    for broken files).
- `LocalReusableWorkflowCache` (`169-234`): same design as the action cache.
  - `FindMetadata(spec)` skips a nil project, a spec without `./`, or a spec where
    `ContainsExpression(spec)` holds.
  - It reads `root/FromSlash(spec)`. A read error caches nil and returns
    `could not read reusable workflow file for %q: %w`. A parse error caches nil and returns
    `error while parsing reusable workflow %q: <msg with newlines replaced>`.
  - The path is **not** restricted to `.github/workflows`. In the testdata the callees live in
    `./reusable/` and `./workflows/`.
  - Specs with `@ref` (`./x.yml@main`) are not stripped.
- `WriteWorkflowCallEvent(wpath, event)` (`264-321`): `RuleWorkflowCall.VisitWorkflowPre`
  (`rule_workflow_call.go:31-41`) **pre-populates** the cache from the AST of every linted workflow that
  has `workflow_call`. The workflow path is converted to a spec by `convWorkflowPathToSpec` (`236-256`):
  make it absolute against `cwd`, require the `root` prefix (prefix bug as in §6), make it relative, use
  `/` separators, and prefix with `./`.

  It does not overwrite an existing entry. Its semantics differ slightly from the file parser:
  `Required` uses `Required.Value`, so an expression is false, and the AST-based ID is used. The race
  between "AST first" and "file first" changes which errors appear (README in
  `testdata/projects/broken_reusable_workflow`).
- Factory: one cache per project root, plus a null cache.

---

## 6. Project discovery and configuration

### 6.1 Project discovery (`project.go`)

- `findProject(path)` (`24-39`) walks upward from `filepath.Abs(path)`. The first directory `d` with
  **both** `d/.github/workflows` as a directory **and** `d/.git` existing (dir or file, so worktrees and
  submodules work) becomes the project. If `path` is a file, the first iteration tests `file/.github`
  (fails) and moves to the parent. If nothing is found the result is nil, which means no project: no
  config, null caches, local actions and reusable workflows unchecked.
- `NewProject(root)` loads the repository config immediately (§6.2). A config parse error makes the whole
  lookup fail, and linting aborts with that error (`project_test.go:136-151`).
- `Projects.At(path)` (`90-106`): a linear search over known projects with `Knows(path)`, which is
  `strings.HasPrefix(abs(path), root)`. This is a **prefix bug**: root `/a/b` "knows" `/a/bc/x.yml`
  (upstream issue #750). Otherwise it calls `findProject` and caches the result. It is not thread-safe.
- `WorkflowsDir()` is `root/.github/workflows`. `LintRepository` lints that directory.
  - `LintDir` walks **recursively**, collecting `*.yml` and `*.yaml`. GitHub only reads the top level of
    `.github/workflows`, so nested files are linted unnecessarily.
  - It errors with `no YAML file was found` if there are none.
  - Files are sorted for deterministic order (`linter.go:285-313`).
- There is no support for `.forgejo/workflows`, `.gitea/workflows` or composite-action-only repos (issue
  #400), and no global config (#675) or config layering (#747).

### 6.2 Config file (`config.go`)

**Location.** `-config-file <path>` (`linter.go:142-149`) overrides everything, for every project.
Otherwise `<root>/.github/actionlint.yaml` is tried first and then `<root>/.github/actionlint.yml`
(`loadRepoConfig`, `120-134`). A not-found error skips to the next candidate. Any other error, even
permission denied, is reported as `could not parse config file %q: could not parse config file %q: ...`.
The prefix is doubled because `ReadConfigFile` (`106-116`) already wraps it.

**Schema.** Every option:

| Key | Type | Semantics | Consumer |
|---|---|---|---|
| `self-hosted-runner.labels` | `[]string` | Extra runner labels. Each is a Go `path.Match` glob (`*`, `?`, `[...]`, `\`). An invalid pattern is reported per label use: `label pattern %q is an invalid glob. kindly check list of labels in actionlint.yaml config file: %v`. `null` and `[]` are equivalent. | `rule_runner_label.go:195-215, 327-333` |
| `config-variables` | `[]string` or null | `null` (the default) **disables** the `vars.*` check. `[]` means no variables are allowed. Comparison is case-insensitive (testdata `projects/config_variables`). | `rule_expression.go:776-779` → `NewExprSemanticsChecker` |
| `paths` | `map[glob]PathConfig` | Keys are doublestar globs (`**`, `{a,b}`, ...), validated at load with `doublestar.ValidatePattern`. An invalid key gives `invalid glob pattern %q in "paths"`. **All** matching entries apply (a union). | `config.go:74-87`, `linter.go:632` |
| `paths.<glob>.ignore` | `[]regex` | Go RE2 regexes, unanchored `MatchString` against the error **message** only. A non-sequence gives `yaml: "ignore" must be a sequence node at line:%d,col:%d`. A bad regex gives `invalid regular expression %q in "ignore" at line%d,col:%d: %w`; note the typo `line%d` and that the line is the sequence's, not the element's. **Bug**: elements are not checked to be scalars, so `ignore: [{}]` compiles `""` and silences everything (upstream issue #749). | `config.go:30-44` |

Unknown top-level keys are silently ignored. Duplicate exact keys are an error. The CLI
`-ignore <regex>` adds global patterns (`linter.go:151-158`). `-init-config` writes the template in
`config.go:136-160` to `.github/actionlint.yaml`.

**Path matching bug.** `PathConfigs(path)` is documented as "relative to the root of the project", but
`Linter.check` passes the path **relative to the current working directory** (`linter.go:368-373`,
`436-440`, `632`). Running from a subdirectory therefore breaks `paths` matching. This is likely the
cause of upstream issue #481, "Unable to ignore errors by paths config". `<stdin>` paths never match.

---

## 7. Glob validation (`glob.go`)

Glob validation is used by `rule_glob.go`:

- the ref glob for `branches`, `branches-ignore`, `tags`, `tags-ignore`, `image_version.versions` and
  `jobs.*.snapshot.version`;
- the path glob for `paths` and `paths-ignore`.

Empty values are skipped there, because the parser already reported them. It is a **validator only**:
it never matches. Matching semantics (for example "is this branch filtered") are not implemented
anywhere.

`InvalidGlobPattern{Message, Column}` (`22-38`). The column is 1-based in runes. `0` means "no position"
(empty pattern, or an error after a newline).

Algorithm (`globValidator`, `40-231`) over Go `text/scanner` in raw `Next()` mode, with rune-based columns
and an error callback for invalid UTF-8:

1. `validate(pat)`:
   - An empty pattern gives `glob pattern cannot be empty`.
   - First character `/` in ref mode: consume it and report `ref name must not start with /`.
   - First character `!`: consume it. If nothing follows, report
     `at least one character must follow !` and stop. Otherwise set `prec = false`, so a `+` or `?`
     right after `!` is an error.
   - Then loop with `validateNext`.
2. `validateNext()`:
   - `\` followed by `[`, `?` or `*`: consume. In ref mode, report an invalid ref character, **but with
     `Peek()`, the character *after* the escaped one, as the reported char**. This is a bug, verified:
     `\[` reports the character `'�'` (EOF rune) and `\[x` reports `'x'`.
   - `\` followed by `+`, `\` or `!`: consume.
   - `\` followed by any other character: in ref mode, report
     `only special characters [, ?, +, *, \\, ! can be escaped with \\` and consume. In path mode `\` is
     literal.
   - `?` or `+` when `!prec`: report
     `invalid glob pattern. unexpected character '?' while checking special character ? (zero or one). the preceding character must not be special character`.
   - `*`: no error. It sets `prec = false`.
   - `[`:
     - `[]` gives `character match must not be empty`.
     - Otherwise loop until `]`. EOF gives `missing ]` and returns.
     - A range `a-b` with `a > b` gives `start of range %q (%d) is larger than end of range %q (%d)`.
     - `a-]` gives `end of range is missing`.
     - A class with exactly one char, such as `[x]`, gives
       `character match with single character is useless. simply use x instead of [x]`.
     - No `[!...]` negation handling exists: `!` is a literal class char.
   - `\r` (optionally followed by `\n`) or `\n`: `newline cannot be contained`.
   - Space, tab, `~`, `^` or `:` in ref mode: `ref name cannot contain spaces, ~, ^, :, [, ?, *`.
   - At EOF in ref mode, a last character of `/` or `.` gives `ref name must not end with / and .`.
3. `ValidatePathGlob(pat)` (`243-262`) pre-checks:
   - leading or trailing whitespace: `leading and trailing spaces are not allowed in glob path`;
   - after stripping a leading `!`, a value of `.` or `..` or a prefix of `./` or `../`:
     `'.' and '..' are not allowed in glob path` (#521).

   If either check fails, only those errors are returned. Otherwise it runs the core validator in path
   mode.
4. Error message wording: `unexpected` (`57-69`) and `invalidRefChar` (`71-79`; it uses `'%c'` for
   printable characters and `%q` otherwise).
5. Column: `scan.Pos().Column - 1`, or 0 if the line is greater than 1. In `rule_glob.go:74-84` the error
   position is `pos + (quoted ? 1 : 0) + (Column - 1)`.

The test suites `glob_test.go:18-472` give the full acceptance and error corpus. Port them verbatim.

---

## 8. Surprising, fragile or buggy behaviour (candidates to fix, or to emulate behind a compat flag)

Every bug below was verified with the probe unless noted.

1. **Panic on `NaN`**. `timeout-minutes: !!float NaN` (and any explicitly tagged NaN that
   `strconv.ParseFloat` accepts) gives `math.IsNaN && err == nil`, then `err.Error()`, a nil dereference
   that crashes the linter (`parse.go:305-306`). Verified stack trace. Upstream issue #753.
2. **`container.volumes` is stored in `Ports`** (`parse.go:1065-1066`). `Volumes` is never set, and if
   both keys are present one overwrites the other. Upstream issue #747.
3. **`parseBool` with `True` or `TRUE`**. Tag `!!bool`, but `Value == "true"` is case-sensitive, so the
   result is `false` (`parse.go:252-255`). `continue-on-error: True` becomes false, and `required: TRUE`
   on workflow_call inputs is treated as not required.
4. A non-expression `!!str` bool yields `&Bool{}`, which is indistinguishable from literal false without
   checking `Expression` (`parse.go:244-250`).
5. The `workflow_dispatch` input unknown-key message omits `type` and `options` (`parse.go:444`).
6. The `snapshot` mapping uses the section name `"on"` (`parse.go:1305`).
7. `services` is not a steps-only key, so reusable-call jobs accept `services` (#748). Only the last
   offending key is reported. Invalid reusable call jobs drop `WorkflowCall` entirely.
8. The step kind is decided by the *last* `uses`/`run` key. The `uses` value is read without a kind check.
9. Missing and invalid are conflated: an invalid `steps` gives two errors (kind and "missing").
   `parseString` returns non-nil empty strings on failure, so callers' nil checks pass.
10. A duplicate key skips the whole entry (no nested errors). A non-scalar key yields an `""` key.
11. Matrix scalars lose their type (`RawYAMLString`), so `1` equals `"1"` and `true` equals `"true"`.
    Matrix `Pos` is inconsistent (key vs value). `MatrixRow.Name` is nil for expression rows.
12. Expression positions inside block scalars, escaped quoted strings and non-ASCII text are wrong (§1.3).
    The error caret is misplaced for non-ASCII lines (byte-vs-rune mix in `error.go`).
13. YAML syntax errors: only one is reported, with a 0-based column and odd line selection. An unknown
    anchor is reported at `0:0` with a doubled `yaml:` prefix. Multi-document files are silently
    truncated to the first document.
14. Leaked Go error strings in messages (`strconv.Atoi: parsing ...`). Integers in YAML forms such as
    `1_000`, `0x10` and `0o17` are rejected after being typed `!!int`.
15. `ContainsExpression` is quirky for `}}` before `${{`. `isExprAssigned` counts only `${{`.
16. Aliases make the AST a DAG. Errors are attributed to anchors and deduplicated textually. Rules that
    count things, such as duplicate IDs, may see the same node twice.
17. Nondeterminism: random job order, and "report once" caches racing across goroutines and jobs (§4,
    §5). Errors therefore move between files and jobs from run to run.
18. Caches are not single-flight. The cache key is the raw spec string. `Projects.Knows` and
    `convWorkflowPathToSpec` use string-prefix path checks (#750).
19. Config: `paths` matching is relative to cwd instead of the repo root (§6.2). `ignore: [{}]` silences
    everything (#749). The error prefix is doubled. Unknown keys are silently accepted, so typos such as
    `self-hosted-runners` are ignored.
20. Local action lookup prefers `action.yaml` over `action.yml`. Aliased action inputs skip the
    deprecation and unknown-key scan. `<<` in inputs becomes an input.
21. Reusable workflows: event-name case-insensitivity differs from the main parser. `@ref` local specs and
    non-`.github/workflows` paths are accepted. The struct-tag `on` is case-sensitive.
22. Popular actions: exact, case-sensitive spec match. SHA pins and minor tags never hit (a big coverage
    gap). The dataset is compiled in and the generator needs network access.
23. The stale comment says webhook filters may be empty while the code forbids it (`parse.go:496-503`).
24. Glob: the ref-escape error prints the wrong character (§7). `[!...]` is not understood.
25. Schema drift: GitHub has added syntax that the hard-coded key lists reject. Examples:
    `concurrency.queue` (#657), parallel steps (`background`, `wait`, `wait-all`, `cancel`, `parallel`,
    #693), `$/` self-repo `uses:` (#704, #760), and `snapshot` was added recently. A data-driven schema
    (keys table plus per-key parser) would make these one-line additions. Upstream is effectively
    unmaintained (#719, #780), so the Rust port will have to track GitHub itself.

---

## 9. Rust design recommendations for this layer

### 9.1 YAML backend: evaluation (with measurements)

Requirements derived from §1:

- (a) start position of each node, **including properties** (anchor or tag start), as line plus rune
  column, ideally with byte offsets and end positions;
- (b) anchor **names** and alias names;
- (c) resolved or explicit tags, with the ability to run our own resolver identical to go-yaml's;
- (d) scalar style, including literal and folded, plus the raw source span to map value offsets;
- (e) the libyaml-identical error messages and marks, for parity on `could not parse as YAML: ...`;
- (f) first-document-only, plus the option to detect extra documents;
- (g) no duplicate-key or merge processing performed by the library;
- (h) comments, optionally, for future suppression comments;
- (i) speed and safety.

Benchmark: a 1.3 MB workflow (3000 jobs, 30k steps, 144k events), release build, Apple Silicon.

| Option | Version (crates.io, 2026-10) | (a) positions | (b) anchor names | (d) style + raw span | (e) error parity | Speed | Verdict |
|---|---|---|---|---|---|---|---|
| **libyaml-safer** | 0.3.0, safe-Rust port of libyaml (via unsafe-libyaml), single maintainer, low adoption (≈34k recent downloads), license listed "non-standard" (check the LICENSE: libyaml is MIT) | **Identical to go-yaml**: `start_mark` is at the anchor or tag, the null scalar is after `:`, the block scalar is at the indicator, `index` is a **byte** offset, `column` counts **chars**, 0-based (verified on the probe file) | **Yes** (`anchor: Option<String>`, `Alias{anchor}`) | style yes (`Plain/SingleQuoted/DoubleQuoted/Literal/Folded`); raw span via `start_mark.index..end_mark.index` | **Yes**: same problem/context strings. go-yaml's `(line, column)` formula reproduced exactly on 6 samples. It does *not* detect unknown aliases (that is composer work), so we do that and can give a real position. | **19.4 ms/pass** (go-yaml `Unmarshal` to Node ≈ 80 ms on the same file) | **Recommended** as the primary backend, **vendored or forked**. The fork must fix the verified panic on `a: \|⏎  x<EOF>` (literal block at EOF, no newline). Wrap calls in `catch_unwind` as a safety net, and add comment-token passthrough if wanted. |
| **saphyr-parser** | 0.2.2, MIT/Apache, active, YAML 1.2 test-suite compliant | span start is at the **content**, not the anchor or tag; the null scalar is *at* the `:`; the block scalar starts at the first content line; `Marker::index()` is documented as bytes but **was a char index** for `&str` input (verified with CJK). Line is 1-based, col 0-based. | **No**: only numeric anchor ids in events (`Event::Scalar(v, style, anchor_id, tag)`). Names would have to be recovered by scanning the source backwards. | style yes; raw span yes (char-indexed) | No (different messages, e.g. `while parsing node, found unknown anchor`). Value difference: a literal block at EOF without a newline gives `"x\n"` where libyaml gives `"x"`. | 23.4 ms/pass | Viable fallback, but parity needs back-scanning hacks for anchor names and property starts, plus a char→byte index map. Best choice if we drop go-yaml position parity. |
| **yaml-rust2** | 0.13.0, same lineage as saphyr | same limitations as saphyr (numeric anchor ids, content-start marks) | No | partial | No | ≈ saphyr | No advantage over saphyr-parser. |
| **marked-yaml** | 0.8.0, on yaml-rust2 | spans (start), keys must be strings | **Anchors/aliases unsupported** ("MAY NOT be used") | no style | No | n/a | Rejected (workflows use anchors; actionlint has anchor diagnostics). |
| **serde-saphyr / serde_yaml(deprecated) / serde_yml(deprecated)** | serde layer | positions only via `Spanned` hacks | No | No | No | n/a | Rejected for workflows. Acceptable for config files, but a custom loader keeps uniform diagnostics. |
| **Own scanner/parser** | n/a | full control | Yes | Yes | only if we port libyaml's messages | likely fastest | Not worth it initially. libyaml-safer *is* effectively that, already ported. Keep it as a long-term option if we fork. |

Differential check (§9.10): over 1733 files, libyaml-safer matched go-yaml on all 1719 files that parse,
node for node: positions, values and anchors. Its only failure was a **panic on a literal block scalar
at EOF without a trailing newline** (9 real-world files). The fork must patch this.

**Recommendation.** Build an internal crate `workflint-yaml` that:

1. Drives libyaml-safer's event parser (vendored). It converts each event into an **arena-allocated YAML
   tree** with these node fields:
   - `kind`;
   - `tag: Tag` (the explicit tag or our resolution);
   - `style`;
   - `anchor: Option<Sym>`;
   - `span: ByteSpan { start, end }`;
   - `pos: Pos { line, col }` computed exactly like go-yaml (start mark + 1);
   - `value: Str` for scalars;
   - `children: Range<NodeId>` (or `ThinVec<NodeId>`);
   - `alias_target: Option<NodeId>`.
2. Implements go-yaml v4's **resolver** bit-for-bit (`resolve.go`: the bool, null, float and merge
   literal table; ints with `_` stripped via Go-`ParseInt(base 0)` semantics; the float regex; the
   timestamp formats). This yields identical `!!tag` strings for messages.
3. Implements alias resolution the way actionlint's `resolveAliases` does: replace aliases with the
   target `NodeId` (cheap with an arena, no cloning), detect recursion by "target not yet closed", and
   track unused anchors per node. Detect unknown aliases with a real position. Under a parity flag, emit
   the go-yaml-style `0:0` message.
4. Converts libyaml errors with go-yaml's `fail()` formula (parity mode), or with proper 1-based
   problem-mark positions plus context (improved mode).
5. Stops after document 1. Optionally emits a diagnostic for extra documents (improvement).
6. Optionally collects comment spans (requires a small patch to the scanner), for future
   `# workflint-disable` support.

For struct-decoded files (`action.yml`, config) **reuse the same tree** with a tiny typed-extraction
layer instead of serde. This gives uniform positions in errors, explicit control of the YAML 1.1 bool
compatibility (`yes/on/y` → true when a bool is expected), merge-key handling, and duplicate checks that
match go-yaml's `uniqueKeys`. It also removes the "error with no position" cases.

### 9.2 Positions and spans

- Store **byte spans** (`u32` start and end) for every YAML node and AST leaf. Compute `Pos{line, col}`
  lazily from a per-file `LineIndex`: a sorted line-start offsets array plus per-line ASCII fast paths.
  Column is the count of chars from the line start, which reproduces go-yaml's rune column. Also cache
  the go-compatible start position (property start) separately from the content start, because they
  differ for anchored or tagged nodes and for block scalars.
- Diagnostics carry a span. Rendering computes display width (`unicode-width`) correctly, which fixes
  the byte/rune caret bug in `error.go`.
- For any scalar, keep `(style, raw_span, indent, chomping)` so that the expression and glob layers can
  call `scalar.source_pos_of(value_byte_offset) -> Pos`. This walks the raw source handling escapes
  (`\t`, `\uXXXX`, `''`), folded and literal line breaks, and indentation stripping. It fixes the
  long-standing TODO (§1.3). For parity mode, keep the old `pos.col + offset (+1 if quoted)` formula
  behind a flag.

### 9.3 AST representation

- **Arena plus IDs**, not `Box`/`Rc`: one `bumpalo`/`typed-arena`, or `Vec`-backed arenas
  (`la-arena`/`id-arena`) per file. The YAML tree is a DAG after alias resolution, and an arena with IDs
  represents shared subtrees naturally, where `Box` would clone them and `Rc` would add overhead. Rules
  get `&'ast` references. The whole file's memory is freed at once, which suits per-file parallelism.
- Leaf types: `Str<'src> { value: Cow<'src, str> or arena &str, quoted: bool, span: Span }`. Zero-copy is
  possible for plain scalars without escapes or folding (the value equals the raw slice). libyaml-safer
  allocates `String` values, so either accept one copy into the arena or patch it to yield borrowed
  slices. Keep `Option<Str>` for "absent" and a separate `Invalid`/`Error` marker where Go conflates
  "invalid" with "empty" (§8.9), e.g. `enum Field<T> { Absent, Invalid(Span), Value(T) }`.
- Typed scalars: `enum BoolLit { Lit(bool), Expr(Str), Invalid }` (and the same for Int and Float),
  instead of Go's `Value + Expression` pair.
- `enum Exec { Run(..), Action(..), DockerAction(..) }`, `enum Event { Webhook, Schedule, ... }`,
  `enum EnvDef { Vars(Map), Expr(Str), Invalid }`, `enum Matrix { Rows{..}, Expr(Str) }`,
  `enum MatrixValue { Scalar{text, tag}, Seq, Map }`. Keep the YAML tag in matrix scalars so comparisons
  can be type-aware (an improvement over Go's string-only `RawYAMLString`).
- Maps: `IndexMap<Sym, Entry>` (or a small `Vec<(Sym, Entry)>` with linear lookup; most maps are tiny)
  keyed by **case-folded interned symbol**, preserving source order. This makes traversal order
  deterministic everywhere, including the job visit order.
- Interning: `lasso::ThreadedRodeo` or a per-file `Rodeo` for keys and identifiers (job IDs, step IDs,
  input names, env names). Folded keys are interned once. Messages need original spellings, so keep both
  (`name: Str`, `id: Sym`).
- Keep **key spans** for every map entry (Go drops many). This enables better diagnostics and fixes.
- Make the schema **data-driven**: a static table per section of
  `{key, parser fn, case_sensitive, allow_empty, steps_only/call_only flags}`. Unknown-key messages and
  "expected one of" lists can then be generated from the table, which guarantees sorted, complete lists
  (fixing §8.5) and makes new GitHub keys trivial to add (§8.25).

### 9.4 Parser structure

- Port the Go parser 1:1 in structure (one function per production) over the arena YAML tree, with a
  `MappingIter` equivalent to `parseMapping` (null handling, kind check, `<<` rejection,
  first-wins duplicates, case folding, emptiness). Emit `Diagnostic { rule: "syntax-check", span, msg }`.
  Message strings must be byte-identical to Go's (port `quotes.go` = Go `strconv.Quote` semantics; test
  against `testdata/err/*.out`).
- Provide a `compat` setting:
  - **parity**: reproduce Go quirks such as `True` → false, the volumes/ports swap, the missing `services`
    steps-only key, the YAML error positions, the `"on"` snapshot label and expression positions;
  - **fixed** (the default once parity tests pass): the corrected behaviour. Gate every §8 item
    individually, so diffs against Go output on the real-world corpus can be explained.
- Never panic: no `unwrap` on parse paths. Fuzz with `cargo-fuzz` using the `fuzz/` corpus and the
  real-world corpus. Differential-fuzz against the Go binary (`.reference/bin`) for syntax-check output.

### 9.5 Visitor and rules

- Keep the single-traversal fan-out (`WorkflowPre → per job: JobPre, steps, JobPost → WorkflowPost`),
  because rules depend on it (stateful rules such as `expression` and `id`). Use a trait with default
  no-op methods, and static dispatch (`enum Rule` or a generic tuple) for the built-in rule set to avoid
  vtable cost. Iterate jobs **in source order**.
- Rules return diagnostics through a `&mut Sink`. Fatal internal errors use `Result`.
- Sort and dedup at the end as Go does. With source spans, dedup by `(file, span.start, msg)`.

### 9.6 Parallelism and caches

- Use `rayon` over files: parse plus rules per file on a worker, with read concurrency bounded by the
  pool.
- External processes (shellcheck) run on a separate bounded executor (outside this layer). Avoid
  pipe-deadlock bugs (#712, #702) by writing stdin from a separate thread or using async I/O.
- Project resolution: precompute it before the parallel phase, as Go does, or use a `DashMap` keyed by
  the canonicalised root. Use `Path::starts_with`, which is component-wise and fixes the prefix bug.
- Metadata caches: `DashMap<CanonicalPath, Arc<OnceLock<Result<Arc<Meta>, MetaError>>>>`. This gives
  **single-flight** loading, normalises keys (resolve `./a/` and `./a/../a` to a canonical path), and
  stores errors. To keep "report once" deterministic, either:
  - report metadata errors at **every** use site (simple, deterministic, and dedup handles repeats); or
  - after linting, attribute each metadata error to the lexicographically first (file, position) use.
    Preferred: deterministic and still "once".
- Pre-populating the reusable-workflow cache from linted ASTs is a nice optimisation, but it should not
  change diagnostics. Compute both the AST-derived and the file-derived metadata from the **same
  function** (parse the callee through the full parser), so they cannot disagree.

### 9.7 Action metadata and the popular-actions dataset

- Parse `action.yml` with the same YAML tree plus typed extraction. Keep Go's semantics:
  - `required && default == None`;
  - YAML 1.1 bools for `required`;
  - case-folded duplicate detection;
  - `deprecationMessage` presence means deprecated.

  Fix the alias and `<<` quirks. Check the runner's file preference (`action.yml` before `action.yaml`).
- Popular actions: convert `popular_actions.go` once into a checked-in **JSON/JSONL data file** using the
  existing generator's `-f jsonl` output, and generate Rust in `build.rs`. Either
  `phf::Map<&'static str, &'static ActionMeta>` or a sorted static slice with binary search; about 218
  entries plus 156 outdated specs. Static data needs no allocation at start-up.
  - Improvements: case-insensitive owner and repo matching.
  - Optional ref resolution: map a full SHA to a tag via an offline table or a user-provided
    lock/annotation (the `# v4.1.2` comment convention after SHA pins). This makes SHA-pinned actions
    checkable, which they currently are not.
  - Keep the `SkipInputs`/`SkipOutputs` flags and the special case for `actions/github-script`.
  - Port the generator to Rust (`xtask`) with retries and the same patch hooks.
- `AllWebhookTypes`: a static table generated from the same source.

### 9.8 Project and config

- Discovery: the same upward walk (`.github/workflows` dir plus `.git` file or dir). Use canonicalised
  paths. Consider `.forgejo/workflows` and `.gitea/workflows` behind a flag (#400). Lint only the top
  level of `workflows/` by default, matching GitHub, with a flag for recursion.
- Config: the same file names and precedence. Possibly add `$XDG_CONFIG_HOME` and layering (#675, #747)
  as opt-in extensions. Parse with the typed-extraction layer. **Warn** on unknown keys (with "did you
  mean"). Validate `ignore` entries as strings. Match `paths` globs against the path **relative to the
  project root** (fix §6.2; parity flag for the cwd-relative behaviour).
  - Glob crates: `globset` (supports `**` and `{a,b}`), or a port of doublestar's validation and
    matching rules if exact parity is needed. Note that doublestar `MatchUnvalidated` semantics (for
    example `**` matching zero directories, `*` not crossing `/`) must be equal; test with
    `config_test.go:153-222`.
  - `self-hosted-runner.labels` uses Go `path.Match`; port it exactly (small).
  - Use the `regex` crate for `ignore`. It is RE2-compatible for practical purposes; flag the rare
    syntax differences (Go supports `\Q..\E`, `\z`).

### 9.9 Glob validator

Port `glob.go` directly as a small hand-written char iterator that tracks a rune column (1-based, 0 on
any line after the first). Reproduce the messages exactly. Fix the escaped-character report bug (§7)
under compat gating. All glob test vectors in `glob_test.go` must pass.

### 9.10 Differential results and remaining unknowns

**Differential run (done).** I compared go-yaml's Node pre-order dump with libyaml-safer's event dump,
field by field: kind, `line:col`, the scalar value bytes and the anchor name. The corpus was
`.reference/realworld/*` (1503 files) plus `testdata/{ok,err,examples}/*.yaml`, 1733 files in total.

| Result | Files |
|---|---|
| Identical | **1719** (every node position, value and anchor matches) |
| YAML syntax error in both | 2 (`err/yaml_syntax_error`, `examples/broken_yaml`) |
| Unknown anchor: go-yaml errors, the libyaml event layer does not | 2 (`err/undefined_anchor`, `examples/dangling_alias`). Expected: that check belongs in our composer. |
| **libyaml-safer 0.3.0 panicked** | **9** real-world files (the only diffs) |

- The panic is in `scanner.rs:166`, "unexpected end of input".
- Minimal reproducer: `printf 'a: |\n  x'`. That is a literal block scalar as the last node with **no
  trailing newline at EOF**, which is common in the wild. Folded `>` in the same situation does not panic.
- This is a must-fix in a vendored fork, plus a `catch_unwind` safety net.
- saphyr-parser handles this input, but it yields the value `"x\n"` where go-yaml and libyaml yield `"x"`.
  That is a second saphyr parity difference.

Open questions to settle with prototypes:

1. libyaml-safer's handling of BOMs, CRLF and very long lines, compared with go-yaml (go-yaml v4 carries
   small libyaml patches; differential fuzzing will reveal them).
2. Comment capture: libyaml's scanner discards comments, so a patch is needed for suppression comments.
3. Whether to emit improved YAML error positions by default, which would change
   `testdata/err/yaml_syntax_error.out`-style expectations.

---

## Appendix A: implementation checklist for this layer

1. `workflint-yaml`: event loop over libyaml-safer, then the arena tree with go-compatible positions,
   then the go-yaml resolver, then alias resolution with recursive/unused diagnostics, then the error
   formula. Tests: dump-tree parity vs `yamlprobe` on the corpus.
2. `LineIndex` and span to `Pos`; scalar value-offset to source-pos mapping.
3. AST types (§2, §9.3) and a data-driven key schema.
4. Parser productions (§3), with messages byte-identical to `testdata/err/*.out` `[syntax-check]` lines
   (43 files contain syntax-check expectations).
5. Visitor (§4) with source-ordered jobs.
6. Action metadata, reusable-workflow metadata, single-flight caches, deterministic error attribution.
7. Popular-actions and webhook static data via `build.rs`.
8. Project discovery and config (all options in §6.2), with path matching relative to the root.
9. Glob validator (§7).
10. Compat flags for each §8 item, a fuzz target, and differential tests against the Go binary.
