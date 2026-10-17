# Differences from actionlint

workflint is compatible with actionlint at commit `011a6d1` (ADR 0003, ADR 0004). This file lists every place where workflint deliberately behaves differently. Any other difference from actionlint is a bug in workflint. An entry that changes the output of a fixture or of the corpus also has records in the conformance suite's allow-list, `tests/conformance/allowed-differences.toml`, keyed by the entry's anchor (ADR 0023). An entry that changes neither has no record.

Rules for this file:

- Add an entry before or in the same pull request as the change. Each allow-list record names the anchor and the fixture.
- Keep anchors stable. Other documents and the allow-list link to them.
- An entry describes behaviour that users can observe: output, exit codes, files read or written. Internal design choices that do not change output belong in the ADRs.
- Status is `planned` until the change ships, then `done` with the version.
- Issue numbers refer to `rhysd/actionlint`.

The last section lists behaviour that was investigated and deliberately kept.

## Output and positions

<a id="rule-id-in-brackets"></a>
### Rule ID in brackets instead of the kind

- Upstream: the default and `-oneline` output end each error with the coarse kind, for example `[expression]` or `[syntax-check]`.
- workflint: the brackets hold the fine-grained rule ID, for example `[undefined-property]`. Each rule maps to exactly one actionlint kind. In `-format` templates, `.Kind` keeps the actionlint kind and `allKinds` keeps listing kinds, so existing JSON, JSON Lines and SARIF templates produce the same values. A new `.Rule` field carries the rule ID. `{{json .}}` keeps its seven keys so its output stays byte-identical.
- Why: ignore comments and per-rule config need a stable, specific ID (ADR 0008, ADR 0009). The upstream problem matcher regex still matches, and its `code` group now captures the rule ID.
- Upstream issue: #71, #75.
- Milestone: M1 (diagnostic model), M5 (output).
- Status: planned. The `.Kind` and `.Rule` split for templates is a proposal to confirm in M5.
- Fixtures: every `.out` line differs in the bracket only. The conformance runner maps rule IDs back to kinds before comparing.

<a id="sort-order"></a>
### Order of diagnostics at the same position

- Upstream: diagnostics are sorted by file, line, column, then message text.
- workflint: diagnostics are sorted by file, line, column, rule ID, then message text (ADR 0007). Deduplication is unchanged: equal file, line, column and message collapse into one. The diagnostic kept has the highest severity and, among those, the smallest rule ID, which is the ID shown in brackets.
- Why: the order of findings at one position should not depend on message wording.
- Upstream issue: none.
- Milestone: M1.
- Status: planned.

<a id="column-units"></a>
### Columns are always Unicode code points

- Upstream: columns from YAML nodes count code points, but positions inside `${{ }}` add byte offsets to the node column. In `"日本語" ${{ matrix.foo }}` the error is reported at column 35, while the code-point column is 29. The `^~~~` indicator slices the source line by bytes, so it lands on the wrong token after non-ASCII text. It counts a tab as zero cells, so a tab before the column moves the `^` left, and East Asian ambiguous characters count as two cells in a CJK locale. The `~` run covers the characters from the column to the next space or tab, whatever the error is about. `EndColumn` in templates is a display width, a third unit.
- workflint: text output, `-format` fields and JSON use 1-based code-point columns everywhere. `EndColumn` uses the same unit as `Column`. SARIF and the future language server use UTF-16 code units, as their specifications require (ADR 0007). The indicator line works like this:
  - The text before the column becomes as many spaces as its display width. A tab stays a tab, so the `^` lines up with the source line at any tab width.
  - The display width is measured over each run of text between tabs, so an emoji sequence takes the cells a terminal gives it. `❤️` (U+2764 U+FE0F) and emoji joined with U+200D take two cells. Wide characters, such as CJK characters and most emoji, take two cells. Ambiguous characters take one cell in every locale, and control characters take none.
  - The `^` and `~` run underlines the diagnostic's span up to the end of its line, so it covers the whole token, spaces included. A tab inside the span stays a tab. Tabs at the start of the span are copied before the `^`, so the `^` sits under the first character after them. Tabs at the end of the span are not underlined. A span with nothing visible on the line, such as one at the end of a line or one of only tabs, gets a single `^` at its start.
  - An empty span falls back to upstream's run: from the column to the next space or tab.
- Why: one consistent unit. ASCII-only files without tabs report the same columns as upstream. Their indicator differs only where the span is not the run up to the next space, for example a message about a quoted string that contains spaces.
- Upstream issue: none.
- Milestone: M1, M3.
- Status: planned.

<a id="positions-inside-scalars"></a>
### Exact positions inside multi-line and quoted strings

- Upstream: the position of an error inside a string, such as an expression or glob error, is the scalar's start plus a byte offset, plus 1 if quoted. In a `run: |` block every error is reported on the indicator line. Escapes such as `\t` or `''` shift the column by one per escape. A bare `if: "foo"` ignores the quote and reports column 13 instead of 14. A `TODO` at `rule_expression.go:736` notes the problem.
- workflint: errors are mapped through the scalar's source map (ADR 0005) to the exact line and column. The maps are exact in every scalar style, including escapes, `''`, line folding, and block indentation. A character that an escape produces maps to the escape's backslash.
- Why: a position on the wrong line sends users to the wrong place.
- Upstream issue: #330.
- Milestone: M2, M3, M4. Expression syntax errors use the exact positions from M2.
- Status: planned.
- Fixtures: `.out` lines for errors inside multi-line or escaped strings.

<a id="script-finding-positions"></a>
### shellcheck and pyflakes findings point into the script

