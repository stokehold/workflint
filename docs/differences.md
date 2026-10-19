# Differences from actionlint

workflint is compatible with actionlint at commit `011a6d1` (ADR 0003, ADR 0004). This file lists every place where workflint deliberately behaves differently. Any other difference from actionlint is a bug in workflint. An entry that changes the output of a fixture or of the corpus also has records in the conformance suite's allow-list, `tests/conformance/allowed-differences.toml`, keyed by the entry's anchor (ADR 0023). An entry that changes neither has no record.

Rules for this file:

- Add an entry before or in the same pull request as the change. Each allow-list record names the anchor and the fixture.
- Keep anchors stable. Other documents and the allow-list link to them.
- An entry describes behaviour that users can observe: output, exit codes, files read or written. Internal design choices that do not change output belong in the ADRs.
- Status is `planned` until the change is built, `implemented` once `main` has it but no release does, and then `done in <version>`. When only part of an entry is built, the status says which part.
- Tasks of workflint are named by their task ID, such as M4.6. `docs/roadmap.md` links each one to its issue.
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
- Status: implemented in the default and `-oneline` output. The `.Kind` and `.Rule` split for templates is a proposal to confirm in M5.
- Fixtures: every `.out` line differs in the bracket only. The conformance runner maps rule IDs back to kinds before comparing.

<a id="sort-order"></a>
### Order of diagnostics at the same position

- Upstream: diagnostics are sorted by file, line, column, then message text.
- workflint: diagnostics are sorted by file, line, column, rule ID, then message text (ADR 0007). Deduplication is unchanged: equal file, line, column and message collapse into one. The diagnostic kept has the highest severity and, among those, the smallest rule ID, which is the ID shown in brackets.
- Why: the order of findings at one position should not depend on message wording.
- Upstream issue: none.
- Milestone: M1.
- Status: implemented.

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
- Status: implemented in the default and `-oneline` output. The other formats follow in M5.

<a id="positions-inside-scalars"></a>
### Exact positions inside multi-line and quoted strings

- Upstream: the position of an error inside a string, such as an expression or glob error, is the scalar's start plus a byte offset, plus 1 if quoted. In a `run: |` block every error is reported on the indicator line. Escapes such as `\t` or `''` shift the column by one per escape. A bare `if: "foo"` ignores the quote and reports column 13 instead of 14. A `TODO` at `rule_expression.go:736` notes the problem.
- workflint: errors are mapped through the scalar's source map (ADR 0005) to the exact line and column. The maps are exact in every scalar style, including escapes, `''`, line folding, and block indentation. A character that an escape produces maps to the escape's backslash. An error at the end of a value, such as an unclosed `${{` at the end of a `run: |` block, is reported just after the last character before the trailing line breaks. With `echo ${{ github.ref` as the last line of a `|` block, on line 8 and indented 10 spaces, workflint reports `8:30`. Upstream reports `8:22`, or `9:22` with `|+`. A glob error about the whole pattern, such as white space around a path, is reported at the pattern's first character. Upstream reports it there too for a plain or quoted scalar, but at the `|` or `>` for a block scalar, the line above.
- Why: a position on the wrong line sends users to the wrong place.
- Upstream issue: #330.
- Milestone: M2, M3, M4. Expression syntax errors use the exact positions from M2.
- Status: implemented for expression syntax errors from M2, for the other errors of the `expression` rule from M4.6, and for the `glob` rule from M4.15. Other errors inside strings follow with the M4 rules that report them.
- Fixtures: `.out` lines for errors inside multi-line or escaped strings, such as line 11 of `err/github_script_untrusted_input`, which workflint reports on line 16, and the `glob` error at `20:9` of `err/glob_more`, which workflint reports at `21:9`.

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

- Upstream: YAML syntax errors use go-yaml's position: the line and column of libyaml's context mark when the error has one, such as the start of the enclosing mapping, and of the problem mark otherwise, with a 0-based column. `steps: -` is reported at `2:7`, although the problem is at `2:8`. An unclosed quoted scalar, as in `runs-on: "ubuntu` on line 4, is reported at its opening quote, `4:13`. A control character in the file gives `control characters are not allowed` at `0:0`, because libyaml's reader records only a byte offset. Bytes that do not decode as UTF-8, or as UTF-16 after a UTF-16 byte order mark, give an encoding error such as `invalid leading UTF-8 octet` at `0:0` too. go-yaml decodes the file in chunks while it parses, so when the bad bytes are in a later chunk than a syntax error, it reports the syntax error instead.
- workflint: the error is reported at the 1-based problem position, a control character at its own position, and an encoding error at the first character that does not decode. An unclosed quoted scalar is the exception: its problem position is the end of the file or the next document marker, so workflint reports it at the opening quote too, at `4:14`, with the snippet of that line. workflint decodes the whole file before it parses, so it always reports the encoding error. The message text is unchanged.
- Why: the upstream position is off by one column and sometimes on the wrong line.
- Upstream issue: #519.
- Milestone: M1.
- Status: implemented.
- Fixtures: `err/yaml_syntax_error`, `examples/broken_yaml`.

<a id="empty-scalar-past-last-line"></a>
### Empty value after the last line of a file without a final line break

- Upstream: go-yaml ends the stream on a new line, so an empty scalar at the end of a file without a final line break is on the line after the last one. A file that holds only `---` has its empty document at `2:1`, and a file that holds only `? ` has the empty value of its key at `2:1`.
- workflint: positions are byte offsets into the file, and no offset lies on a line after the last one. The empty document is at `1:4`, after the `---`, and the empty value is at `1:3`. With a final line break, both tools report `2:1`.
- Why: a position must point into the file.
- Upstream issue: none.
- Milestone: M1.
- Status: implemented. No fixture covers it. `cargo xtask yaml-parity` allows the difference for `tests/yaml-parity/document-start-at-eof.yaml`.

<a id="scanner-error-after-comment-before-dash"></a>
### Scanner error after a comment before a `-` item

- Upstream: when a comment line comes before a `-` item, go-yaml reads the token after the `-` to move the comment, and ignores a scanner error while it reads. The scanner then goes on after the token that failed, and the parser reports a later error. In `#\n-\n,1\n|x\n`, the `|x` is not a valid block scalar header, and upstream reports `did not find expected node content` at `2:0` instead.
- workflint: the first error stops the parse, so the same file reports `did not find expected comment or line break` at `4:2`.
- Why: the ignored error is a go-yaml bug, and both tools reject the file.
- Upstream issue: none.
- Milestone: M1.
- Status: implemented. No fixture covers it. `cargo xtask yaml-parity` allows the difference for `tests/yaml-parity/scanner-error-after-comment-before-dash.yaml`.

<a id="utf16-control-characters"></a>
### Control characters in UTF-16 files

- Upstream: go-yaml checks each 512-byte chunk of the file for control characters when it decodes the chunk, and in a UTF-16 file it counts UTF-16 bytes. A syntax error in an earlier chunk comes first. A UTF-16LE file with `a: 1\nb\nc: `, 290 `x`, and U+0001 has the control character in its second chunk, so upstream reports `could not find expected ':'` at `2:0`.
- workflint: the file is decoded to UTF-8 before it is parsed, and the chunks count UTF-8 bytes. The same file reports `control characters are not allowed` at the U+0001, `3:294`.
- Why: a chunk boundary is not something a user can see, and both tools reject the file. UTF-8 files get the same message as upstream.
- Upstream issue: none.
- Milestone: M1.
- Status: implemented. No fixture covers it. `cargo xtask yaml-parity` allows the difference for `tests/yaml-parity/utf16le-control-character-in-second-chunk.yaml`.

