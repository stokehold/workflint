# yaml-syntax-error

| Kind | Category | Default severity |
|---|---|---|
| `syntax-check` | correctness | error |

The input is not valid YAML.

workflint reports the first syntax error of the YAML parser, and only that one, as `could not parse as YAML: <problem>`. No rule runs on the file, because there is no tree to check. Bytes that do not decode as UTF-8, or as UTF-16 after a UTF-16 byte order mark, are a syntax error too.

The parser's `did not find expected key` says only that a block mapping met something other than a key. The diagnostic gets a help text that names the likely cause when it is one of these:

- A `-` list item indented differently from the items before it. The help text gives both indentations and the line of the earlier item.
- A `-` list item among the keys of a mapping, with no list above it.
- A key indented deeper than the keys of its mapping, after a value that ends on its own line, such as a quoted string. The help text gives both indentations.
- Text after a flow sequence, a flow mapping, or a quoted string on the same line, as in `name: [Foo] bar`.
- A key with no space after its `:`, as in `runs-on:ubuntu-latest`. The parser reports this as `could not find expected ':'` when the key is at the indentation of the other keys, and the help text names the cause there too.

Other errors get no help text. The default and one-line formats do not print it (ADR 0032).

## Why

GitHub cannot read a workflow that is not valid YAML, so it never runs it. A plain scalar that contains `: `, as in `echo hello: world`, is a common cause, because YAML reads it as the start of a mapping. Quote the value or use a block scalar.

## Failing examples

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

A list item indented less than the item before it:

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo build
     - run: echo test
```

```text
test.yaml:7:6: could not parse as YAML: did not find expected key [yaml-syntax-error]
```

The help text is `this list item is indented 5 spaces and the item on line 6 is indented 6, but the items of one list need the same indentation`.

Text after a flow sequence:

```yaml
on: push
jobs:
  test:
    name: [Foo] bar
    runs-on: ubuntu-latest
    steps:
      - run: echo test
```

```text
test.yaml:4:17: could not parse as YAML: did not find expected key [yaml-syntax-error]
```

The help text is `a flow sequence ends at its "]" and YAML allows no more text after it on the line, so quote the whole value if it is a string`.

A key with no space after its colon:

```yaml
on: push
jobs:
  test:
    steps:
      - run: echo test
    runs-on:ubuntu-latest
```

```text
test.yaml:7:1: could not parse as YAML: could not find expected ':' [yaml-syntax-error]
```

The help text is `YAML reads "runs-on:ubuntu-latest" as one string, because a ":" after a key needs a space after it; did you mean "runs-on: ubuntu-latest"?`.

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
