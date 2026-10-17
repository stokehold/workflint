# invalid-reusable-call-key

| Kind | Category | Default severity |
|---|---|---|
| `syntax-check` | correctness | error |

A job that calls a reusable workflow has a key that only a job with steps may have.

The keys are `runs-on`, `environment`, `outputs`, `env`, `defaults`, `steps`, `timeout-minutes`, `continue-on-error`, `container`, and `services`. Each one gets its own error at the key.

## Why

GitHub rejects these keys on a job that calls a reusable workflow. The called workflow chooses its own runners and steps.

## Failing example

```yaml
on: push
jobs:
  build:
    uses: octo-org/example/.github/workflows/build.yaml@v1
    runs-on: ubuntu-latest
```

```text
test.yaml:5:5: when a reusable workflow is called with "uses", "runs-on" is not available. only following keys are allowed: "name", "uses", "with", "secrets", "needs", "if", and "permissions" in job "build" [invalid-reusable-call-key]
```

## Passing example

```yaml
on: push
jobs:
  build:
    uses: octo-org/example/.github/workflows/build.yaml@v1
```

## Differences from actionlint

- [`services` on a reusable workflow call job](../differences.md#reusable-call-services)
- [Every misplaced key of a reusable workflow call job is reported](../differences.md#reusable-call-keys)