<a id="utf16-second-bom"></a>
### A second byte order mark in UTF-16 files

- Upstream: when the text of a UTF-16 file starts with a second U+FEFF after its byte order mark, go-yaml drops the first character of every line after the first. The UTF-16LE file `a: 1\nbb: 2\n` with two byte order marks has the keys `a` at `1:2` and `b` at `2:2`.
- workflint: the second U+FEFF is skipped as a byte order mark, and no character is dropped. The keys are `a` at `1:1` and `bb` at `2:1`.
- Why: upstream loses characters of the file. In a UTF-8 file with two byte order marks, workflint matches upstream, whose reader sees the same bytes.
- Upstream issue: none.
- Milestone: M1.
- Status: implemented. No fixture covers it. `cargo xtask yaml-parity` allows the difference for `tests/yaml-parity/utf16le-second-byte-order-mark.yaml`.

<a id="unknown-anchor-position"></a>
### Undefined alias position and message prefix

- Upstream: an alias to an undefined anchor is reported at `0:0` as `could not parse as YAML: yaml: unknown anchor 'X' referenced`, with go-yaml's `yaml:` prefix repeated inside the message.
- workflint: the error is reported at the alias as `could not parse as YAML: unknown anchor 'X' referenced`. As upstream, no rules run on that file.
- Why: `0:0` is not a position, and other YAML errors drop the inner prefix.
- Upstream issue: none.
- Milestone: M1.
- Status: implemented.
- Fixtures: `err/undefined_anchor`, `examples/dangling_alias`.

<a id="long-line-snippets"></a>
### Snippets for lines over 64 KiB

- Upstream: the source line for the snippet is read with Go's `bufio.Scanner`, which fails on lines longer than 64 KiB. The error then prints without a snippet.
- workflint: the snippet is printed whatever the line length.
- Why: the limit is an artefact of the Go reader.
- Upstream issue: none.
- Milestone: M1.
- Status: implemented.

<a id="snippet-line-breaks"></a>
### Snippet lines end at every YAML line break

- Upstream: the YAML parser counts a lone `\r`, U+0085, U+2028, and U+2029 as line breaks, so the line number in the header counts them. The snippet reader splits lines at `\n` only. In a file with one of these breaks, the snippet shows a different line than the header names, or no snippet at all.
- workflint: the snippet shows the line the header names. It ends at the same line breaks the YAML parser counts.
- Why: the snippet must show the line the diagnostic is on.
- Upstream issue: none.
- Milestone: M1.
- Status: implemented.

<a id="local-action-paths"></a>
### Paths of local actions in messages

