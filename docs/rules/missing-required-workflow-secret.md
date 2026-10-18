# missing-required-workflow-secret

| Kind | Category | Default severity |
|---|---|---|
| `workflow-call` | correctness | error |

A call does not pass a secret that the local reusable workflow requires.

A secret of `on.workflow_call.secrets` is required when it has `required: true`. A call with `secrets: inherit` passes every secret of the caller and is not checked. Secret names are case-insensitive. Only local reusable workflows, called with `./`, are checked.

## Why

GitHub fails the run when a call leaves out a required secret.

## Failing example

```yaml file=.github/workflows/publish.yaml
on:
  workflow_call:
    secrets:
      registry-token:
        required: true
jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - run: ./publish.sh
```

```yaml
on: push
jobs:
  publish:
    uses: ./.github/workflows/publish.yaml
```

```text
test.yaml:4:11: secret "registry-token" is required by "./.github/workflows/publish.yaml" reusable workflow [missing-required-workflow-secret]
```

## Passing examples

```yaml file=.github/workflows/publish.yaml
on:
  workflow_call:
    secrets:
      registry-token:
        required: true
jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - run: ./publish.sh
```

```yaml
on: push
jobs:
  publish:
    uses: ./.github/workflows/publish.yaml
    secrets:
      registry-token: ${{ secrets.REGISTRY_TOKEN }}
```

```yaml file=.github/workflows/publish.yaml
on:
  workflow_call:
    secrets:
      registry-token:
        required: true
jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - run: ./publish.sh
```

```yaml
on: push
jobs:
  publish:
    uses: ./.github/workflows/publish.yaml
    secrets: inherit
```
