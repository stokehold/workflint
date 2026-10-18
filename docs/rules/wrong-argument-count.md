# wrong-argument-count

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

A call of a built-in function has the wrong number of arguments.

The message gives the function's signature. A function with several signatures, such as `contains()`, gets one message per signature.

## Why

GitHub rejects a workflow with a call that has too few or too many arguments.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ startsWith(github.ref) }}'
```

```text
test.yaml:6:24: number of arguments is wrong. function "startsWith(string, string) -> bool" takes 2 parameters but 1 arguments are given [wrong-argument-count]
```

## Passing example

`join()` takes an optional separator, and `hashFiles()` takes any number of patterns:

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ join(github.event.commits.*.id) }} ${{ hashFiles('a', 'b', 'c') }}'
```
