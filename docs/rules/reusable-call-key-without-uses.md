# reusable-call-key-without-uses

| Kind | Category | Default severity |
|---|---|---|
| `syntax-check` | correctness | error |

A job without `uses` has `with` or `secrets`.

Each of the two keys gets its own error at the key.

## Why

Only a job that calls a reusable workflow passes inputs and secrets with `with` and `secrets`. GitHub rejects them on a job with steps. A step passes inputs to an action with its own `with`.

## Failing example

```yaml
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    with:
      target: release
    steps:
      - run: ./build.sh
```

```text
test.yaml:5:5: "with" is only available for a reusable workflow call with "uses" but "uses" is not found in job "build" [reusable-call-key-without-uses]
```

## Passing example

```yaml
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    env:
      TARGET: release
    steps:
      - run: ./build.sh
```

## Differences from actionlint

- [Every misplaced key of a reusable workflow call job is reported](../differences.md#reusable-call-keys)
