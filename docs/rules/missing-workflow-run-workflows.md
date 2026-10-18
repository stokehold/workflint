# missing-workflow-run-workflows

| Kind | Category | Default severity |
|---|---|---|
| `events` | correctness | error |

A `workflow_run` event has no `workflows`.

## Why

`workflow_run` runs after the workflows that `workflows` names. GitHub requires the list, and without it the event has no workflow to follow.

## Failing example

```yaml
on:
  workflow_run:
    types: [completed]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

```text
test.yaml:2:3: no workflow is configured for "workflow_run" event [missing-workflow-run-workflows]
```

## Passing example

```yaml
on:
  workflow_run:
    workflows: [CI]
    types: [completed]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```
