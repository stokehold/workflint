# duplicate-needs

| Kind | Category | Default severity |
|---|---|---|
| `job-needs` | correctness | error |

A job lists the same job twice in `needs`, ignoring case.

The rule reports each entry of `needs` that repeats an earlier entry. Job IDs are compared without case, so `build` and `BUILD` are the same job.

## Why

The second entry adds nothing, because the job already waits for that job. It is usually a typo for a job that the author meant to list.

## Failing example

```yaml
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: make
  test:
    needs: [build, BUILD]
    runs-on: ubuntu-latest
    steps:
      - run: make test
```

```text
test.yaml:8:20: job ID "BUILD" duplicates in "needs" section. note that job ID is case insensitive [duplicate-needs]
```

## Passing example

```yaml
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: make
  test:
    needs: [build]
    runs-on: ubuntu-latest
    steps:
      - run: make test
```
