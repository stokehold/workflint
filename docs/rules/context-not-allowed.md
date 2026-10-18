# context-not-allowed

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

An expression reads a context that its workflow key does not allow.

GitHub evaluates each workflow key at a point of the run where only some contexts exist. For example, `env` is not available in `jobs.<job_id>.env`, which defines it, but it is in `jobs.<job_id>.steps.env`. The message lists the contexts that the key allows.

## Why

GitHub rejects the workflow when an expression reads a context that its key does not allow.

## Failing example

```yaml
on: push
env:
  TARGET: production
jobs:
  test:
    runs-on: ubuntu-latest
    env:
      DEPLOY: ${{ env.TARGET }}
    steps:
      - run: ./test.sh
```

```text
test.yaml:8:19: context "env" is not allowed here. available contexts are "github", "inputs", "matrix", "needs", "secrets", "strategy", "vars". see https://docs.github.com/en/actions/learn-github-actions/contexts#context-availability for more details [context-not-allowed]
```

## Passing example

```yaml
on: push
env:
  TARGET: production
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: ./test.sh
        env:
          DEPLOY: ${{ env.TARGET }}
```
