# recursive-alias

| Kind | Category | Default severity |
|---|---|---|
| `syntax-check` | correctness | error |

An alias refers to a node that encloses it.

The alias stays unresolved, so the section that contains it also gets an error about its node kind from [unexpected-node-kind](unexpected-node-kind.md).

## Why

Expanding the alias would never end, so the workflow has no meaning. GitHub rejects it.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - &step
        run: ./build.sh
        env: *step
```

```text
test.yaml:8:14: recursive alias "step" is found. anchor was declared at line:6, column:9 [recursive-alias]
test.yaml:8:14: "env" section is alias node but mapping node is expected [unexpected-node-kind]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - &build
        run: ./build.sh
      - *build
```
