# unknown-anchor

| Kind | Category | Default severity |
|---|---|---|
| `syntax-check` | correctness | error |

An alias refers to an anchor that no earlier node defines.

An alias such as `*credentials` must come after the anchor `&credentials` that it refers to. As with a [yaml-syntax-error](yaml-syntax-error.md), no tree is built and no rule runs on the file.

## Why

GitHub rejects a workflow with an undefined alias. The cause is usually a typo in the alias, or an alias written before its anchor.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: ./download.sh
        env: *credentials
```

```text
test.yaml:7:14: could not parse as YAML: unknown anchor 'credentials' referenced [unknown-anchor]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: ./download.sh
        env: &credentials
          USERNAME: ${{ github.actor }}
      - run: ./upload.sh
        env: *credentials
```

## Differences from actionlint

- [Undefined alias position and message prefix](../differences.md#unknown-anchor-position)
