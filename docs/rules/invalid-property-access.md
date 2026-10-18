# invalid-property-access

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

An expression reads a property with a dot from a value that is not an object.

## Why

A property of a string, a number, a boolean, or `null` evaluates to an empty string. The expression usually confuses a string, such as `github.repository`, with an object.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ github.repository.owner }}'
```

```text
test.yaml:6:24: receiver of object dereference "owner" must be type of object but got "string" [invalid-property-access]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ github.repository_owner }}'
```
