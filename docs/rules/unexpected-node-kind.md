# unexpected-node-kind

| Kind | Category | Default severity |
|---|---|---|
| `syntax-check` | correctness | error |

A node has the wrong kind, such as a sequence where a mapping is expected.

The parser skips the node, so the rules do not check it.

## Why

GitHub rejects a section that has the wrong kind of value.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    env:
      - CI=true
    steps:
      - run: ./test.sh
```

```text
test.yaml:6:7: "env" section is sequence node but mapping node is expected [unexpected-node-kind]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    env:
      CI: true
    steps:
      - run: ./test.sh
```

## Differences from actionlint

- [Section name of an event with an empty name](../differences.md#empty-event-name-section)
- [Keys that are not strings in `permissions` and `env`](../differences.md#non-scalar-keys)
- [`snapshot` errors name the right section](../differences.md#snapshot-section-name)
