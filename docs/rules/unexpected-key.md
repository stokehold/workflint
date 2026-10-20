# unexpected-key

| Kind | Category | Default severity |
|---|---|---|
| `syntax-check` | correctness | error |

A mapping has a key that its section does not define.

The message lists the keys that the section accepts. Most keys are case sensitive, so `Shell` is not `shell`.

## Why

GitHub rejects a workflow with an unknown key. A typo in a key name would otherwise make the setting silently missing.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: ./test.sh
        Shell: bash
```

```text
test.yaml:7:9: unexpected key "Shell" for step to run shell command. expected one of "continue-on-error", "env", "id", "if", "name", "run", "shell", "timeout-minutes", "working-directory" [unexpected-key]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: ./test.sh
        shell: bash
```

## Differences from actionlint

- [Complete key list for `workflow_dispatch` inputs](../differences.md#dispatch-input-key-list)
- [`${{ insert }}` as a mapping key](../differences.md#insert-mapping-key)
