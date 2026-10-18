# choice-input-without-options

| Kind | Category | Default severity |
|---|---|---|
| `events` | correctness | error |

A `choice` input of `workflow_dispatch` has no `options`.

An empty `options` list also gets [empty-section](empty-section.md).

## Why

GitHub shows a `choice` input as a list of its options. Without options there is nothing to choose.

## Failing example

```yaml
on:
  workflow_dispatch:
    inputs:
      environment:
        type: choice
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

```text
test.yaml:4:7: input type of "environment" is "choice" but "options" is not set [choice-input-without-options]
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
