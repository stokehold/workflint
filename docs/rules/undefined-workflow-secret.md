# undefined-workflow-secret

| Kind | Category | Default severity |
|---|---|---|
| `workflow-call` | correctness | error |

`secrets:` passes a secret that the local reusable workflow does not declare.

Secret names are case-insensitive. Only local reusable workflows, called with `./`, are checked.

## Why

GitHub rejects a call that passes a secret the callee does not declare, and the run fails. The message lists the secrets the callee declares, which catches a misspelled name.

## Failing example

```yaml file=.github/workflows/publish.yaml
on:
  workflow_call:
    secrets:
      registry-token:
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
      registry_token: ${{ secrets.REGISTRY_TOKEN }}
```

```text
test.yaml:6:7: secret "registry_token" is not defined in "./.github/workflows/publish.yaml" reusable workflow. defined secret is "registry-token" [undefined-workflow-secret]
```

## Passing example

```yaml file=.github/workflows/publish.yaml
on:
  workflow_call:
    secrets:
      registry-token:
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