- Upstream: messages about a local action's metadata name its directory or its `action.yml` as the project root joined with the `uses:` path. The CLI finds the project root itself, so the path is absolute. The Go test of `projects/local_action_invalid` passes the relative root `testdata/projects/local_action_invalid`, and its `.out` file expects paths that start with it. The reference binary fails that fixture too.
- workflint: prints the absolute path, as the reference binary does. A path from the `runs` section, as in `file "dist/index.js" does not exist`, is printed as `action.yml` writes it, with `/`, where the reference on Windows prints `dist\index.js`. Whether a local Docker `image` is named `Dockerfile` is decided by the name after the last `/`, where the reference on Windows also splits at `\`.
- Why: the absolute path is no difference from the reference binary, and the entry exists so that the allow-list can replace the fixture lines that only the Go test prints. A path from `action.yml` must print the same on every host (ADR 0013). GitHub builds Docker actions on Linux runners, where `\` is part of a file name.
- Upstream issue: none.
- Milestone: M4.18.
- Status: implemented.
- Fixtures: `projects/local_action_invalid`.

## Workflow parser

<a id="nan-panic"></a>
### No panic on NaN

- Upstream: `timeout-minutes: !!float NaN` makes `strconv.ParseFloat` succeed with NaN, and the error path then calls `Error()` on a nil error, which crashes actionlint (`parse.go:305-306`).
- workflint: the value is rejected with the same message as other invalid floats, `invalid float value: "NaN": strconv.ParseFloat: parsing "NaN": invalid syntax`. More generally, no input makes workflint panic. If a rule panics anyway, workflint reports an `internal-error` diagnostic for that file, lints the other files, and exits with code 3 (ADR 0007).
- Why: a crash loses every other result.
- Upstream issue: #753.
- Milestone: M2.
- Status: implemented in the parser. The `internal-error` diagnostic for a rule that panics is planned for M5.

<a id="yaml-node-limit"></a>
### Workflows over GitHub's YAML node limit

- Upstream: the parser walks an anchored subtree again at each alias that uses it. Nested aliases multiply, so a "billion laughs" workflow, nine levels of anchors with ten aliases each, makes the parser walk 10^9 nodes, and actionlint does not finish.
- workflint: before it parses, the parser counts the YAML events that GitHub's reader reads, with each alias expanded: the stream and document markers, each scalar, and the start and end of each collection. The reader is `YamlObjectReader` in `actions/runner` at `9abfe11`, and it stops past 50,000 events with `Maximum YAML nodes exceeded`. It counts only when its parse options allow anchors (`m_allowAnchors` in `MoveNext`). Without them, it rejects every anchor and counts nothing. GitHub accepts anchors in workflows, so workflint counts in every workflow. Past the same limit, workflint reports `workflow has more than 50000 YAML nodes when its aliases are expanded. GitHub rejects it with "Maximum YAML nodes exceeded"` at the start of the document, with the rule `too-many-yaml-nodes`, and parses nothing else. The count takes time in proportion to the size of the file. A recursive alias counts as one event, because the YAML layer reports it already.
- Why: a linter must finish on any input, and GitHub rejects these files anyway.
- Upstream issue: none.
- Milestone: M2.
- Status: implemented. No fixture covers it.

<a id="container-volumes"></a>
### Container `volumes` is kept separate from `ports`

- Upstream: the parser stores `volumes` in the `Ports` field (`parse.go:1065-1066`). When both keys exist, the later one replaces the earlier, so expressions in the other list are never checked.
- workflint: `ports` and `volumes` are parsed and checked separately.
- Why: the field assignment is a copy-paste mistake.
- Upstream issue: #747.
- Milestone: M2.
- Status: implemented. The parser keeps both lists, and the `expression` rule checks the expressions in both from M4.6.
- Fixtures: `ok/container_syntax.yaml` (lines 16-24), `err/empty_sequence_or_string` (lines 25-30), and `err/invalid_container_syntax` (lines 51-54) have both keys. Their output does not change, because neither list holds an expression. The test `every_location_reports_a_context_or_function_that_its_workflow_key_does_not_allow` in `crates/workflint-rules/tests/expression.rs` has an expression in each list.

<a id="capitalized-booleans"></a>
### `True` and `TRUE` are true

- Upstream: YAML resolves `True` and `TRUE` as booleans, but the parser compares the text with `"true"` case-sensitively, so they become false (`parse.go:253`). `required: True` on a `workflow_call` input is treated as not required, which hides missing-input errors.
- workflint: `true`, `True` and `TRUE` are true, and `false`, `False` and `FALSE` are false, as YAML defines.
- Why: the value contradicts the YAML type actionlint itself assigned.
- Upstream issue: none.
- Milestone: M2.
- Status: implemented.

<a id="reusable-call-services"></a>
### `services` on a reusable workflow call job

- Upstream: a job with `uses:` may not have `runs-on`, `steps`, `container` and other step-job keys, but `services` is missing from that list (`parse.go:1396`) and is accepted.
- workflint: `services` gets the same error as the other keys, `when a reusable workflow is called with "uses", "services" is not available. ...`. Unlike the other keys, it does not make the job lose its call: the expressions in `with` and `secrets` are still checked, as upstream checks them.
- Why: GitHub does not allow `services` on a job that calls a reusable workflow. The upstream message itself lists the allowed keys, and `services` is not among them.
- Upstream issue: #748.
- Milestone: M2.
- Status: implemented.
- Fixtures: `workflint/err/reusable_call_keys`.

<a id="reusable-call-keys"></a>
### Every misplaced key of a reusable workflow call job is reported

- Upstream: a job with `uses` reports only the last key written that only a job with steps may have, such as `runs-on` or `env` (`parse.go:1461`). A job without `uses` reports only the last of `with` and `secrets` (`parse.go:1479`). After fixing that key, the next run reports the one before it.
- workflint: each such key gets its own error, at its key.
- Why: one run should show every key to remove. No fixture of the reference and no corpus file has a job with two such keys, so no expected output of the reference changes.
- Upstream issue: none.
- Milestone: M2.
- Status: implemented.
- Fixtures: `workflint/err/reusable_call_keys`.

<a id="snapshot-section-name"></a>
### `snapshot` errors name the right section

- Upstream: the `snapshot` mapping is parsed with the section name `"on"` (`parse.go:1305`), so an error such as an empty mapping says `"on" section`.
- workflint: such errors say `"snapshot" section`.
- Why: the section name is a copy-paste mistake.
- Upstream issue: none.
- Milestone: M2.
- Status: implemented.
- Fixtures: `workflint/err/snapshot_section_name`.

<a id="dispatch-input-key-list"></a>
### Complete key list for `workflow_dispatch` inputs

- Upstream: an unknown key in a `workflow_dispatch` input lists only `"default", "description", "required"` as expected keys (`parse.go:444`), although `type` and `options` are also accepted.
- workflint: the message lists `"default", "description", "options", "required", "type"`.
- Why: the hint should list every valid key.
- Upstream issue: none.
- Milestone: M2.
- Status: implemented.
- Fixtures: `err/case_sensitive_keys`, `err/unexpected_keys`.
- Corpus: `netdata_netdata_.github_workflows_dashboard-pr.yml`, `netdata_netdata_.github_workflows_docker.yml`, `netdata_netdata_.github_workflows_packaging.yml`. The allow-list has no records for them, because `cargo xtask oracle` cannot apply records to corpus files yet (#49).

<a id="empty-event-name-section"></a>
### Section name of an event with an empty name

- Upstream: when the body of an event with an empty name is not a mapping, the message names the section `%q section`. An event key that is not a scalar also has the empty name. The parser formats the section name only when the event name is not empty (`parse.go:60-66`), so the format verb stays in the message.
- workflint: the message names the section `"" section`, as upstream's unexpected-key messages for the same event already do.
- Why: `%q` is an unformatted verb, not a section name.
- Upstream issue: none.
- Milestone: M2.
- Status: implemented.
- Fixtures: `workflint/err/empty_event_name`.

<a id="action-metadata-aliases"></a>
### Aliases and merge keys in `action.yml` inputs

- Upstream: when an input in a local `action.yml` is an alias (`bar: *input`), the scan for `deprecationMessage` and for unknown keys sees nothing, so a deprecated aliased input is not reported. An alias used as an input name (`*name: {}`) gives an input named after the anchor. A `<<` merge key under `inputs` becomes an input named `<<`.
- workflint: aliases under `inputs` are resolved before inputs are read, both as input names and as input definitions. A merge key under `inputs` merges the referenced inputs, as go-yaml does for the rest of the file: the inputs written under `inputs` come first, and a merged input does not replace one with the same name. With a sequence of mappings, earlier mappings win. A merged input whose name differs from another only in case is a duplicate. A merge value that is not a mapping or a sequence of mappings, or that contains itself, is an error with go-yaml's text. A `<<` key inside one input's definition is still an unknown key.
- Why: an alias should behave like the node it refers to.
- Upstream issue: none.
- Milestone: M2.
- Status: implemented in the decoder, and the `action` rule reads the decoded inputs since M4.18.

<a id="non-scalar-keys"></a>
### Keys that are not strings in `permissions` and `env`

- Upstream: a key of `permissions` or `env` that is a sequence or a mapping, such as `? [x]`, gets `expected scalar node for string value but found sequence node with "!!seq" tag`, and the parser keeps the entry under the name `""`. The `permissions` rule then also reports `unknown permission scope ""` at the key.
- workflint: the parser reports the key and leaves the entry out, so the `permissions` rule does not report it again. In `env` the output does not change, because no rule reports an empty variable name.
- Why: the second error repeats the first and names a scope that is not in the file.
- Upstream issue: none.
- Milestone: M2.
- Status: implemented in the parser and the `permissions` rule (M4.22).
- Fixtures: `workflint/err/invalid_value_reported_once`.

<a id="invalid-value-reported-once"></a>
### An invalid value is reported once

- Upstream: the parser reports a value that is not a valid string and keeps it as the string `""`. The rules then check that string as if the file had it, and these cases get a second error:
  - An activity type that is empty, null, or not a scalar, as in `types: [[x]]`, `types: ['']`, `types: ''`, or `types:`, also gets `invalid activity type "" for "pull_request" Webhook event` (`events`).
  - An event key that is not a scalar, as in `[a]: x` under `on`, also gets `unknown Webhook event ""` (`events`).
  - A `default` of a `workflow_call` or `workflow_dispatch` input that is not a scalar, as in `default: [x]`, is checked as `""`. It gets `cannot be parsed as a float number` for a `number` input, `must be "true" or "false"` for a `boolean` input, `is not included in its options` for a `choice` input, and `has the default value "", but it is also required` for a required `workflow_call` input (`events`).
  - A `shell` of `defaults.run` or of a step that is empty or not a scalar also gets `shell name "" is invalid` (`shell-name`).
  - A `runs-on` label that is empty or not a scalar also gets `label "" is unknown` (`runner-label`).
  - A step's `uses` that is empty or not a scalar also gets `specifying action "" in invalid format because ref is missing` (`action`).
  - A `permissions` scope whose value is empty or not a scalar, as in `contents:` or `contents: [read]`, also gets `"" is invalid as permission of scope "contents"` (`permissions`).
- workflint: the parser reports the value once, and the rules skip it. The value keeps its place in its list, so a check that only needs the value to be written still runs. `options: [[x]]` on a `string` input of `workflow_dispatch` still gets `"options" can not be set to "text" input because its input type is not "choice"`, and `options: ['']` on a `choice` input counts as set, as upstream. A `choice` input with an invalid option does not compare its default with the options, because the invalid option has no value to compare. Upstream compares the default with `""` in its place. A required `workflow_call` input with an invalid default is not reported either, although that check only needs the default to be written, because its message would quote `""`. The keys of `permissions` and `env` are in [Keys that are not strings in `permissions` and `env`](#non-scalar-keys). The string form of `permissions` keeps the upstream output: `permissions:` with no value gets `"" is invalid for permission for all the scopes` after `string should not be empty`, because `err/issue170_empty_permissions` expects both.
- Why: the second error repeats the first and names a value that is not in the file.
- Upstream issue: none.
- Milestone: M2.
- Status: implemented in the parser, the `events` rule (M4.13), the `runner-label` rule (M4.16), the `action` rule (M4.17), the `permissions` rule (M4.22), and the `shell-name` rule (M4.23).
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
- Status: implemented. No fixture covers it.

## Expressions

<a id="int-literal-range"></a>
### Integer literals beyond 32 bits

- Upstream: integer literals are parsed as 32-bit integers, so `${{ 2147483648 }}` is an error: `parsing invalid integer literal "2147483648": strconv.ParseInt: parsing "2147483648": value out of range`.
- workflint: integer literals are accepted in the range of a double, as the runner evaluates numbers as doubles. Decimal and `0x` hexadecimal literals become the nearest double, so `9007199254740993` is `9007199254740992`. A literal beyond the largest double, such as a 1 followed by 309 zeros, is still an error, with upstream's message: `parsing invalid integer literal "...": strconv.ParseInt: parsing "...": value out of range`.
- Why: upstream rejects valid expressions.
- Upstream issue: PR #733.
- Milestone: M3.
- Status: implemented.

<a id="exponent-plus-sign"></a>
### `+` in a number's exponent

- Upstream: `${{ 1e+5 }}` is a lexer error, because only `-` is accepted after `e`.
- workflint: `1e+5` is a valid number, as in JSON.
- Why: upstream rejects valid expressions.
- Upstream issue: PR #708.
- Milestone: M3.
- Status: implemented.

<a id="comparison-precedence"></a>
### Comparison operator precedence

- Upstream: all six comparison operators share one precedence level and associate to the right. `'a' < 'b' == true` parses as `'a' < ('b' == true)` and reports `"string" value cannot be compared to "bool" value with "<" operator`.
- workflint: `<`, `<=`, `>` and `>=` bind tighter than `==` and `!=`, and all associate to the left, following GitHub's documented operator order. `'a' < 'b' == true` parses as `('a' < 'b') == true`. A chain of `&&` or `||` is one node with all its operands, as in GitHub's parser. Upstream nests it to the right, which gives the same values, types, and errors.
- Confirmed against the expression parser in `actions/languageservices` (`expressions/src/parser.ts`) at commit `4043eda`. From the loosest: `||`, then `&&`, then `==` and `!=`, then `<`, `<=`, `>` and `>=`, then `!`, then property access and index access. The comparison and equality levels are left-associative loops. The `&&` and `||` levels collect any number of operands into one node.
- Why: the tree must match how GitHub evaluates the expression, or type errors are wrong.
- Upstream issue: none.
- Milestone: M3.
- Status: implemented in `workflint-expr`. Its type errors reach the output from M4.6.

<a id="expression-nesting-limit"></a>
### Nesting limit for expressions

- Upstream: expressions can nest without limit.
- workflint: an expression nested more than 128 levels deep is an error, `expression is nested more than 128 levels deep`, reported by `invalid-expression-syntax`. The depth is the most levels on a path from the root of the tree to a leaf. Each parenthesis adds a level, and so do `!`, a function call, a property access, an object filter, an index access, and a comparison. A chain of `&&` or `||` of any length adds one level, so a condition with hundreds of `a == b ||` alternatives is accepted. `a.b.c[x.y]` is 3 levels deep: the index access is one level above the deeper of `a.b.c` and `x.y`.
- Why: the parser and the checks walk the tree recursively, and a limit keeps a crafted expression from overflowing the stack. The parser reads a chain of `&&` or `||` in a loop, so a long chain needs no limit. The deepest expression in the corpus is 11 levels deep. GitHub's parser in `actions/languageservices` stops at 50 levels.
- Upstream issue: none.
- Milestone: M3.
- Status: implemented.

<a id="bom-before-string-literal"></a>
### Byte order mark before a string literal

- Upstream: when an expression starts with a byte order mark, the lexer keeps the mark in the first token. If that token is a string literal, the parser cuts the first byte and the last byte off the token, so `${{ 'x' }}` after a byte order mark has the value `"\xbb\xbf'x"`: the last two bytes of the mark, the opening quote, and `x`.
- workflint: the parser drops the byte order mark before it removes the quotes, so the value is `"x"`.
- Why: upstream's value is not valid UTF-8 and keeps a quote.
- Upstream issue: none.
- Milestone: M3.
- Status: implemented.

<a id="bool-assignability"></a>
### `bool` no longer accepts every type

- Upstream: any type is assignable to `bool` (`expr_type.go:123-128`). As a result, a string or number passed in `with:` to a `boolean` input of a local reusable workflow is never reported, and the type checks on `!`, `if:` and the `case()` predicate can never fire.
- workflint: only `bool` and `any` are assignable to `bool` in assignments, which covers `with:` values for `boolean` inputs and function parameters. Where GitHub converts values to booleans, in `if:`, `!`, `&&`, `||` and the `case()` predicate, every type is still accepted.
- Why: GitHub rejects a non-boolean value for a `boolean` input of a reusable workflow at run time.
- Upstream issue: none.
- Milestone: M3, M4.
- Status: implemented in `workflint-expr`, and in the output from M4.6: `if:` and `!` accept every type (`workflint/ok/if_condition_any_type.yaml`). The `with:` check is implemented since M4.10, and adds a line to `projects/workflow_call_input_type_check` for a string passed to a `boolean` input.

<a id="literal-index-case"></a>
### Case-insensitive literal index keys

- Upstream: property access with a dot is case-insensitive, but an index with a string literal is not. `steps['MyStep']`, `matrix['OS']` and `github['EVENT']` report `property ... is not defined`. The untrusted-input check misses `github['EVENT']['ISSUE']['TITLE']`.
- workflint: literal index keys are compared case-insensitively, like dot access, in type checking and in the untrusted-input check. Messages keep the original spelling.
- Why: GitHub treats both forms the same.
- Upstream issue: none.
- Milestone: M3.
- Status: implemented in `workflint-expr`. It reaches the output from M4.6.

<a id="fromjson-key-case"></a>
### Case-insensitive keys from `fromJSON` literals

- Upstream: object keys in a `fromJSON('...')` literal keep their case while property names are lowercased, so `fromJSON('{"Foo":1}').Foo` reports `property "foo" is not defined in object type {Foo: number}`.
- workflint: keys from `fromJSON` literals are matched case-insensitively. They keep their spelling in printed types, so the object above prints as `{Foo: number}`. If two keys differ only in case, the key spelled exactly like the property wins, so `.foo` reads `foo` and `['Foo']` reads `Foo`.
- Why: upstream rejects valid code. Property access is case-insensitive at run time.
- Upstream issue: none.
- Milestone: M3.
- Status: implemented in `workflint-expr`. It reaches the output from M4.6.

<a id="fromjson-nesting"></a>
### Deeply nested `fromJSON` literals

- Upstream: the type of a `fromJSON('...')` literal follows its value at any depth up to the 10,000 levels that Go's `encoding/json` accepts.
- workflint: a value nested more than 128 levels deep has type `any`. The syntax check still covers the whole literal, with Go's limit and message.
- Why: printing, merging, and comparing types recurse, and a type 10,000 levels deep could overflow the stack. The expression parser has the same limit of 128 levels.
- Upstream issue: none.
- Milestone: M3.
- Status: implemented in `workflint-expr`. It reaches the output from M4.6. No fixture covers it.

<a id="matrix-value-nesting"></a>
### Deeply nested matrix values

- Upstream: a matrix value gets a type at any depth, up to the 10,000 levels that the YAML parser accepts. The typing recurses, as does every later walk of the type.
- workflint: an object or array in a matrix value nested more than 128 levels deep has type `any`, as for `fromJSON()` literals ([Deeply nested `fromJSON` literals](#fromjson-nesting)). The rule still checks every `${{ }}` in the value, at any depth.
- Why: printing, merging, and comparing types recurse, and a type 10,000 levels deep could overflow the stack (ADR 0018).
- Upstream issue: none.
- Milestone: M4.
- Status: implemented. No fixture covers it.

<a id="object-filter-mutation"></a>
### Object filters do not change shared types

- Upstream: typing `x.*` marks the array type of `x` as filtered in place (`expr_sema.go:721`). The type is shared for the whole job, so after `${{ matrix.os.* }}` a later `${{ matrix.os.a }}` is wrongly accepted.
- workflint: types are immutable. Each expression is checked against the original types.
- Why: the result of one expression must not depend on an earlier one.
- Upstream issue: none.
- Milestone: M3.
- Status: implemented in `workflint-expr`. It reaches the output from M4.6.

<a id="object-merge-order"></a>
### Merged object types do not depend on map order

- Upstream: merging two object types adds each property that only the second one has to the element type of the result, in Go map order, which is random (`expr_type.go:343-351`). The merged element type depends on that order, so the result can vary between runs. A map of `number` merged with a strict object `{a: string; b: bool}` prints as `{string => string}` or as `object`.
- workflint: the properties are added in byte order of their names, so that merge always gives `{string => string}`.
- Why: output must not vary between runs (ADR 0013).
- Upstream issue: none.
- Milestone: M3.
- Status: implemented in `workflint-expr`. It reaches the output from M4.6.

<a id="matrix-include-expressions"></a>
### Expressions in `matrix.include` elements are checked

- Upstream: when an element of `include:` is a whole `${{ }}` expression, the rule checks `m.Include.Expression`, which is always nil on that path, instead of the element (`rule_expression.go:950`). `include: ["${{ undefined }}"]` gives no error, and the element's type is not merged into the matrix type, which causes false `property ... is not defined` errors later.
- workflint: each element expression is checked, and its object type is merged into `matrix`. An element of another type is reported as `type of expression at "matrix combination at element of include section" must be object but found type ...`, and `matrix` then accepts any property. A syntax error in an element expression is reported from M2.
- Why: the upstream code reads the wrong variable.
- Upstream issue: none.
- Milestone: M2, M4.
- Status: implemented. No fixture covers it.

<a id="all-expressions-in-string"></a>
### Every `${{ }}` in a string is checked

- Upstream: the first `${{ }}` with any error, including an untrusted-input error, stops checking the rest of the string (`rule_expression.go:755-757`). `echo ${{ foo }} ${{ bar }}` reports only `foo`.
- workflint: every expression in the string is checked and reported. After a syntax error, the search for the next `${{` goes on after the `}}` that ends the broken expression, or after the character that the lexer rejects if it finds no `}}`. Syntax errors are reported from M2, and type and context errors from M4.
- Why: users fix one error, rerun, and find the next one.
- Upstream issue: none.
- Milestone: M2, M4.
- Status: implemented. Syntax errors from M2, and type, context, and untrusted-input errors from M4.6.
- Fixtures: `err/context_availability` (line 106, where upstream reports only the first of two errors).

<a id="invalid-if-condition"></a>
### An empty or non-string `if`

- Upstream: the parser keeps an `if` whose value is empty or not a scalar as an empty string after it reports the value (`parse.go:204-209`). The `expression` rule then parses that empty string as a condition and reports a second error at the same position: `unexpected end of input while parsing variable access, function call, null, bool, int, float or string. expecting "IDENT", "(", "INTEGER", "FLOAT", "STRING"`.
- workflint: only the parser's error is reported, such as `string should not be empty`.
- Why: the second error is about the empty string that upstream puts in place of the value, not about the file.
- Upstream issue: none.
- Milestone: M2.
- Status: implemented in M4.6: the `expression` rule skips a value that the parser marks invalid. No fixture covers it. The test `an_invalid_value_gets_no_diagnostic_from_the_rule` in `crates/workflint-rules/tests/expression.rs` does.

<a id="jobs-context-result"></a>
### `jobs.<job_id>.result` in reusable workflow outputs

- Upstream: the `jobs` context in `on.workflow_call.outputs.<id>.value` has only `outputs`, so `${{ jobs.build.result }}` reports `property "result" is not defined`.
- workflint: each job has `outputs` and `result`.
- Why: upstream rejects valid code. GitHub documents `jobs.<job_id>.result`.
- Upstream issue: #343, PR #724.
- Milestone: M4.
- Status: implemented in M4.7. No fixture reads `jobs.<job_id>.result`, and the reference's fixtures keep their output, because a message about a missing output prints only the type of `outputs`. The test `jobs_in_the_workflow_call_outputs_has_the_outputs_and_result_of_each_job` in `crates/workflint-rules/tests/expression.rs` covers it.

<a id="secrets-context-mixed-triggers"></a>
### `secrets` in workflows with several triggers

- Upstream: when `on.workflow_call.secrets` exists, `secrets` only has the declared secrets for every job, even when the workflow also runs on `push` or `workflow_dispatch`, where every repository secret is available (`rule_expression.go:152-158`).
- workflint: `secrets` is limited to the declared secrets and the built-in ones only when `workflow_call` is the only trigger and `on.workflow_call.secrets` is written. Otherwise any secret name is accepted.
- Why: upstream rejects valid workflows that have more than one trigger. When `workflow_call` is the only trigger, an undeclared secret stays an error, as upstream reports it, although a caller may pass it with `secrets: inherit` (#148). workflint cannot see the callers, and a typo in a secret name is the more common mistake.
- Upstream issue: #701, PR #703.
- Milestone: M4.
- Status: implemented in M4.7. The two fixtures that declare secrets and have another trigger, `err/unexpected_keys` and `err/case_sensitive_keys`, never read `secrets`, so no fixture output changes. The tests `secrets_has_only_the_declared_and_built_in_secrets_when_workflow_call_is_the_only_trigger` and `secrets_accepts_any_name_when_the_workflow_has_another_trigger` in `crates/workflint-rules/tests/expression.rs` cover it.

<a id="tag-before-flow-comma"></a>
### A tag followed by a comma in a flow collection

- Upstream: go-yaml allows `,`, `[`, and `]` in every tag. In `[!!str, x]` the tag is `!!str,`, and the sequence has one item, `x`.
- workflint: in a flow collection, a tag ends before `,`, `[`, and `]`, as in libyaml and the YAML test suite (case WZ62). `[!!str, x]` has two items: an empty string tagged `!!str`, and `x`. Outside flow collections, a tag takes these characters, as in go-yaml.
- Why: a comma separates entries in a flow collection. go-yaml keeps a rule that libyaml dropped.
- Upstream issue: none.
- Milestone: M1.
- Status: implemented. No fixture covers it. `cargo xtask yaml-parity` allows the difference for `tests/yaml-parity/tag-before-flow-sequence-end.yaml`, where upstream reads `[!!str]` as the tag `!!str]` and fails.

<a id="yaml-solidus-escape"></a>
### The `\/` escape in double-quoted strings

- Upstream: go-yaml rejects the escape `\/` in a double-quoted scalar with `found unknown escape character`, so `run: "echo \/"` does not parse.
- workflint: `\/` is a `/`. YAML 1.2 defines this escape (section 5.7), and libyaml accepts it. Other escapes match go-yaml, including `\'`, which YAML does not define.
- Why: upstream rejects valid YAML. ADR 0022 keeps go-yaml's behaviour except where go-yaml rejects valid YAML.
- Upstream issue: none.
- Milestone: M1.
- Status: implemented. No fixture covers it.

