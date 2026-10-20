# expected-single-expression

| Kind | Category | Default severity |
|---|---|---|
| `syntax-check` | correctness | error |

A value that must be a literal or a single `${{ }}` expression is other text.

Boolean, integer, and float values, such as `continue-on-error`, `max-parallel`, and `timeout-minutes`, accept a literal of their type or one expression that computes it, and `env` accepts a mapping or one expression. Text around the expression, or text without one, gets this error. The value of an `${{ insert }}` key must be one expression too, or a mapping in the places that accept one, such as `matrix`, `strategy`, and a container ([differences](../differences.md#insert-mapping-key)).

## Why

GitHub rejects text where it needs a number, a boolean, or a mapping.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: ./test.sh
        timeout-minutes: two minutes
```

```text
test.yaml:7:26: expecting a single ${{...}} expression or float number literal, but found plain text node [expected-single-expression]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: ./test.sh
        timeout-minutes: 2
```

## Differences from actionlint

- [Plain scalars follow the YAML 1.2 core schema](../differences.md#yaml-core-schema)
- [`${{ insert }}` as a mapping key](../differences.md#insert-mapping-key)
