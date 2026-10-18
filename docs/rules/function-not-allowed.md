# function-not-allowed

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

An expression calls a special function that its workflow key does not allow.

The special functions are `always()`, `cancelled()`, `failure()`, `success()`, and `hashFiles()`. The status functions work only in `if:` conditions, and `hashFiles()` only in the keys of a step.

## Why

GitHub rejects the workflow when a key calls a special function that it does not allow.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo 'Success? ${{ success() }}'
```

```text
test.yaml:6:33: calling function "success" is not allowed here. "success" is only available in "jobs.<job_id>.if", "jobs.<job_id>.steps.if". see https://docs.github.com/en/actions/learn-github-actions/contexts#context-availability for more details [function-not-allowed]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo Success
        if: ${{ success() }}
```
