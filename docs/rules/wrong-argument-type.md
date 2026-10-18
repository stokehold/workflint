# wrong-argument-type

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

An argument of a built-in function has a type that its parameter does not accept.

Only `any` and `number` convert to a string. A parameter that GitHub reads as a condition, such as the predicate of `case()`, accepts every type.

## Why

GitHub converts an object to the string `Object` and an array to `Array`, so an object or an array where a function takes a string is a bug.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ startsWith('refs/tags/', github.event) }}'
```

```text
test.yaml:6:49: 2nd argument of function call is not assignable. "object" cannot be assigned to "string". called function type is "startsWith(string, string) -> bool" [wrong-argument-type]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ startsWith(github.ref, 'refs/tags/') }} ${{ case(github.ref_name, 'named', 'unnamed') }}'
```

## Differences from actionlint

- [`bool` no longer accepts every type](../differences.md#bool-assignability)
