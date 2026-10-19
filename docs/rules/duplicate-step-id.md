# duplicate-step-id

| Kind | Category | Default severity |
|---|---|---|
| `id` | correctness | error |

Two steps of one job have the same ID, ignoring case.

The rule reports each step whose `id` repeats the `id` of an earlier step in the same job, and names the position of the first. Steps of different jobs may share an ID.

## Why

GitHub rejects a workflow in which two steps of a job have the same `id`. It compares step IDs without case, so `build` and `BUILD` are the same ID, and `steps.build` could not tell them apart.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
        id: step_id
      - run: echo bye
        id: STEP_ID
```

```text
test.yaml:9:13: step ID "STEP_ID" duplicates. previously defined at line:7,col:13. step ID must be unique within a job. note that step ID is case insensitive [duplicate-step-id]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
        id: hello
      - run: echo bye
        id: bye
  other:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
        id: hello
```
