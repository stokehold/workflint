# cyclic-needs

| Kind | Category | Default severity |
|---|---|---|
| `job-needs` | correctness | error |

The jobs' `needs` make a cycle.

The rule reports one cycle, the first that a depth-first search finds when it starts from each job in source order and follows `needs` in the order written. The message lists the jobs of the cycle in lowercase, starting at the job written first, and the rule reports at that job's ID. A job that lists itself in `needs` is a cycle of one job. The rule does not look for cycles when a `needs` entry names an [unknown job](unknown-needed-job.md).

## Why

No job of the cycle can start, because each waits for another job of the cycle to finish. GitHub refuses to run the workflow.

## Failing example

```yaml
on: push
jobs:
  prepare:
    needs: [build]
    runs-on: ubuntu-latest
    steps:
      - run: echo prepare
  install:
    needs: [prepare]
    runs-on: ubuntu-latest
    steps:
      - run: echo install
  build:
    needs: [install]
    runs-on: ubuntu-latest
    steps:
      - run: echo build
```

```text
test.yaml:3:3: cyclic dependencies in "needs" job configurations are detected. detected cycle is "prepare" -> "build" -> "install" -> "prepare" [cyclic-needs]
```

## Passing example

```yaml
on: push
jobs:
  prepare:
    runs-on: ubuntu-latest
    steps:
      - run: echo prepare
  install:
    needs: [prepare]
    runs-on: ubuntu-latest
    steps:
      - run: echo install
  build:
    needs: [install]
    runs-on: ubuntu-latest
    steps:
      - run: echo build
```

## Differences from actionlint

- [Deterministic job order](../differences.md#deterministic-order): when the jobs have several cycles, actionlint reports whichever its random map order finds first.