<a id="yaml-1-2-directive"></a>
### The `%YAML 1.2` directive

- Upstream: go-yaml accepts only `%YAML 1.1`. A file that starts with `%YAML 1.2` fails with `found incompatible YAML document`.
- workflint: `%YAML 1.1` and `%YAML 1.2` are accepted. Other versions fail with the same message as upstream.
- Why: upstream rejects valid YAML 1.2 documents, such as the YAML test suite cases 27NA and RTP8. ADR 0022 keeps go-yaml's behaviour except where go-yaml rejects valid YAML.
- Upstream issue: none.
- Milestone: M1.
- Status: implemented. No fixture covers it.

<a id="yaml-core-schema"></a>
### Plain scalars follow the YAML 1.2 core schema

- Upstream: go-yaml resolves some plain scalars with YAML 1.1 rules. `1_000` and `0b10` become integers that later fail Go's number parsing, and dates such as `2026-10-15` get the `!!timestamp` tag, which appears in messages. A plain `<<` gets the `!!merge` tag, so `timeout-minutes: <<` reports a `"!!merge" tag`, and `-0` is a float.
- workflint: plain scalars are typed under the YAML 1.2 core schema (ADR 0005). `1_000`, `0b10`, `2026-10-15`, and `<<` are strings, so a numeric key such as `timeout-minutes: 1_000` reports a string where a number is expected, and no message names a `!!timestamp` or `!!merge` tag. `-0` is an integer.
- Numbers that overflow get other messages too. go-yaml resolves a plain scalar as a number only when Go can parse it, while the core schema types it by its pattern alone. With the value at `timeout-minutes`, `continue-on-error`, and `max-parallel`:
  - `1e400` is a string for go-yaml and a float for workflint. Upstream reports `expecting a single ${{...}} expression or float number literal, but found plain text node` at `timeout-minutes`, `... or boolean literal "true" or "false", but found plain text node` at `continue-on-error`, and `... or integer literal, but found plain text node` at `max-parallel`. workflint reports, at the same keys, `invalid float value: "1e400": strconv.ParseFloat: parsing "1e400": value out of range`, `expected bool value but found scalar node with "!!float" tag`, and `expected scalar node for integer value but found scalar node with "!!float" tag`.
  - `99999999999999999999` is a float for go-yaml and an integer for workflint. Neither reports it at `timeout-minutes`. Upstream reports `expected bool value but found scalar node with "!!float" tag` at `continue-on-error` and `expected scalar node for integer value but found scalar node with "!!float" tag` at `max-parallel`. workflint reports `expected bool value but found scalar node with "!!int" tag` and `invalid integer value: "99999999999999999999": strconv.Atoi: parsing "99999999999999999999": value out of range`.
  - `0x1FFFFFFFFFFFFFFFFF` is a string for go-yaml and an integer for workflint. Upstream reports the three `plain text node` messages above. workflint reports `invalid float value: "0x1FFFFFFFFFFFFFFFFF": strconv.ParseFloat: parsing "0x1FFFFFFFFFFFFFFFFF": invalid syntax`, `expected bool value but found scalar node with "!!int" tag`, and `invalid integer value: "0x1FFFFFFFFFFFFFFFFF": strconv.Atoi: parsing "0x1FFFFFFFFFFFFFFFFF": invalid syntax`.
