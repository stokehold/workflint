# activity-types-not-supported

| Kind | Category | Default severity |
|---|---|---|
| `events` | correctness | error |

`types` is set on an event that has no activity types.

Events such as `push`, `create`, and `workflow_dispatch` have no activity types. `repository_dispatch` takes any type.

## Why

The event has no activity types for `types` to filter on, so the list cannot do what its author meant. It is often meant for another event, or meant to be `branches` or `tags`.

## Failing example

```yaml
on:
  push:
    types: [created]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

```text
test.yaml:2:3: "types" cannot be specified for "push" Webhook event [activity-types-not-supported]
```

## Passing example

```yaml
on:
  push:
    branches: [main]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```
