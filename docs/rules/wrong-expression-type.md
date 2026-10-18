# wrong-expression-type

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

A value written as one `${{ }}` has a type that its key does not accept.

A boolean key, such as `continue-on-error` or `cancel-in-progress`, needs a bool. A number key, such as `timeout-minutes` or `max-parallel`, needs a number. A whole-expression `env`, `credentials`, `services`, or `matrix` needs an object, and a matrix row or `exclude` needs an array. A whole-expression `runs-on` needs a string or an array. A value of type `any`, such as the result of `fromJSON()`, is accepted everywhere.

## Why

GitHub fails the run when it cannot convert the value to the type that the key needs. A string in `continue-on-error` or an `env` that is a string, not a mapping, is a mistake that only shows up when the workflow runs.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    continue-on-error: ${{ github.ref }}
    steps:
      - run: ./test.sh
        env: ${{ github.ref }}
```

```text
test.yaml:5:24: type of expression must be bool but found type string [wrong-expression-type]
test.yaml:8:14: type of expression at "env" must be object but found type string [wrong-expression-type]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    continue-on-error: ${{ github.ref != 'refs/heads/main' }}
    steps:
      - run: ./test.sh
        env: ${{ fromJSON(vars.TEST_ENV) }}
```
