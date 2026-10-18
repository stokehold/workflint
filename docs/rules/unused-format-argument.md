# unused-format-argument

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

The format string of `format()` has no placeholder for an argument.

The rule reads the format string only when it is a string literal.

## Why

`format()` drops an argument that no placeholder names. The format string usually lacks a placeholder that it meant to have.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ format('{0}-{1}', github.ref, github.sha, github.run_id) }}'
```

```text
test.yaml:6:24: format string "{0}-{1}" does not contain placeholder {2}. remove argument which is unused in the format string [unused-format-argument]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ format('{0}-{1}-{2}', github.ref, github.sha, github.run_id) }}'
```
