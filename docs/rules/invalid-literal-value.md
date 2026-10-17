# invalid-literal-value

| Kind | Category | Default severity |
|---|---|---|
| `syntax-check` | correctness | error |

A boolean, integer, or float value has the wrong tag or does not parse.

Plain scalars get their type from the YAML 1.2 core schema. A value of another type, or a number that does not fit, gets this error. Text that is not a number or a boolean at all gets [expected-single-expression](expected-single-expression.md) instead.

## Why

GitHub rejects a workflow whose value has the wrong type, such as a fraction where it needs a whole number.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    strategy:
      max-parallel: 1.5
      matrix:
        node: [20, 22]
    steps:
      - run: ./test.sh
```

```text
test.yaml:6:21: expected scalar node for integer value but found scalar node with "!!float" tag [invalid-literal-value]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    strategy:
      max-parallel: 2
      matrix:
        node: [20, 22]
    steps:
      - run: ./test.sh
```

## Differences from actionlint

- [No panic on NaN](../differences.md#nan-panic)
- [Plain scalars follow the YAML 1.2 core schema](../differences.md#yaml-core-schema)
