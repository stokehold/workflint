# required-input-with-default

| Kind | Category | Default severity |
|---|---|---|
| `events` | correctness | error |

A required `workflow_call` input has a default, which is never used.

The check also runs when the default contains `${{ }}`.

## Why

Every caller must give a required input, so GitHub never uses its default. Either the input is optional or the default is dead.

## Failing example

```yaml
on:
  workflow_call:
    inputs:
      environment:
        type: string
        required: true
        default: staging
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

```text
test.yaml:7:18: input "environment" of workflow_call event has the default value "staging", but it is also required. if an input is marked as required, its default value will never be used [required-input-with-default]
```

## Passing example

```yaml
on:
  workflow_call:
    inputs:
      environment:
        type: string
        required: false
        default: staging
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

## Differences from actionlint

- [An invalid value is reported once](../differences.md#invalid-value-reported-once)