- Upstream: every shellcheck and pyflakes finding is reported at the `run:` key. The position inside the script appears only in the message, for example `SC2086:info:1:6:`.
- workflint: the finding is reported at its line and column in the workflow file, mapped through the `run:` scalar's source map. The message text is unchanged.
- Why: users want the editor or annotation to point at the faulty line (ADR 0012).
- Upstream issue: #88, #360, PR #556.
- Milestone: M5.
- Status: planned.
- Fixtures: the shellcheck and pyflakes lines in `err/` and `examples/`.

<a id="yaml-error-positions"></a>
### YAML syntax error positions

- Upstream: YAML syntax errors use go-yaml's position, which takes the line from libyaml's context mark when it has one and reports a 0-based column. `steps: -` is reported at `2:7`, although the problem is at `2:8`. A control character in the file gives `control characters are not allowed` at `0:0`, because libyaml's reader records only a byte offset. Bytes that do not decode as UTF-8, or as UTF-16 after a UTF-16 byte order mark, give an encoding error such as `invalid leading UTF-8 octet` at `0:0` too. go-yaml decodes the file in chunks while it parses, so when the bad bytes are in a later chunk than a syntax error, it reports the syntax error instead.
- workflint: the error is reported at the 1-based problem position, a control character at its own position, and an encoding error at the first character that does not decode. workflint decodes the whole file before it parses, so it always reports the encoding error. The message text is unchanged.
- Why: the upstream position is off by one column and sometimes on the wrong line.
- Upstream issue: #519.
- Milestone: M1.
- Status: planned.
- Fixtures: `err/yaml_syntax_error`, `examples/broken_yaml`.

<a id="empty-scalar-past-last-line"></a>
### Empty value after the last line of a file without a final line break

- Upstream: go-yaml ends the stream on a new line, so an empty scalar at the end of a file without a final line break is on the line after the last one. A file that holds only `---` has its empty document at `2:1`, and a file that holds only `? ` has the empty value of its key at `2:1`.
- workflint: positions are byte offsets into the file, and no offset lies on a line after the last one. The empty document is at `1:4`, after the `---`, and the empty value is at `1:3`. With a final line break, both tools report `2:1`.
- Why: a position must point into the file.
- Upstream issue: none.
- Milestone: M1.
- Status: planned. No fixture covers it.

<a id="unknown-anchor-position"></a>
### Undefined alias position and message prefix

- Upstream: an alias to an undefined anchor is reported at `0:0` as `could not parse as YAML: yaml: unknown anchor 'X' referenced`, with go-yaml's `yaml:` prefix repeated inside the message.
- workflint: the error is reported at the alias as `could not parse as YAML: unknown anchor 'X' referenced`. As upstream, no rules run on that file.
- Why: `0:0` is not a position, and other YAML errors drop the inner prefix.
- Upstream issue: none.
- Milestone: M1.
- Status: planned.
- Fixtures: `err/undefined_anchor`, `examples/dangling_alias`.

<a id="long-line-snippets"></a>
### Snippets for lines over 64 KiB

- Upstream: the source line for the snippet is read with Go's `bufio.Scanner`, which fails on lines longer than 64 KiB. The error then prints without a snippet.
- workflint: the snippet is printed whatever the line length.
- Why: the limit is an artefact of the Go reader.
- Upstream issue: none.
- Milestone: M1.
- Status: planned.

<a id="snippet-line-breaks"></a>
### Snippet lines end at every YAML line break

- Upstream: the YAML parser counts a lone `\r`, U+0085, U+2028, and U+2029 as line breaks, so the line number in the header counts them. The snippet reader splits lines at `\n` only. In a file with one of these breaks, the snippet shows a different line than the header names, or no snippet at all.
- workflint: the snippet shows the line the header names. It ends at the same line breaks the YAML parser counts.
- Why: the snippet must show the line the diagnostic is on.
- Upstream issue: none.
- Milestone: M1.
- Status: planned.

## Workflow parser

<a id="nan-panic"></a>
### No panic on NaN

- Upstream: `timeout-minutes: !!float NaN` makes `strconv.ParseFloat` succeed with NaN, and the error path then calls `Error()` on a nil error, which crashes actionlint (`parse.go:305-306`).
- workflint: the value is rejected with the same message as other invalid floats, `invalid float value: "NaN": strconv.ParseFloat: parsing "NaN": invalid syntax`. More generally, no input makes workflint panic. If a rule panics anyway, workflint reports an `internal-error` diagnostic for that file, lints the other files, and exits with code 3 (ADR 0007).
- Why: a crash loses every other result.
- Upstream issue: #753.
- Milestone: M2.
- Status: planned.

<a id="yaml-node-limit"></a>
### Workflows over GitHub's YAML node limit

- Upstream: the parser walks an anchored subtree again at each alias that uses it. Nested aliases multiply, so a "billion laughs" workflow, nine levels of anchors with ten aliases each, makes the parser walk 10^9 nodes, and actionlint does not finish.
- workflint: before it parses, the parser counts the YAML events that GitHub's reader reads, with each alias expanded: the stream and document markers, each scalar, and the start and end of each collection. The reader is `YamlObjectReader` in `actions/runner` at `9abfe11`, and it stops past 50,000 events with `Maximum YAML nodes exceeded`. Past the same limit, workflint reports `workflow has more than 50000 YAML nodes when its aliases are expanded. GitHub rejects it with "Maximum YAML nodes exceeded"` at the start of the document, with the rule `too-many-yaml-nodes`, and parses nothing else. The count takes time in proportion to the size of the file. A recursive alias counts as one event, because the YAML layer reports it already.
- Why: a linter must finish on any input, and GitHub rejects these files anyway.
- Upstream issue: none.
- Milestone: M2.
- Status: planned. No fixture covers it.

<a id="container-volumes"></a>
### Container `volumes` is kept separate from `ports`

