# cron-too-frequent

| Kind | Category | Default severity |
|---|---|---|
| `events` | correctness | error |

A `schedule` cron expression runs more often than every 5 minutes.

The rule compares the first two runs after January 1, 1970, 00:00 UTC, and prints the seconds between them. It computes both runs in UTC on every host. actionlint computes them in the host's time zone, or in the zone of a `TZ=` prefix, so it can give a schedule whose runs are unevenly spaced a different interval. A schedule that never runs, such as `0 0 30 2 *` for February 30, has an interval of 0 seconds.

## Why

The shortest interval that GitHub runs a scheduled workflow at is once every 5 minutes.

## Failing example

```yaml
on:
  schedule:
    - cron: '*/4 * * * *'
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

```text
test.yaml:3:13: scheduled job runs too frequently. it runs once per 240 seconds. the shortest interval is once every 5 minutes [cron-too-frequent]
```

## Passing example

```yaml
on:
  schedule:
    - cron: '*/5 * * * *'
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

## Differences from actionlint

- [Schedule intervals are computed in UTC](../differences.md#cron-interval-in-utc)
