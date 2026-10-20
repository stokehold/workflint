# undefined-function

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

An expression calls a function that is not built in.

The message lists the built-in functions. Function names are case insensitive, so `toJson` is `toJSON`.

The diagnostic also has a help text with the closest built-in function, such as `did you mean "startsWith"?` for `startWith`. A function that is available only at some workflow keys, such as `always`, counts only where it is available. The default and one-line formats do not print it (ADR 0032).

## Why

GitHub rejects a workflow whose expression calls a function that does not exist. The cause is usually a typo, such as `startWith` for `startsWith`.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ startWith(github.ref, 'refs/tags/') }}'
```

```text
test.yaml:6:24: undefined function "startWith". available functions are "always", "cancelled", "case", "contains", "endswith", "failure", "format", "fromjson", "hashfiles", "join", "startswith", "success", "tojson" [undefined-function]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ startsWith(github.ref, 'refs/tags/') }}'
```