- Upstream: the parser stores `volumes` in the `Ports` field (`parse.go:1065-1066`). When both keys exist, the later one replaces the earlier, so expressions in the other list are never checked.
- workflint: `ports` and `volumes` are parsed and checked separately.
- Why: the field assignment is a copy-paste mistake.
- Upstream issue: #747.
- Milestone: M2.
- Status: planned.

<a id="capitalized-booleans"></a>
### `True` and `TRUE` are true

- Upstream: YAML resolves `True` and `TRUE` as booleans, but the parser compares the text with `"true"` case-sensitively, so they become false (`parse.go:253`). `required: True` on a `workflow_call` input is treated as not required, which hides missing-input errors.
- workflint: `true`, `True` and `TRUE` are true, and `false`, `False` and `FALSE` are false, as YAML defines.
- Why: the value contradicts the YAML type actionlint itself assigned.
- Upstream issue: none.
- Milestone: M2.
- Status: planned.

<a id="reusable-call-services"></a>
### `services` on a reusable workflow call job

- Upstream: a job with `uses:` may not have `runs-on`, `steps`, `container` and other step-job keys, but `services` is missing from that list (`parse.go:1396`) and is accepted.
- workflint: `services` gets the same error as the other keys, `when a reusable workflow is called with "uses", "services" is not available. ...`.
- Why: GitHub does not allow `services` on a job that calls a reusable workflow. The upstream message itself lists the allowed keys, and `services` is not among them.
- Upstream issue: #748.
- Milestone: M2.
- Status: planned.

<a id="reusable-call-keys"></a>
### Every misplaced key of a reusable workflow call job is reported

- Upstream: a job with `uses` reports only the last key written that only a job with steps may have, such as `runs-on` or `env` (`parse.go:1461`). A job without `uses` reports only the last of `with` and `secrets` (`parse.go:1479`). After fixing that key, the next run reports the one before it.
- workflint: each such key gets its own error, at its key.
- Why: one run should show every key to remove. No fixture and no corpus file has a job with two such keys, so no expected output changes.
- Upstream issue: none.
- Milestone: M2.
- Status: planned.

<a id="snapshot-section-name"></a>
### `snapshot` errors name the right section

- Upstream: the `snapshot` mapping is parsed with the section name `"on"` (`parse.go:1305`), so an error such as an empty mapping says `"on" section`.
- workflint: such errors say `"snapshot" section`.
- Why: the section name is a copy-paste mistake.
- Upstream issue: none.
- Milestone: M2.
- Status: planned.

<a id="dispatch-input-key-list"></a>
### Complete key list for `workflow_dispatch` inputs

