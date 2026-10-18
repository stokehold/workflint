# object-in-template

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

A `${{ }}` in a string evaluates to an object, an array, or null.

The rule checks every `${{ }}` in a string value, such as `run`, `name`, or an `env` value. An `if` condition written without `${{ }}` is not a string, so it may have any type. The diagnostic points at the `${{` of the expression.

## Why

GitHub turns an object or an array into the text `Object` or `Array`, and null into an empty string. The step then gets a value that nobody meant. Pass the value through `toJSON()` to get its JSON text, or read the property you need.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ github.event }}'
```

```text
test.yaml:6:20: object, array, and null values should not be evaluated in template with ${{ }} but evaluating the value of type object [object-in-template]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ toJSON(github.event) }}'
```

## Differences from actionlint

- [Every `${{ }}` in a string is checked](../differences.md#all-expressions-in-string)
- [Exact positions inside multi-line and quoted strings](../differences.md#positions-inside-scalars)
