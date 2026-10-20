# wrong-argument-type

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

An argument of a built-in function has a type that its parameter does not accept.

Only `any` and `number` convert to a string. A predicate of `case()` must be a `bool` or `any`. The predicates are the 1st, 3rd, 5th, and later odd-numbered arguments, except the last one, which is the default.

## Why

GitHub converts an object to the string `Object` and an array to `Array`, so an object or an array where a function takes a string is a bug. GitHub does not convert a predicate of `case()` to a boolean. A call whose predicate is not a boolean fails at run time with `case predicate must evaluate to a boolean value`.

## Failing examples

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

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ case(github.ref_name, 'named', 'unnamed') }}'
```

```text
test.yaml:6:29: case() predicate must evaluate to a boolean value but the 1st argument has type "string" [wrong-argument-type]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ startsWith(github.ref, 'refs/tags/') }} ${{ case(github.ref_name != '', 'named', 'unnamed') }}'
```

## Differences from actionlint

- [`bool` no longer accepts every type](../differences.md#bool-assignability)
- [The predicate of `case()` must be a boolean](../differences.md#case-predicate-type)
