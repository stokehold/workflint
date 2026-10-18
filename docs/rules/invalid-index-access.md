# invalid-index-access

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

An index has the wrong type, or its operand is neither an object nor an array.

An object takes a string index and an array takes a number index. Any other operand, such as a string or a boolean, takes no index.

## Why

An index of the wrong type, or an index into a value that has no elements, evaluates to an empty string.

## Failing examples

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ env[0] }}'
```

```text
test.yaml:6:28: property access of object must be type of string but got "number" [invalid-index-access]
```

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ github.sha[0] }}'
```

```text
test.yaml:6:24: index access operand must be type of object or array but got "string" [invalid-index-access]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ env['HOME'] }} ${{ github.event.commits[0].id }}'
```
