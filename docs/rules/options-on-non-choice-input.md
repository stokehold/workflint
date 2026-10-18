# options-on-non-choice-input

| Kind | Category | Default severity |
|---|---|---|
| `events` | correctness | error |

A `workflow_dispatch` input that is not a `choice` has `options`.

An input without `type`, or with an unknown `type`, is not a `choice`.

## Why

Only a `choice` input reads `options`. On another type the list does nothing, and the input was probably meant to be a `choice`.

## Failing example

```yaml
on:
  workflow_dispatch:
    inputs:
      environment:
        type: string
        options: [staging, production]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

```text
test.yaml:4:7: "options" can not be set to "environment" input because its input type is not "choice" [options-on-non-choice-input]
```

## Passing example

```yaml
on:
  workflow_dispatch:
    inputs:
      environment:
        type: choice
        options: [staging, production]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

## Differences from actionlint

- [An invalid value is reported once](../differences.md#invalid-value-reported-once)
