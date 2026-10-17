# too-many-yaml-nodes

| Kind | Category | Default severity |
|---|---|---|
| `syntax-check` | correctness | error |

The workflow has more YAML nodes, with its aliases expanded, than GitHub reads.

GitHub reads at most 50,000 YAML nodes, and it counts the nodes of an anchor again at each alias that refers to it. Nested aliases multiply. workflint reports the workflow at the start of its document and parses nothing else in it.

## Why

GitHub rejects the workflow with `Maximum YAML nodes exceeded`. Nested aliases let a file of a few lines expand to billions of nodes, and without the limit workflint would visit every one of them.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: ./test.sh
x0: &x0 [a, a, a, a, a, a, a, a, a, a]
x1: &x1 [*x0, *x0, *x0, *x0, *x0, *x0, *x0, *x0, *x0, *x0]
x2: &x2 [*x1, *x1, *x1, *x1, *x1, *x1, *x1, *x1, *x1, *x1]
x3: &x3 [*x2, *x2, *x2, *x2, *x2, *x2, *x2, *x2, *x2, *x2]
x4: &x4 [*x3, *x3, *x3, *x3, *x3, *x3, *x3, *x3, *x3, *x3]
x5: *x4
```

```text
test.yaml:1:1: workflow has more than 50000 YAML nodes when its aliases are expanded. GitHub rejects it with "Maximum YAML nodes exceeded" [too-many-yaml-nodes]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: ./build.sh
        env: &env
          CI: true
      - run: ./test.sh
        env: *env
```

## Differences from actionlint

- [Workflows over GitHub's YAML node limit](../differences.md#yaml-node-limit)
