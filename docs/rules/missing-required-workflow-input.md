# missing-required-workflow-input

| Kind | Category | Default severity |
|---|---|---|
| `workflow-call` | correctness | error |

A call does not set an input that the local reusable workflow requires.

An input of `on.workflow_call.inputs` is required when it has `required: true` and no `default`. Input names are case-insensitive. Only local reusable workflows, called with `./`, are checked.

## Why

GitHub fails the run when a call leaves out a required input, before the called workflow's jobs start.

## Failing example

```yaml file=.github/workflows/deploy.yaml
on:
  workflow_call:
    inputs:
      environment:
        type: string
        required: true
jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - run: ./deploy.sh
```

```yaml
on: push
jobs:
  deploy:
    uses: ./.github/workflows/deploy.yaml
```

```text
test.yaml:4:11: input "environment" is required by "./.github/workflows/deploy.yaml" reusable workflow [missing-required-workflow-input]
```

## Passing example

```yaml file=.github/workflows/deploy.yaml
on:
  workflow_call:
    inputs:
      environment:
        type: string
        required: true
jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - run: ./deploy.sh
```

```yaml
on: push
jobs:
  deploy:
    uses: ./.github/workflows/deploy.yaml
    with:
      ENVIRONMENT: production
```
