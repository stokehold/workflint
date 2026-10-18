# undefined-workflow-input

| Kind | Category | Default severity |
|---|---|---|
| `workflow-call` | correctness | error |

`with:` sets an input that the local reusable workflow does not declare.

Only the inputs of `on.workflow_call.inputs` count. When the callee declares the input under `on.workflow_dispatch.inputs` instead, the message says so, because a call cannot set those. Input names are case-insensitive. Only local reusable workflows, called with `./`, are checked.

## Why

GitHub rejects a call that sets an input the callee does not declare, and the run fails. The message lists the inputs the callee declares, which catches a misspelled name.

## Failing examples

```yaml file=.github/workflows/deploy.yaml
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

```yaml
on: push
jobs:
  deploy:
    uses: ./.github/workflows/deploy.yaml
    with:
      enviroment: production
```

```text
test.yaml:6:7: input "enviroment" is not defined in "./.github/workflows/deploy.yaml" reusable workflow. defined input is "environment" [undefined-workflow-input]
```

An input that only `workflow_dispatch` declares gets a hint:

```yaml file=.github/workflows/deploy.yaml
on:
  workflow_dispatch:
    inputs:
      environment:
        type: string
  workflow_call:
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
      environment: production
```

```text
test.yaml:6:7: input "environment" is not defined in "./.github/workflows/deploy.yaml" reusable workflow. no input is defined. "environment" is defined in "workflow_dispatch" inputs, but a reusable workflow call sets only "workflow_call" inputs [undefined-workflow-input]
```

## Passing example

```yaml file=.github/workflows/deploy.yaml
on:
  workflow_dispatch:
    inputs:
      environment:
        type: string
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

```yaml
on: push
jobs:
  deploy:
    uses: ./.github/workflows/deploy.yaml
    with:
      environment: production
```

## Differences from actionlint

- [Hint for inputs of `workflow_dispatch`](../differences.md#dispatch-input-hint)
