# yaml-syntax-error

| Kind | Category | Default severity |
|---|---|---|
| `syntax-check` | correctness | error |

The input is not valid YAML.

workflint reports the first syntax error of the YAML parser, and only that one, as `could not parse as YAML: <problem>`. No rule runs on the file, because there is no tree to check. Bytes that do not decode as UTF-8, or as UTF-16 after a UTF-16 byte order mark, are a syntax error too.

## Why

GitHub cannot read a workflow that is not valid YAML, so it never runs it. A plain scalar that contains `: `, as in `echo hello: world`, is a common cause, because YAML reads it as the start of a mapping. Quote the value or use a block scalar.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello: world
```

```text
test.yaml:6:24: could not parse as YAML: mapping values are not allowed in this context [yaml-syntax-error]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: 'echo hello: world'
```

## Differences from actionlint

- [YAML syntax error positions](../differences.md#yaml-error-positions)
- [Scanner error after a comment before a `-` item](../differences.md#scanner-error-after-comment-before-dash)
- [Control characters in UTF-16 files](../differences.md#utf16-control-characters)
- [A tag followed by a comma in a flow collection](../differences.md#tag-before-flow-comma)
- [The `\/` escape in double-quoted strings](../differences.md#yaml-solidus-escape)
- [The `%YAML 1.2` directive](../differences.md#yaml-1-2-directive)
