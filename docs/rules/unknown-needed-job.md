# unknown-needed-job

| Kind | Category | Default severity |
|---|---|---|
| `job-needs` | correctness | error |

A job lists a job in `needs` that the workflow does not have.

The rule reports at the ID of the job that has the `needs`, once for each unknown entry. The message has both IDs in lowercase, because job IDs are compared without case. When any entry is unknown, the rule does not look for [cycles](cyclic-needs.md).

## Why

GitHub refuses to run a workflow in which a job depends on a job that does not exist.

## Failing example

```yaml
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: make
  test:
    needs: [build, Setup]
    runs-on: ubuntu-latest
    steps:
      - run: make test
```

```text
test.yaml:7:3: job "test" needs job "setup" which does not exist in this workflow [unknown-needed-job]
```

## Passing example

```yaml
on: push
jobs:
  setup:
    runs-on: ubuntu-latest
    steps:
      - run: make setup
  test:
    needs: [Setup]
    runs-on: ubuntu-latest
    steps:
      - run: make test
```
