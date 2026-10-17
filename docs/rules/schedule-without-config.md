# schedule-without-config

| Kind | Category | Default severity |
|---|---|---|
| `syntax-check` | correctness | error |

`schedule` is named in a string or a sequence item of `on`, so it has no `cron` entries.

`schedule` needs a mapping under `on` with a list of `cron` entries.

## Why

A `schedule` trigger without `cron` entries never runs the workflow, and GitHub rejects it.

## Failing example

```yaml
on: [push, schedule]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: ./test.sh
```

```text
test.yaml:1:12: schedule event must be configured with mapping [schedule-without-config]
```

## Passing example

```yaml
on:
  push:
  schedule:
    - cron: '0 3 * * 1'
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: ./test.sh
```
