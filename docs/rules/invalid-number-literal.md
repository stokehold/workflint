# invalid-number-literal

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

A number literal in an expression has no value as a double.

GitHub evaluates numbers as doubles, so a literal beyond the largest double, about `1.8e308`, has no value.

## Why

A literal that no double can hold is a typo or a value that GitHub cannot represent.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    timeout-minutes: ${{ 1e400 }}
    steps:
      - run: ./test.sh
```

```text
test.yaml:5:26: parsing invalid float literal "1e400": strconv.ParseFloat: parsing "1e400": value out of range [invalid-number-literal]
```

## Passing example

An integer beyond 32 bits is valid:

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    timeout-minutes: ${{ 2147483648 }}
    steps:
      - run: ./test.sh
```

## Differences from actionlint

- [Integer literals beyond 32 bits](../differences.md#int-literal-range)
