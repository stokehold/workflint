# default-not-in-options

| Kind | Category | Default severity |
|---|---|---|
| `events` | correctness | error |

The default of a `choice` input of `workflow_dispatch` is not one of its options.

The comparison is case-sensitive. When an option is not a valid string, which [unexpected-node-kind](unexpected-node-kind.md) or [empty-string](empty-string.md) reports, the default is not compared.

## Why

GitHub preselects the default in the list of options, and a default that is not an option cannot be selected.

## Failing example

```yaml
on:
  workflow_dispatch:
    inputs:
      environment:
        type: choice
        options: [staging, production]
        default: dev
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

```text
test.yaml:7:18: default value "dev" of "environment" input is not included in its options "staging", "production" [default-not-in-options]
```

## Passing example

```yaml
on:
  workflow_dispatch:
    inputs:
      environment:
        type: choice
        options: [staging, production]
        default: staging
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

## Differences from actionlint

- [`workflow_dispatch` options quoted once](../differences.md#dispatch-options-quoting)
- [An invalid value is reported once](../differences.md#invalid-value-reported-once)
