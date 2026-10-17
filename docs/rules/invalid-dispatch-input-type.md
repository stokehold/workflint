# invalid-dispatch-input-type

| Kind | Category | Default severity |
|---|---|---|
| `syntax-check` | correctness | error |

A `workflow_dispatch` input has an unknown `type`.

The types are `string`, `number`, `boolean`, `choice`, and `environment`.

## Why

GitHub rejects an input type that it does not know.

## Failing example

```yaml
on:
  workflow_dispatch:
    inputs:
      environment:
        type: text
jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - run: ./deploy.sh
```

```text
test.yaml:5:15: input type of workflow_dispatch event must be one of "string", "number", "boolean", "choice", "environment" but got "text" [invalid-dispatch-input-type]
```

## Passing example

```yaml
on:
  workflow_dispatch:
    inputs:
      environment:
        type: string
jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - run: ./deploy.sh
```
