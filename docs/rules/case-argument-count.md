# case-argument-count

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

`case()` has an even number of arguments, so it has no default value.

`case()` takes pairs of a condition and a value, then a default value that it returns when no condition is true.

## Why

GitHub rejects a workflow that calls `case()` without a default value.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ case(github.ref_name == 'main', 'production', github.ref_name == 'dev', 'staging') }}'
```

```text
test.yaml:6:24: case() requires an odd number of arguments (pred/value pairs + default) but got 4 [case-argument-count]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ case(github.ref_name == 'main', 'production', github.ref_name == 'dev', 'staging', 'test') }}'
```
