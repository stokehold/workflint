# JSON output

`-format json` and `-format jsonl` print the diagnostics in a JSON schema that has the rule ID, the severity, the help text, and byte offsets, which actionlint's `-format` fields lack. A tool can read this output without a template. `--format` is the same flag, as for every flag ([cli.md](cli.md#flag-syntax)).

```sh
workflint -format json
workflint --format jsonl .github/workflows/ci.yaml
```

The schema is [json-output.schema.json](json-output.schema.json), in JSON Schema draft 2020-12. A test validates workflint's output against it.

## `json`

`json` prints one document on one line, followed by a line break. For a workflow whose step runs `echo ${{ gihub.sha }}`, it prints:

```json
{"version":1,"diagnostics":[{"rule":"undefined-variable","kind":"expression","severity":"error","message":"undefined variable \"gihub\". available variables are \"env\", \"github\", \"inputs\", \"job\", \"matrix\", \"needs\", \"runner\", \"secrets\", \"steps\", \"strategy\", \"vars\"","help":"did you mean \"github\"?","path":".github/workflows/ci.yaml","start":{"line":6,"column":23,"offset":83},"end":{"line":6,"column":28,"offset":88}}]}
```

`diagnostics` lists the diagnostics in the order of the default output: the files in the order of the arguments, or of their paths in a run without arguments, and the diagnostics of each file by position. A run without diagnostics prints `{"version":1,"diagnostics":[]}`.

## `jsonl`

`jsonl` prints one diagnostic per line, in the same order, with the `version` key first:

```json
{"version":1,"rule":"undefined-variable","kind":"expression","severity":"error","message":"undefined variable \"gihub\". available variables are \"env\", \"github\", \"inputs\", \"job\", \"matrix\", \"needs\", \"runner\", \"secrets\", \"steps\", \"strategy\", \"vars\"","help":"did you mean \"github\"?","path":".github/workflows/ci.yaml","start":{"line":6,"column":23,"offset":83},"end":{"line":6,"column":28,"offset":88}}
```

A run without diagnostics prints nothing. The schema of a line is `#/$defs/line` in the schema file.

## Diagnostic keys

| Key | Value |
|---|---|
| `rule` | The workflint rule ID, such as `undefined-variable`. The default output prints it in brackets, and ignore comments name it. |
| `kind` | The actionlint kind of the rule, such as `expression`, which actionlint's `-format` field `Kind` holds. |
| `severity` | `error`, `warning`, or `info`. |
| `message` | The message, as the default output prints it. |
| `help` | A hint on how to fix the problem, such as `did you mean "github"?`, or `null`. The default output does not print it (ADR 0032). |
| `path` | The path of the file, as the default output prints it, or the name of the workflow read from stdin. |
| `start` | The position of the first byte of the span. |
| `end` | The position just after the last byte of the span. An empty span has `end` equal to `start`. |

A position has three keys:

| Key | Value |
|---|---|
| `line` | The 1-based line. |
| `column` | The 1-based column, counted in Unicode code points, as in the default output. |
| `offset` | The 0-based offset in bytes from the start of the file. |

`end` points after the span, so `end.column` is one more than the `EndColumn` of a `-format` template for a span on one line. With the offsets, `text[start.offset..end.offset]` is the span in the file's bytes.

Strings hold UTF-8 text as it is, without `\u` escapes for characters outside ASCII. The output has no color, `-oneline` changes nothing, and `-silent` prints nothing. The exit code is the one of the default output.

## Versions

`version` is `1`. It changes only when a key is removed or renamed, or its value changes meaning. A later workflint may add keys without changing the version, so a reader should ignore keys it does not know. Labels and fixes are planned as such keys. Each release will publish the schema file with its binaries.
