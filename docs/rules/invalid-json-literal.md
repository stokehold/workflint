# invalid-json-literal

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

The string literal passed to `fromJSON()` is not valid JSON.

The message gives the byte offset of the error in the string.

## Why

GitHub fails the run when `fromJSON()` cannot parse its argument. Workflows often build constant arrays and objects with `fromJSON('...')`, so a broken literal is a typo that no run can pass.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo deploy
        if: contains(fromJSON('["main", "release"'), github.ref_name)
```

```text
test.yaml:7:31: broken JSON string is passed to fromJSON() at offset 18: unexpected end of JSON input [invalid-json-literal]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo deploy
        if: contains(fromJSON('["main", "release"]'), github.ref_name)
```

## Differences from actionlint

- [Deeply nested `fromJSON` literals](../differences.md#fromjson-nesting)
