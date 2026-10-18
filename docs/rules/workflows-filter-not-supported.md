# workflows-filter-not-supported

| Kind | Category | Default severity |
|---|---|---|
| `events` | correctness | error |

`workflows` is set on an event other than `workflow_run`.

## Why

Only `workflow_run` reads `workflows`. On another event the list filters nothing, so the workflow runs more often than its author meant.

## Failing example

```yaml
on:
  pull_request:
    workflows: [CI]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

```text
test.yaml:2:3: "workflows" cannot be configured for "pull_request" event. it is only for workflow_run event [workflows-filter-not-supported]
```

## Passing example

```yaml
on:
  workflow_run:
    workflows: [CI]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```