- Upstream: an unknown key in a `workflow_dispatch` input lists only `"default", "description", "required"` as expected keys (`parse.go:444`), although `type` and `options` are also accepted.
- workflint: the message lists `"default", "description", "options", "required", "type"`.
- Why: the hint should list every valid key.
- Upstream issue: none.
- Milestone: M2.
- Status: planned.
- Fixtures: `err/case_sensitive_keys`, `err/unexpected_keys`.
- Corpus: `netdata_netdata_.github_workflows_dashboard-pr.yml`, `netdata_netdata_.github_workflows_docker.yml`, `netdata_netdata_.github_workflows_packaging.yml`. The allow-list has no records for them, because `cargo xtask oracle` cannot apply records to corpus files yet (#49).

<a id="empty-event-name-section"></a>
### Section name of an event with an empty name

- Upstream: when the body of an event with an empty name is not a mapping, the message names the section `%q section`. An event key that is not a scalar also has the empty name. The parser formats the section name only when the event name is not empty (`parse.go:60-66`), so the format verb stays in the message.
- workflint: the message names the section `"" section`, as upstream's unexpected-key messages for the same event already do.
- Why: `%q` is an unformatted verb, not a section name.
- Upstream issue: none.
- Milestone: M2.
- Status: planned.
- Fixtures: `workflint/err/empty_event_name`.

<a id="action-metadata-aliases"></a>
### Aliases and merge keys in `action.yml` inputs

- Upstream: when an input in a local `action.yml` is an alias (`bar: *input`), the scan for `deprecationMessage` and for unknown keys sees nothing, so a deprecated aliased input is not reported. An alias used as an input name (`*name: {}`) gives an input named after the anchor. A `<<` merge key under `inputs` becomes an input named `<<`.
- workflint: aliases under `inputs` are resolved before inputs are read, both as input names and as input definitions. A merge key under `inputs` merges the referenced inputs, as go-yaml does for the rest of the file: the inputs written under `inputs` come first, and a merged input does not replace one with the same name. With a sequence of mappings, earlier mappings win. A merged input whose name differs from another only in case is a duplicate. A merge value that is not a mapping or a sequence of mappings, or that contains itself, is an error with go-yaml's text. A `<<` key inside one input's definition is still an unknown key.
- Why: an alias should behave like the node it refers to.
- Upstream issue: none.
- Milestone: M2.
- Status: planned.

<a id="non-scalar-keys"></a>
### Keys that are not strings in `permissions` and `env`

- Upstream: a key of `permissions` or `env` that is a sequence or a mapping, such as `? [x]`, gets `expected scalar node for string value but found sequence node with "!!seq" tag`, and the parser keeps the entry under the name `""`. The `permissions` rule then also reports `unknown permission scope ""` at the key.
- workflint: the parser reports the key and leaves the entry out, so the `permissions` rule does not report it again. In `env` the output does not change, because no rule reports an empty variable name.
- Why: the second error repeats the first and names a scope that is not in the file.
- Upstream issue: none.
- Milestone: M2.
- Status: planned. No fixture covers it.

<a id="invalid-value-reported-once"></a>
### An invalid value is reported once

- Upstream: the parser reports a value that is not a valid string and keeps it as the string `""`. The rules then check that string as if the file had it, and these cases get a second error:
  - An activity type that is empty, null, or not a scalar, as in `types: [[x]]`, `types: ['']`, `types: ''`, or `types:`, also gets `invalid activity type "" for "pull_request" Webhook event` (`events`).
  - An event key that is not a scalar, as in `[a]: x` under `on`, also gets `unknown Webhook event ""` (`events`).
  - A `shell` of `defaults.run` or of a step that is empty or not a scalar also gets `shell name "" is invalid` (`shell-name`).
  - A `runs-on` label that is empty or not a scalar also gets `label "" is unknown` (`runner-label`).
- workflint: the parser reports the value once, and the rules skip it. The value keeps its place in its list, so a check that only needs the value to be written still runs. `options: [[x]]` on a `string` input of `workflow_dispatch` still gets `"options" can not be set to "text" input because its input type is not "choice"`, and `options: ['']` on a `choice` input counts as set, as upstream. The keys of `permissions` and `env` are in [Keys that are not strings in `permissions` and `env`](#non-scalar-keys).
- Why: the second error repeats the first and names a value that is not in the file.
- Upstream issue: none.
- Milestone: M2.
- Status: planned.
- Fixtures: `err/issue280_runs_on`, `workflint/err/invalid_value_reported_once`.

<a id="typed-decoding-edge-cases"></a>
### Rare inputs to the typed decoding of local metadata

- Upstream: go-yaml decodes local `action.yml` files and the `on:` section of local reusable workflows into Go types. A few inputs that no real file uses take paths that workflint does not reproduce:
  - A scalar with an explicit `!!timestamp` tag must be a valid date, or decoding fails with `cannot decode !!str ... as a !!timestamp`.
  - A mapping or sequence with an explicit `!!null` tag as the value of `inputs`, `outputs`, or `secrets` skips the reference's own code for these sections, and go-yaml decodes it by its generic rules.
  - A plain `<<` key with an explicit `!!str` tag is an ordinary key.
  - A mapping with many equal keys gives an error for every two of them, so 10,000 equal keys give about 50 million errors.
  - In a `cannot use '...' as a map key` message, map keys of different Go types are printed in an order that depends on the address of their type.
- workflint: an explicit `!!timestamp` tag is not checked. A `!!null` mapping or sequence is decoded like an untagged one. A plain `<<` key is a merge key whatever its tag. One duplicate key check reports at most 1,000 errors. Map keys of different types are printed nil first, then bools, integers, floats, and strings.
- Why: these cases need go-yaml internals that no metadata file depends on, and the error count must stay bounded.
- Upstream issue: none.
- Milestone: M2.
- Status: planned. No fixture covers it.

## Expressions

<a id="int-literal-range"></a>
### Integer literals beyond 32 bits

- Upstream: integer literals are parsed as 32-bit integers, so `${{ 2147483648 }}` is an error: `parsing invalid integer literal "2147483648": strconv.ParseInt: parsing "2147483648": value out of range`.
- workflint: integer literals are accepted in the range of a double, as the runner evaluates numbers as doubles. Decimal and `0x` hexadecimal literals become the nearest double, so `9007199254740993` is `9007199254740992`. A literal beyond the largest double, such as a 1 followed by 309 zeros, is still an error, with upstream's message: `parsing invalid integer literal "...": strconv.ParseInt: parsing "...": value out of range`.
- Why: upstream rejects valid expressions.
- Upstream issue: PR #733.
- Milestone: M3.
- Status: planned.

<a id="exponent-plus-sign"></a>
### `+` in a number's exponent

- Upstream: `${{ 1e+5 }}` is a lexer error, because only `-` is accepted after `e`.
- workflint: `1e+5` is a valid number, as in JSON.
- Why: upstream rejects valid expressions.
- Upstream issue: PR #708.
- Milestone: M3.
- Status: planned.

<a id="comparison-precedence"></a>
### Comparison operator precedence

- Upstream: all six comparison operators share one precedence level and associate to the right. `'a' < 'b' == true` parses as `'a' < ('b' == true)` and reports `"string" value cannot be compared to "bool" value with "<" operator`.
- workflint: `<`, `<=`, `>` and `>=` bind tighter than `==` and `!=`, and all associate to the left, following GitHub's documented operator order. `'a' < 'b' == true` parses as `('a' < 'b') == true`. `&&` and `||` keep upstream's grouping to the right, which evaluates to the same value as any other grouping.
- Confirmed against the expression parser in `actions/languageservices` (`expressions/src/parser.ts`) at commit `4043eda`. From the loosest: `||`, then `&&`, then `==` and `!=`, then `<`, `<=`, `>` and `>=`, then `!`, then property access and index access. The comparison and equality levels are left-associative loops. The `&&` and `||` levels collect any number of operands into one node.
- Why: the tree must match how GitHub evaluates the expression, or type errors are wrong.
- Upstream issue: none.
- Milestone: M3.
- Status: planned.

<a id="expression-nesting-limit"></a>
### Nesting limit for expressions

- Upstream: expressions can nest without limit.
- workflint: an expression nested more than 128 levels deep is an error, `expression is nested more than 128 levels deep`, reported by `invalid-expression-syntax`. Each parenthesis, `!`, function call, property access, object filter, index access, and binary operator adds a level.
- Why: the parser and the checks walk the tree recursively, and a limit keeps a crafted expression from overflowing the stack. The deepest expression in the corpus is 11 levels deep. GitHub's parser in `actions/languageservices` stops at 50 levels.
- Upstream issue: none.
- Milestone: M3.
- Status: planned.

<a id="bool-assignability"></a>
### `bool` no longer accepts every type

- Upstream: any type is assignable to `bool` (`expr_type.go:123-128`). As a result, a string or number passed in `with:` to a `boolean` input of a local reusable workflow is never reported, and the type checks on `!`, `if:` and the `case()` predicate can never fire.
- workflint: only `bool` and `any` are assignable to `bool` in assignments, which covers `with:` values for `boolean` inputs and function parameters. Where GitHub converts values to booleans, in `if:`, `!`, `&&`, `||` and the `case()` predicate, every type is still accepted.
- Why: GitHub rejects a non-boolean value for a `boolean` input of a reusable workflow at run time.
- Upstream issue: none.
- Milestone: M3, M4.
- Status: planned.

<a id="literal-index-case"></a>
### Case-insensitive literal index keys

- Upstream: property access with a dot is case-insensitive, but an index with a string literal is not. `steps['MyStep']`, `matrix['OS']` and `github['EVENT']` report `property ... is not defined`. The untrusted-input check misses `github['EVENT']['ISSUE']['TITLE']`.
- workflint: literal index keys are compared case-insensitively, like dot access, in type checking and in the untrusted-input check. Messages keep the original spelling.
- Why: GitHub treats both forms the same.
- Upstream issue: none.
- Milestone: M3.
- Status: planned.

<a id="fromjson-key-case"></a>
### Case-insensitive keys from `fromJSON` literals

- Upstream: object keys in a `fromJSON('...')` literal keep their case while property names are lowercased, so `fromJSON('{"Foo":1}').Foo` reports `property "foo" is not defined in object type {Foo: number}`.
- workflint: keys from `fromJSON` literals are matched case-insensitively. They keep their spelling in printed types, so the object above prints as `{Foo: number}`. If two keys differ only in case, the key spelled exactly like the property wins, so `.foo` reads `foo` and `['Foo']` reads `Foo`.
- Why: upstream rejects valid code. Property access is case-insensitive at run time.
- Upstream issue: none.
- Milestone: M3.
- Status: planned.

<a id="fromjson-nesting"></a>
### Deeply nested `fromJSON` literals

- Upstream: the type of a `fromJSON('...')` literal follows its value at any depth up to the 10,000 levels that Go's `encoding/json` accepts.
- workflint: a value nested more than 128 levels deep has type `any`. The syntax check still covers the whole literal, with Go's limit and message.
- Why: printing, merging, and comparing types recurse, and a type 10,000 levels deep could overflow the stack. The expression parser has the same limit of 128 levels.
- Upstream issue: none.
- Milestone: M3.
- Status: planned. No fixture covers it.

<a id="object-filter-mutation"></a>
### Object filters do not change shared types

- Upstream: typing `x.*` marks the array type of `x` as filtered in place (`expr_sema.go:721`). The type is shared for the whole job, so after `${{ matrix.os.* }}` a later `${{ matrix.os.a }}` is wrongly accepted.
- workflint: types are immutable. Each expression is checked against the original types.
- Why: the result of one expression must not depend on an earlier one.
- Upstream issue: none.
- Milestone: M3.
- Status: planned.

<a id="object-merge-order"></a>
### Merged object types do not depend on map order

- Upstream: merging two object types adds each property that only the second one has to the element type of the result, in Go map order, which is random (`expr_type.go:343-351`). The merged element type depends on that order, so the result can vary between runs. A map of `number` merged with a strict object `{a: string; b: bool}` prints as `{string => string}` or as `object`.
- workflint: the properties are added in byte order of their names, so that merge always gives `{string => string}`.
- Why: output must not vary between runs (ADR 0013).
- Upstream issue: none.
- Milestone: M3.
- Status: planned.

<a id="matrix-include-expressions"></a>
### Expressions in `matrix.include` elements are checked

- Upstream: when an element of `include:` is a whole `${{ }}` expression, the rule checks `m.Include.Expression`, which is always nil on that path, instead of the element (`rule_expression.go:950`). `include: ["${{ undefined }}"]` gives no error, and the element's type is not merged into the matrix type, which causes false `property ... is not defined` errors later.
- workflint: each element expression is checked, and its object type is merged into `matrix`. A syntax error in an element expression is reported from M2.
- Why: the upstream code reads the wrong variable.
- Upstream issue: none.
- Milestone: M2, M4.
- Status: planned.

<a id="all-expressions-in-string"></a>
### Every `${{ }}` in a string is checked

- Upstream: the first `${{ }}` with any error, including an untrusted-input error, stops checking the rest of the string (`rule_expression.go:755-757`). `echo ${{ foo }} ${{ bar }}` reports only `foo`.
- workflint: every expression in the string is checked and reported. After a syntax error, the search for the next `${{` goes on after the `}}` that ends the broken expression, or after the character that the lexer rejects if it finds no `}}`. Syntax errors are reported from M2, and type and context errors from M4.
- Why: users fix one error, rerun, and find the next one.
- Upstream issue: none.
- Milestone: M2, M4.
- Status: planned.

<a id="invalid-if-condition"></a>
### An empty or non-string `if`

- Upstream: the parser keeps an `if` whose value is empty or not a scalar as an empty string after it reports the value (`parse.go:204-209`). The `expression` rule then parses that empty string as a condition and reports a second error at the same position: `unexpected end of input while parsing variable access, function call, null, bool, int, float or string. expecting "IDENT", "(", "INTEGER", "FLOAT", "STRING"`.
- workflint: only the parser's error is reported, such as `string should not be empty`.
- Why: the second error is about the empty string that upstream puts in place of the value, not about the file.
- Upstream issue: none.
- Milestone: M2.
- Status: planned. No fixture covers it.

<a id="jobs-context-result"></a>
### `jobs.<job_id>.result` in reusable workflow outputs

- Upstream: the `jobs` context in `on.workflow_call.outputs.<id>.value` has only `outputs`, so `${{ jobs.build.result }}` reports `property "result" is not defined`.
- workflint: each job has `outputs` and `result`.
- Why: upstream rejects valid code. GitHub documents `jobs.<job_id>.result`.
- Upstream issue: #343, PR #724.
- Milestone: M4.
- Status: planned.

<a id="secrets-context-mixed-triggers"></a>
### `secrets` in workflows with several triggers

- Upstream: when `on.workflow_call.secrets` exists, `secrets` only has the declared secrets for every job, even when the workflow also runs on `push` or `workflow_dispatch`, where every repository secret is available (`rule_expression.go:152-158`).
- workflint: `secrets` is limited to the declared secrets only when `workflow_call` is the only trigger. Otherwise any secret name is accepted.
- Why: upstream rejects valid workflows that have more than one trigger.
- Upstream issue: #701, #148, PR #703.
- Milestone: M4.
- Status: planned.

<a id="tag-before-flow-comma"></a>
### A tag followed by a comma in a flow collection

- Upstream: go-yaml allows `,`, `[`, and `]` in every tag. In `[!!str, x]` the tag is `!!str,`, and the sequence has one item, `x`.
- workflint: in a flow collection, a tag ends before `,`, `[`, and `]`, as in libyaml and the YAML test suite (case WZ62). `[!!str, x]` has two items: an empty string tagged `!!str`, and `x`. Outside flow collections, a tag takes these characters, as in go-yaml.
- Why: a comma separates entries in a flow collection. go-yaml keeps a rule that libyaml dropped.
- Upstream issue: none.
- Milestone: M1.
- Status: planned. No fixture covers it.

<a id="yaml-solidus-escape"></a>
### The `\/` escape in double-quoted strings

- Upstream: go-yaml rejects the escape `\/` in a double-quoted scalar with `found unknown escape character`, so `run: "echo \/"` does not parse.
- workflint: `\/` is a `/`. YAML 1.2 defines this escape (section 5.7), and libyaml accepts it. Other escapes match go-yaml, including `\'`, which YAML does not define.
- Why: upstream rejects valid YAML. ADR 0022 keeps go-yaml's behaviour except where go-yaml rejects valid YAML.
- Upstream issue: none.
- Milestone: M1.
- Status: planned. No fixture covers it.

<a id="yaml-1-2-directive"></a>
### The `%YAML 1.2` directive

- Upstream: go-yaml accepts only `%YAML 1.1`. A file that starts with `%YAML 1.2` fails with `found incompatible YAML document`.
- workflint: `%YAML 1.1` and `%YAML 1.2` are accepted. Other versions fail with the same message as upstream.
- Why: upstream rejects valid YAML 1.2 documents, such as the YAML test suite cases 27NA and RTP8. ADR 0022 keeps go-yaml's behaviour except where go-yaml rejects valid YAML.
- Upstream issue: none.
- Milestone: M1.
- Status: planned. No fixture covers it.

<a id="yaml-core-schema"></a>
### Plain scalars follow the YAML 1.2 core schema

- Upstream: go-yaml resolves some plain scalars with YAML 1.1 rules. `1_000` and `0b10` become integers that later fail Go's number parsing, and dates such as `2026-10-15` get the `!!timestamp` tag, which appears in messages. A plain `<<` gets the `!!merge` tag, so `timeout-minutes: <<` reports a `"!!merge" tag`, and `-0` is a float.
- workflint: plain scalars are typed under the YAML 1.2 core schema (ADR 0005). `1_000`, `0b10`, `2026-10-15`, and `<<` are strings, so a numeric key such as `timeout-minutes: 1_000` reports a string where a number is expected, and no message names a `!!timestamp` or `!!merge` tag. `-0` is an integer.
- Why: GitHub types plain scalars with the 1.2 core schema, so the diagnostic describes what GitHub does with the file. `YamlObjectReader` in `actions/runner` at `9abfe11` (`src/Sdk/WorkflowParser/Conversion/YamlObjectReader.cs`) tries null, boolean, integer, and float in turn, with the core schema's patterns, and reads any other plain scalar as a string. Its patterns match `crates/workflint-yaml/src/resolve.rs`: `True` is a boolean, `0o17` and `0x1F` are integers, `.inf` is a float, and `yes`, `1_000`, and `2026-10-15` are strings. Quoted and block scalars are strings.
- Upstream issue: none.
- Milestone: M1.
- Status: planned. The conformance allow-list records each fixture whose message changes.

<a id="quoted-scalar-types"></a>
### Quoted values are strings

- Upstream: literal values in `with:` and in matrix rows are typed from their decoded text, ignoring quotes. `with: { flag: 'true' }` to a `string` input reports `bool value cannot be assigned`, and the matrix value `"3.10"` is typed as a number.
- workflint: types come from the YAML tag and style. Quoted and block scalars are strings. Plain scalars are typed as before.
- Why: upstream rejects valid code. Quoting is how users write a string in YAML.
- Upstream issue: #225, #250, PR #727.
- Milestone: M4.
- Status: planned.

## Rules

<a id="if-cond-closing-braces"></a>
### `}}` inside a string literal in `if:`

- Upstream: the `if-cond` rule finds the end of `${{` with the first `}}` in the text, so `if: ${{ github.ref == '}}' }}` is reported as always true.
- workflint: the end of the expression is found by the expression lexer, which skips string literals.
- Why: upstream rejects a valid condition.
- Upstream issue: none.
- Milestone: M4.
- Status: planned.

<a id="glob-escape-character"></a>
### Escaped characters in branch and tag globs

- Upstream: for `\[`, `\?` and `\*` in a branch or tag filter, the error reports the character after the escaped one (`glob.go:100`). `\[x` reports `'x'`, and `\[` at the end reports the end-of-input rune.
- workflint: the error reports the escaped character, for example `'['`. The column is the same as upstream.
- Why: the message names the wrong character.
- Upstream issue: none.
- Milestone: M2.
- Status: planned.
- Fixtures: none. The `workflint-syntax` test `an_escaped_special_character_in_a_ref_glob_reports_that_character` covers it.

<a id="branding-color-message"></a>
### Branding color message names `branding.color`

- Upstream: an invalid color in a local action's metadata reports `incorrect color %q at branding.icon ...` (`rule_action.go:565`).
- workflint: the message says `at branding.color`.
- Why: the upstream message names the wrong key.
- Upstream issue: none.
- Milestone: M4.
- Status: planned.
- Fixtures: `examples/action_metadata_syntax_validation`, `projects/local_action_invalid`.

<a id="dispatch-options-quoting"></a>
### `workflow_dispatch` options quoted once

- Upstream: when a `choice` input's default is not among its options, the options are quoted twice: `... is not included in its options "\"Tama\", \"Mike\""` (`rule_events.go:268`).
- workflint: `... is not included in its options "Tama", "Mike"`.
- Why: the escaped quotes are unreadable.
- Upstream issue: none.
- Milestone: M4.
- Status: planned.
- Fixtures: `err/workflow_dispatch_input_types`, `examples/workflow_dispatch_input_types`.

<a id="deprecated-command-digits"></a>
### Deprecated commands with digits in the name

- Upstream: the pattern for `::set-output`, `::save-state` and `::set-env` requires the name to match `[a-zA-Z][a-zA-Z_-]*`, so `::set-output name=result1::x` is not reported.
- workflint: digits are allowed after the first letter.
- Why: upstream misses real uses. The command is deprecated whatever the name.
- Upstream issue: none.
- Milestone: M4.
- Status: planned.

<a id="deterministic-order"></a>
### Deterministic job order

- Upstream: jobs are stored in a Go map and visited in random order. When there are several cycles in `needs`, which one is reported varies between runs. When a label conflicts with several others, the partner named in `runner-label-conflict` varies too.
- workflint: jobs are visited in source order. The first cycle found in source order is reported, and the conflict partner is the earliest conflicting label.
- Why: the same input must give the same output.
- Upstream issue: none.
- Milestone: M4.
- Status: planned.

<a id="cached-metadata-errors"></a>
### Errors in shared local actions and reusable workflows

- Upstream: an error in a local `action.yml` or a local reusable workflow is reported once per run, at whichever use loaded it first. Files are linted in parallel goroutines and jobs in random order, so the file, the position and even the kind (`action`, `workflow-call` or `expression`) vary between runs. The cache is not single-flight, so two concurrent first uses can both report the error. `testdata/projects/broken_reusable_workflow/README.md` documents the race.
- workflint: each action or workflow is loaded once. Its errors are reported once, at the first use in order of file path and then source position, by the `action` or `workflow-call` rule.
- Why: the same input must give the same output.
- Upstream issue: none.
- Milestone: M5.
- Status: planned.
- Fixtures: `projects/broken_local_action`, `projects/broken_reusable_workflow`, `projects/workflow_call_not_found`.

## Configuration and project discovery

<a id="paths-relative-to-root"></a>
### `paths` globs match from the project root

- Upstream: `paths.<glob>` in the config is documented as relative to the repository root, but it is matched against the path relative to the current directory. Run from a subdirectory, the path becomes `../.github/workflows/x.yaml`, and the ignores stop working.
- workflint: globs match the path relative to the project root, with `/` separators on every OS (ADR 0010).
- Why: this is the documented behaviour.
- Upstream issue: #481.
- Milestone: M5.
- Status: planned.

<a id="project-root-boundary"></a>
### Project root checks compare whole path components

- Upstream: `Project.Knows` and the reusable workflow path conversion use a string prefix test, so a project at `/work/repo` claims files in `/work/repo-other/` (`project.go:66`, `reusable_workflow.go:244`).
- workflint: paths are compared by whole components.
- Why: files from a sibling directory got the wrong config and caches.
- Upstream issue: #750.
- Milestone: M5.
- Status: planned.

<a id="project-root-detection"></a>
### Project root without `.git`

- Upstream: a project root needs `.github/workflows` and a `.git` file or directory. Without `.git`, linting with no arguments fails with `no project was found in any parent directories of ...` and exit code 3.
- workflint: `.jj` and `.hg` also mark a root. If no ancestor has a version control marker, the nearest ancestor with a `.github` directory is the root (ADR 0010).
- Why: repositories in other version control systems, and exported source trees, should lint.
- Upstream issue: #242, PR #752.
- Milestone: M5.
- Status: planned.

<a id="config-file-lookup"></a>
### Config file names and lookup order

- Upstream: the config is `-config-file`, else `.github/actionlint.yaml`, else `.github/actionlint.yml`. `-init-config` writes `.github/actionlint.yaml`.
- workflint: the order is `-config-file` or `--config`, then `.github/workflint.yaml` and `.yml`, then `.github/actionlint.yaml` and `.yml`, then the user config in `$XDG_CONFIG_HOME/workflint/config.yaml`. The first file found is used without merging. `--no-config` skips them all. `-init-config` writes `.github/workflint.yaml` (ADR 0010).
- Why: workflint adds settings that actionlint does not understand, so they need their own file. Existing actionlint configs keep working.
- Upstream issue: #675.
- Milestone: M5.
- Status: planned.

<a id="config-unknown-keys"></a>
### Unknown config keys are reported

- Upstream: unknown keys are ignored, so a typo such as `self-hosted-runners` silently does nothing.
- workflint: an unknown key in an actionlint config file prints a warning. An unknown key in a workflint config file is a fatal error with exit code 3 (ADR 0010).
- Why: silent typos lead to wrong results.
- Upstream issue: #554.
- Milestone: M5.
- Status: planned.

<a id="config-ignore-entries"></a>
### Non-string `ignore` entries are rejected

- Upstream: entries of `paths.<glob>.ignore` are not checked to be strings. `ignore: [{}]` compiles the empty regex, which matches every message and silences everything.
- workflint: a non-string entry is a config error, exit code 3.
- Why: a malformed config should not hide all errors.
- Upstream issue: #749.
- Milestone: M5.
- Status: planned.

<a id="config-error-messages"></a>
### Config error messages

- Upstream: errors in a repository config are wrapped twice, `could not parse config file "x": could not parse config file "x": ...`. An invalid ignore regex reports `... at line%d,col:%d`, missing the colon after `line`, and gives the position of the list rather than the entry.
- workflint: the prefix appears once, the position reads `line:%d,col:%d`, and it points at the entry.
- Why: the upstream messages contain a typo and repeated text.
- Upstream issue: none.
- Milestone: M5.
- Status: planned.

## Command line and output

<a id="verbose-and-debug"></a>
### `-verbose -debug` enables debug output

- Upstream: when both flags are given, only verbose output is enabled (`linter.go:120-124`).
- workflint: debug output is enabled, which includes verbose output.
- Why: the more detailed level should win.
- Upstream issue: none.
- Milestone: M5.
- Status: planned.

<a id="color-flag-precedence"></a>
### `-color` overrides `NO_COLOR`

- Upstream: a non-empty `NO_COLOR` disables color even when `-color` is given, because the color library reads the variable at startup.
- workflint: `-color` forces color. `-no-color` still wins over `-color`. Without either flag, `NO_COLOR` disables color as before.
- Why: no-color.org says a command-line flag overrides the variable.
- Upstream issue: none.
- Milestone: M5.
- Status: planned.

<a id="template-execution-errors"></a>
### `-format` execution errors exit with code 3

- Upstream: when a template fails at run time, for example `-format '{{.Foo}}'`, the error is ignored for a single file or stdin, which prints nothing and exits with 0 or 1. With two or more files the same error exits with code 3.
- workflint: a template execution error prints the error to stderr and exits with code 3 in every mode.
- Why: a broken template should not look like a clean run.
- Upstream issue: none.
- Milestone: M5.
- Status: planned.

<a id="log-interleaving"></a>
### Log lines do not interleave

- Upstream: the `verbose: ` prefix and the message are separate writes without a lock, so concurrent files produce lines such as `verbose: verbose: Found 1 parse errors`.
- workflint: each log line is written whole.
- Why: interleaved prefixes make the log hard to read.
- Upstream issue: none.
- Milestone: M5.
- Status: planned.

## External tools

<a id="tool-failures"></a>
### A tool failure does not abort the run

- Upstream: if shellcheck or pyflakes exits with an error and no output, is killed by a signal, prints invalid JSON, or prints output without a final newline, the whole run stops with exit code 3 and prints no lint results.
- workflint: the failure becomes a diagnostic on that script, with rule ID `shellcheck-failed` or `pyflakes-failed`, and the other scripts and files are still linted (ADR 0012). The run exits with code 1 because a diagnostic was reported.
- Why: one bad script should not hide every other result.
- Upstream issue: none.
- Milestone: M5.
- Status: planned. The rule IDs are proposals.

<a id="missing-tools"></a>
### Missing tools are reported

- Upstream: if shellcheck or pyflakes cannot be found, the rule is disabled silently. Only `-verbose` mentions it.
- workflint: if a tool enabled by default is not installed, workflint prints one warning per run to stderr and continues. If the user asked for the tool by flag or config and it is not installed, workflint exits with code 3. `-shellcheck=` and `-pyflakes=` still disable the tools without a message (ADR 0012).
- Why: results silently differed between machines with and without the tools.
- Upstream issue: #189, #347.
- Milestone: M5.
- Status: planned.

<a id="external-process-hangs"></a>
### External processes cannot hang the run

- Upstream: shellcheck runs can deadlock. On the reference commit, a run over the 1,503-file corpus on macOS hung for more than 10 minutes. There is no timeout.
- workflint: stdin is written from its own thread while output is read, each process has a timeout (60 seconds by default), a timed-out script gets a diagnostic, and SIGINT or SIGTERM kills the child processes (ADR 0012). Output of runs that finish is unchanged.
- Why: CI jobs hung for hours.
- Upstream issue: #702, #704, #712.
- Milestone: M5.
- Status: planned.

<a id="python-linter-auto"></a>
### ruff is used for Python scripts when installed

- Upstream: `shell: python` scripts are checked with pyflakes.
- workflint: config `python.linter` takes `auto`, `ruff`, `pyflakes` or `off`. The default, `auto`, uses ruff restricted to the pyflakes rule set if ruff is installed, and otherwise pyflakes. The `-pyflakes` flag keeps working as in actionlint. With ruff, the finding text after `pyflakes reported issue in this script:` comes from ruff and can differ from pyflakes' wording (ADR 0012).
- Why: ruff is faster and more often installed than pyflakes.
- Upstream issue: #439.
- Milestone: M5.
- Status: planned.

## Investigated and kept

These upstream behaviours were reported as bugs or looked suspicious, and workflint keeps them unchanged for now. Changing one needs a new entry above.

- A multi-line `if: |` whose only content is `${{ ... }}` is reported as always true, because the block scalar adds a trailing newline (#517). Upstream documents this in `docs/checks.md#if-cond-constant` as the runner's behaviour, and `examples/if_cond_always_true` and `err/if_cond_edge_cases_trailing_leading_chars` pin it.
- The schedule timezone `UTC` is rejected, although `UTC` is an IANA name. Whether GitHub accepts it is not confirmed.
- Values in `matrix.exclude` are checked against values added by `include` too (#634). How GitHub orders `include` and `exclude` for this check is not confirmed.
- Messages that embed Go library errors, such as `strconv.ParseFloat: parsing "x": invalid syntax` and robfig/cron errors, keep that text.
