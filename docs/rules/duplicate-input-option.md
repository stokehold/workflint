# duplicate-input-option

| Kind | Category | Default severity |
|---|---|---|
| `events` | correctness | error |

A `choice` input of `workflow_dispatch` lists an option more than once.

Each repetition after the first gets a diagnostic.

## Why

The list shows the same option twice. The second entry is a typo or a leftover.

## Failing example

```yaml
on:
  workflow_dispatch:
    inputs:
      environment:
        type: choice
        options: [staging, production, staging]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

```text
test.yaml:6:40: option "staging" is duplicated in options of "environment" input [duplicate-input-option]
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
