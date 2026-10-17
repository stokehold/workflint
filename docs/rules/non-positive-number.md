# non-positive-number

| Kind | Category | Default severity |
|---|---|---|
| `syntax-check` | correctness | error |

A literal `timeout-minutes` or `max-parallel` is zero or less.

The rule checks literal values only. A value from a `${{ }}` expression is known only when the workflow runs.

## Why

A job or step with no time to run, or a matrix that runs no job at once, cannot do anything. GitHub rejects these values.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    timeout-minutes: 0
    steps:
      - run: ./test.sh
```

```text
test.yaml:5:22: value at "timeout-minutes" must be greater than zero: 0 [non-positive-number]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    timeout-minutes: 30
    steps:
      - run: ./test.sh
```