- Why: GitHub types plain scalars with the 1.2 core schema, so the diagnostic describes what GitHub does with the file. `YamlObjectReader` in `actions/runner` at `9abfe11` (`src/Sdk/WorkflowParser/Conversion/YamlObjectReader.cs`) tries null, boolean, integer, and float in turn, with the core schema's patterns, and reads any other plain scalar as a string. Its patterns match `crates/workflint-yaml/src/resolve.rs`: `True` is a boolean, `0o17` and `0x1F` are integers, `.inf` is a float, and `yes`, `1_000`, and `2026-10-15` are strings. Quoted and block scalars are strings.
- Upstream issue: none.
- Milestone: M1.
- Status: implemented. No fixture that passes today shows it, so the allow-list has no records for it.

<a id="quoted-scalar-types"></a>
### Quoted values are strings

- Upstream: literal values in `with:` and in matrix rows are typed from their decoded text, ignoring quotes. `with: { flag: 'true' }` to a `string` input reports `bool value cannot be assigned`, and the matrix value `"3.10"` is typed as a number. A plain scalar is a bool if it is `true` or `false`, null if it is `null`, a number if Go's `strconv.ParseFloat` accepts it, and a string otherwise.
- workflint: types come from the YAML tag, so quoted scalars, block scalars, and scalars tagged `!!str` are strings. A plain scalar has the tag that the YAML 1.2 core schema resolves ([Plain scalars follow the YAML 1.2 core schema](#yaml-core-schema)), which gives the upstream type in the common cases. Where the two disagree, the core schema wins: `True` is a bool, `~` and an empty value are null, `0x1F`, `0o17`, `.inf`, and `1e400` are numbers, and `inf` and `NaN` are strings. A scalar written as one `${{ }}` has the type of its expression, as upstream.
- Why: upstream rejects valid code. Quoting is how users write a string in YAML.
- Upstream issue: #225, #250, PR #727.
- Milestone: M4.
- Status: implemented for matrix values in M4.8 and for `with:` values in M4.10. `projects/workflow_call_input_type_check` passes empty values to a `string` and a `number` input. Upstream reports a string for the `number` input only. workflint reports null for both.

## Rules

<a id="custom-shell-hint"></a>
### A `shell` that is no shell name says how to run another program

- Upstream: `shell: dash` gets `shell name "dash" is invalid. available names are "bash", "pwsh", "python", "sh"`. The message reads as if only those names work, and users take it for a bug when the program is on the runner's `PATH` (#530).
- workflint: when no platform has the shell, the message adds `. use a command with {0} for a custom shell, as in "dash {0}"`. A shell that another platform has, such as `powershell` on Linux, keeps the upstream message, because a command template does not help there.
- Why: GitHub runs a program that is not a built-in shell only through a command template, where `{0}` stands for the path of the script. The workflow syntax reference says so under `jobs.<job_id>.steps[*].shell` ("Custom shell"). The rule already accepts any value with `{0}`, so the hint names the fix.
- Upstream issue: #530.
- Milestone: M4.23.
- Status: implemented.
- Fixtures: `examples/shell_name_validation` has two changed lines in the allow-list.

<a id="matrix-exclude-before-include"></a>
### `exclude` is matched against the matrix rows only

- Upstream: the `matrix` rule adds the keys and values of `include` to the rows before it checks `exclude` (`rule_matrix.go:158`). An `exclude` entry that names a value or key that only `include` adds passes. When any `include` element is a `${{ }}` expression, the rule skips the `exclude` check. When a matrix has no rows but `include` has entries, `exclude` is not reported.
- workflint: `exclude` is matched against the rows only. A value that only `include` adds is reported as `value ... does not match in matrix ... combinations`, a key that only `include` adds is reported as `... does not exist in matrix`, and the list of available keys and possible values names the rows only. An expression in `include` does not stop the check. A matrix with `exclude` and no rows reports `"exclude" section exists but no matrix variation exists`, with or without `include`.
- Why: GitHub removes the `exclude` combinations from the rows and then adds `include`. The workflow syntax reference says so under `jobs.<job_id>.strategy.matrix.exclude`: "All `include` combinations are processed after `exclude`." An `exclude` entry that only matches `include` values removes nothing.
- Upstream issue: #634.
- Milestone: M4.21.
- Status: implemented.
- Fixtures: `err/matrix_exclude_mismatch` has five changed lines in the allow-list. `workflint/err/matrix_exclude_before_include` and `workflint/err/matrix_exclude_without_matrix` show the change.

<a id="if-cond-closing-braces"></a>
### `}}` inside a string literal in `if:`

- Upstream: the `if-cond` rule finds the end of `${{` with the first `}}` in the text, so `if: ${{ github.ref == '}}' }}` is reported as always true. It also counts every `${{` in the text, so `if: ${{ github.ref == '${{' }}` is reported as always true.
- workflint: the expression lexer finds the end of each `${{ }}` and skips string literals. The rule uses the placeholders that the parser stored. When an expression does not parse, the rule falls back to the first `}}`, as upstream does.
- Why: upstream rejects a valid condition.
- Upstream issue: none.
- Milestone: M4.12.
- Status: implemented. No fixture covers it. The `workflint-rules` test `closing_braces_inside_a_string_literal_do_not_end_the_expression` does.

<a id="if-cond-suggestion"></a>
### Always-true conditions suggest one `${{ }}`

- Upstream: `if: ${{ success() }} || ${{ failure() }}` gets `if: condition "${{ success() }} || ${{ failure() }}" is always evaluated to true because extra characters are around ${{ }}`.
- workflint: the message ends with `. did you mean "${{ success() || failure() }}"?`. The suggestion puts the text around the placeholders and the text inside them into one `${{ }}`. A placeholder gets parentheses when an operator next to it binds tighter than its own, so `${{ a || b }} && ${{ c }}` suggests `${{ (a || b) && c }}`. There is no suggestion when the text around the placeholders is only whitespace, when a placeholder or the joined text does not parse, or when the joined expression is constant, which the `constant-condition` rule would report.
- Why: the joined expression is what the author meant in most cases, and upstream's message does not say how to fix the condition.
- Upstream issue: #367.
- Milestone: M4.12.
- Status: implemented.
- Fixtures: `examples/if_cond_always_true` line 29.
- Corpus: `apache_superset_.github_workflows_latest-release-tag.yml`, which suggests `${{ (! steps.latest-tag.outputs.SKIP_TAG ) }}`. The allow-list has no record for it, because `cargo xtask oracle` cannot apply records to corpus files yet (#49).

<a id="glob-escape-character"></a>
### Escaped characters in branch and tag globs

- Upstream: for `\[`, `\?` and `\*` in a branch or tag filter, the error reports the character after the escaped one (`glob.go:100`). `\[x` reports `'x'`, and `\[` at the end reports the end-of-input rune.
- workflint: the error reports the escaped character, for example `'['`. The column is the same as upstream.
- Why: the message names the wrong character.
- Upstream issue: none.
- Milestone: M2.
- Status: implemented in the validator, and the `glob` rule reports it from M4.15.
- Fixtures: none. The `workflint-syntax` test `an_escaped_special_character_in_a_ref_glob_reports_that_character` and the `workflint-rules` test `an_escaped_special_character_in_a_ref_pattern_is_reported_at_that_character` cover it.

<a id="branding-color-message"></a>
### Branding color message names `branding.color`

- Upstream: an invalid color in a local action's metadata reports `incorrect color %q at branding.icon ...` (`rule_action.go:565`).
- workflint: the message says `at branding.color`.
- Why: the upstream message names the wrong key.
- Upstream issue: none.
- Milestone: M4.
- Status: implemented in M4.18.
- Fixtures: `examples/action_metadata_syntax_validation`, `projects/local_action_invalid`.

<a id="action-file-not-found-warning"></a>
### A missing file of a local action is a warning

- Upstream: a file that the `runs` section of a local action names and that does not exist is an error, like every other finding: `file "dist/index.js" does not exist in "...". it is specified at "main" key in "runs" section in "..." action`.
- workflint: the message is the same, and the rule `action-file-not-found` has the default severity warning. This covers `main`, `pre`, and `post` of JavaScript actions and `image`, `pre-entrypoint`, `entrypoint`, and `post-entrypoint` of Docker actions. Until M5.6 applies `--fail-on`, a warning still makes the exit code 1, so the output and the exit code do not change yet. After M5.6, a run whose only findings are missing files exits with 0 by default.
- Why: a JavaScript action's entrypoint is often built by an earlier step of the job, such as `npm run build`, and is not in the repository when workflint runs.
- Upstream issue: #518.
- Milestone: M4.18 (severity), M5.6 (exit code).
- Status: the severity is implemented. The exit code follows with M5.6.

<a id="dispatch-options-quoting"></a>
### `workflow_dispatch` options quoted once

- Upstream: when a `choice` input's default is not among its options, the options are quoted twice: `... is not included in its options "\"Tama\", \"Mike\""` (`rule_events.go:268`).
- workflint: `... is not included in its options "Tama", "Mike"`.
- Why: the escaped quotes are unreadable.
- Upstream issue: none.
- Milestone: M4.
- Status: implemented.
- Fixtures: `err/workflow_dispatch_input_types`, `examples/workflow_dispatch_input_types`.

<a id="deprecated-command-digits"></a>
### Deprecated commands with digits in the name

- Upstream: the pattern for `::set-output`, `::save-state` and `::set-env` requires the name to match `[a-zA-Z][a-zA-Z_-]*`, so `::set-output name=result1::x` is not reported.
- workflint: digits are allowed after the first letter.
- Why: upstream misses real uses. The command is deprecated whatever the name.
- Upstream issue: none.
- Milestone: M4.
- Status: planned.

<a id="host-time-zones"></a>
### Schedule time zones do not depend on the host

- Upstream: `on.schedule[*].timezone` is checked with Go's `time.LoadLocation`, which reads the host's zoneinfo before the tzdata embedded in the binary (tzdata 2025c in a build with Go 1.26.4). So the answer depends on the host. On Linux with `/usr/share/zoneinfo`, `localtime` and `posixrules` are accepted as well. A host with a newer tzdata accepts zones that 2025c lacks. On a file system that ignores case, such as macOS's default, `america/new_york` is accepted.
- workflint: accepts exactly the 598 names of that embedded tzdata, with case. It rejects `UTC` in any case, `local` in any case, and the empty string, as upstream does before it calls `time.LoadLocation`.
- The `TZ=` and `CRON_TZ=` prefix of a cron expression goes through the same lookup. There workflint accepts the names of the embedded tzdata, `UTC`, `Local`, and the empty name, as `time.LoadLocation` does. Any other name gives `unknown time zone <name>`, or `time: invalid location name` for a name with `..` or a leading `/` or `\`. Upstream on Linux accepts `TZ=localtime` and reports `TZ=America` with `provided bad location America: is a directory`.
- Why: the same input must give the same output on every host (ADR 0013).
- Upstream issue: none.
- Milestone: M4.
- Status: implemented.

<a id="cron-interval-in-utc"></a>
### Schedule intervals are computed in UTC

- Upstream: `cron-too-frequent` compares the first two runs of a schedule after the Unix epoch. robfig/cron computes them in the host's local time zone, or in the zone of a `TZ=` prefix. A schedule whose runs are unevenly spaced gets a different interval in different zones. `0,1 0 * * *` runs at 00:01 and then at 00:00 the next day in UTC, an interval of 86340 seconds. A host in `Asia/Tokyo` gets 00:00 and 00:01 instead and reports `it runs once per 60 seconds`. `TZ=Asia/Tokyo 0,1 0 * * *` is reported on every host.
- workflint: computes both runs in UTC, whatever the host and the prefix, so neither example is reported.
- Why: the same input must give the same output on every host (ADR 0013). Computing in the prefix's zone needs its offsets and daylight saving rules, which `workflint-data` does not have, and upstream only checks the first interval anyway.
- Upstream issue: none.
- Milestone: M4.
- Status: implemented.

<a id="cron-time-zone-without-fields"></a>
### A cron time zone prefix without fields

- Upstream: a cron expression that starts with `TZ=` or `CRON_TZ=` and has no space, such as `TZ=UTC`, crashes actionlint. robfig/cron v3.0.1 slices the name up to the index of the first space, which is -1 (`parser.go:99`).
- workflint: reports the expression as invalid. `TZ=UTC` gives `invalid CRON format "TZ=UTC" in schedule event: TZ= prefix must be followed by a space and 5 fields`, and `CRON_TZ=UTC` names `CRON_TZ=` instead.
- Why: a crash loses every other result.
- Upstream issue: none.
- Milestone: M4.
- Status: implemented.

<a id="deterministic-order"></a>
### Deterministic job order

- Upstream: jobs are stored in a Go map and visited in random order. When there are several cycles in `needs`, which one is reported varies between runs. When a label conflicts with several others, the partner named in `runner-label-conflict` varies too.
- workflint: jobs are visited in source order. The first cycle found in source order is reported, and the conflict partner is the earliest conflicting label.
- Why: the same input must give the same output.
- Upstream issue: none.
- Milestone: M4.
- Status: jobs are visited in source order since M4.1, the `runner-label` rule names the earliest partner since M4.16, and the `job-needs` rule reports the first cycle in source order since M4.20.

<a id="cached-metadata-errors"></a>
### Errors in shared local actions and reusable workflows

- Upstream: an error in a local `action.yml` or a local reusable workflow is reported once per run, at whichever use loaded it first. Files are linted in parallel goroutines and jobs in random order, so the file, the position and even the kind (`action`, `workflow-call` or `expression`) vary between runs. The cache is not single-flight, so two concurrent first uses can both report the error. `testdata/projects/broken_reusable_workflow/README.md` documents the race.
- workflint: each action or workflow is loaded once. Its errors are reported once, at the first use in order of file path and then source position, by the `action` or `workflow-call` rule. The `expression` rule reports the error of a reusable workflow when a job's `needs` names a later job that calls it, because typing `needs` looks the callee up before the `workflow-call` rule visits the later job. The position is the later job's `uses:`, as upstream. It looks up only a valid `./` path. When the later job's `uses:` is an invalid `./` value, which the `workflow-call` rule reports, upstream's `expression` rule also reports the file as missing, and workflint does not. The `action` rule skips a local action whose `uses:` contains `${{ }}`, so broken metadata behind such a value is not reported, where upstream's `expression` rule reports it.
- Why: the same input must give the same output.
- Upstream issue: none.
- Milestone: M5.
- Status: the cache computes each entry once and gives its error to the first lookup only (M4.3). The binary lints files in the order given, so the first use is the first in that order. The `workflow-call` rule reports the errors of reusable workflows since M4.25. The `expression` rule types step outputs without reporting them since M4.9, and reports the errors of reusable workflows that typing `needs` finds since M4.10. The `action` rule reports the errors of local actions since M4.18. The order by file path follows with M5.3.
- Fixtures: `projects/broken_local_action`, `projects/broken_reusable_workflow`, `projects/workflow_call_not_found`.

<a id="with-expression-errors"></a>
### A `with:` value whose expression has an error is not type-checked

- Upstream: a `with:` value whose `${{ }}` has an error, such as an undefined property, is typed as a string. A call that passes it to a `number` or `boolean` input of a local reusable workflow gets a second error, `string value cannot be assigned`, on top of the first.
- workflint: the value gets no type check, so only the expression's own error is reported.
- Why: the type of the value is not known. The second error describes a value that the author did not write.
- Upstream issue: none.
- Milestone: M4.
- Status: implemented in M4.10. No fixture shows it, so the allow-list has no records for it.

<a id="dispatch-input-hint"></a>
### Hint for inputs of `workflow_dispatch`

- Upstream: a call to a local reusable workflow that sets an input the callee declares only under `on.workflow_dispatch.inputs` gets the plain `input "x" is not defined in "./w.yaml" reusable workflow. ...` message (`rule_workflow_call.go:113`).
- workflint: the check still uses only `on.workflow_call.inputs`, and the message ends with `. "x" is defined in "workflow_dispatch" inputs, but a reusable workflow call sets only "workflow_call" inputs`.
- Why: a workflow often declares the same inputs for both triggers. When the author forgets them under `workflow_call`, the plain message lists the `workflow_call` inputs and leaves the author wondering why an input they can see is undefined.
- Upstream issue: #336.
- Milestone: M4.
- Status: implemented.

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
- workflint: paths are compared by whole components. Each file gets the project of its own nearest root, so a file in `/work/repo-other/` never uses the caches of `/work/repo`.
- Why: files from a sibling directory got the wrong config and caches.
- Upstream issue: #750.
- Milestone: M4 for the caches, M5 for the config.
- Status: implemented for the caches of local actions and local reusable workflows. No fixture covers it. The config is planned for M5.

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

<a id="closed-pipe"></a>
### A closed output pipe ends the run with its exit code

- Upstream: when the reader of stdout goes away, as in `actionlint x.yaml | head -1`, the next write kills the process with SIGPIPE. It prints nothing more, and a shell reports exit status 141.
- workflint: it stops writing, prints nothing to stderr, and exits with the code the run would have had: 1 when there are diagnostics, 0 when there are none. Rust ignores SIGPIPE, and workflint forbids the unsafe code it would take to restore it.
- Why: the exit code still says whether the files have problems, and a shell pipeline without `pipefail` reports the last command either way.
- Upstream issue: none.
- Milestone: M2.
- Status: implemented.

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

- A multi-line `if: |` whose only content is `${{ ... }}` is reported as always true, because the block scalar adds a trailing newline (#517). Upstream documents this in `docs/checks.md#if-cond-constant` as the runner's behaviour, and `examples/if_cond_always_true` and `err/if_cond_edge_cases_trailing_leading_chars` pin it. The runner confirms it: `TemplateReader.ParseScalar` in `actions/runner` at `9abfe11` does not trim the value, so the line break becomes text after the expression, and the condition becomes a `format()` call that returns a non-empty string.
- The schedule timezone `UTC` is rejected, although `UTC` is an IANA name. Whether GitHub accepts it is not confirmed.
- In a branch, tag, or path filter, `[!a]` is a character match of `!` and `a`, not a negated match. GitHub's filter pattern cheat sheet documents `!` only at the start of a pattern, and workflint reads `[!...]` as the reference does, so no difference exists.
- The reference's glob validator reports Go's `invalid UTF-8 encoding` scanner error, and workflint's validator, which takes a Rust string, does not. No workflow can reach that error: go-yaml rejects a file with invalid UTF-8, and its escapes always produce valid UTF-8.
- Messages that embed Go library errors, such as `strconv.ParseFloat: parsing "x": invalid syntax` and robfig/cron errors, keep that text.
