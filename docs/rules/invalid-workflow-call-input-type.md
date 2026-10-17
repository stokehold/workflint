# invalid-workflow-call-input-type

| Kind | Category | Default severity |
|---|---|---|
| `syntax-check` | correctness | error |

A `workflow_call` input has an unknown `type`.

The types are `boolean`, `number`, and `string`. A reusable workflow has no `choice` or `environment` inputs, which `workflow_dispatch` has.

## Why

GitHub rejects an input type that it does not know.

## Failing example

```yaml
on:
  workflow_call:
    inputs:
      environment:
        type: choice
jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - run: ./deploy.sh
```

```text
test.yaml:5:15: invalid value "choice" for input type of workflow_call event. it must be one of "boolean", "number", or "string" [invalid-workflow-call-input-type]
```

## Passing example

```yaml
on:
  workflow_call:
    inputs:
      environment:
        type: string
jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - run: ./deploy.sh
```
