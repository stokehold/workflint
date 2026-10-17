# step-without-run-or-uses

| Kind | Category | Default severity |
|---|---|---|
| `syntax-check` | correctness | error |

A step has neither `run` nor `uses`.

When a step has both keys, the later one wins, and the other gets [unexpected-key](unexpected-key.md).

## Why

A step must either run a script or run an action. GitHub rejects a step that does neither.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - name: Test
        env:
          CI: true
```

```text
test.yaml:6:9: step must run script with "run" section or run action with "uses" section [step-without-run-or-uses]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - name: Test
        run: ./test.sh
        env:
          CI: true
```
