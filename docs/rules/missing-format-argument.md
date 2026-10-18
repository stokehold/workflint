# missing-format-argument

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

The format string of `format()` has a placeholder beyond its arguments.

The rule reads the format string only when it is a string literal.

## Why

GitHub fails the step when it evaluates a placeholder that names an argument the call does not pass.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ format('{0}-{1}-{2}', github.ref, github.sha) }}'
```

```text
test.yaml:6:24: format string "{0}-{1}-{2}" contains placeholder {2} but only 2 arguments are given to format [missing-format-argument]
```

## Passing example

`{{` and `}}` are a literal brace, not a placeholder:

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ format('{{0}}-{0}-{1}', github.ref, github.sha) }}'
```
