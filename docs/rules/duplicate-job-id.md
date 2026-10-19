# duplicate-job-id

| Kind | Category | Default severity |
|---|---|---|
| `job-needs` | correctness | error |

Two jobs have the same ID, ignoring case.

## Why

GitHub compares job IDs without case, so two jobs named `test` and `TEST` would be the same entry of `needs` and of the `needs` context.

## Unreachable

The rule never reports. The parser reports a job whose ID repeats an earlier one as [duplicate-key](duplicate-key.md) and drops it, so the rule sees each job ID once. actionlint has the same check, and it does not report there either. The rule's unit tests pass the same job to it twice.

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: make test
  lint:
    runs-on: ubuntu-latest
    steps:
      - run: make lint
```
