# invalid-comparison

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

A comparison has operands that GitHub cannot compare.

GitHub converts operands of different types to numbers. An object or an array becomes `NaN`, so comparing it with a value of another type is always false. `<`, `<=`, `>`, and `>=` on a boolean, `null`, an object, or an array have no useful meaning. Both cases are errors.

## Why

The comparison has the same result in every run, which is never what the condition meant.

## Failing example

```yaml
on:
  workflow_dispatch:
    inputs:
      long:
        type: boolean
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo called
        if: ${{ github.event == 'workflow_dispatch' }}
      - run: echo long
        if: ${{ inputs.long > 60 }}
```

```text
test.yaml:11:17: "object" value cannot be compared to "string" value with "==" operator [invalid-comparison]
test.yaml:13:17: "bool" value cannot be compared to "number" value with ">" operator [invalid-comparison]
```

## Passing example

`<` binds tighter than `==`, so the second condition compares two booleans:

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo called
        if: ${{ github.event_name == 'push' }}
      - run: echo sorted
        if: ${{ github.ref_name < 'v2' == true }}
```

## Differences from actionlint

- [Comparison operator precedence](../differences.md#comparison-precedence)
