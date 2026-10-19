# invalid-timezone

| Kind | Category | Default severity |
|---|---|---|
| `events` | correctness | error |

A `schedule` time zone is not an IANA time zone name.

workflint accepts the names in the IANA time zone database that it has built in, such as `America/New_York`. Names are case-sensitive. `UTC` and `Local` are rejected in any case, as actionlint rejects them. Write `Etc/UTC` for UTC.

## Why

GitHub accepts only IANA time zone names in `timezone`.

## Failing example

```yaml
on:
  schedule:
    - cron: '30 5 * * *'
      timezone: 'Asia/Somewhere'
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

```text
test.yaml:4:17: invalid timezone "Asia/Somewhere" in schedule event. it must be a valid IANA timezone name [invalid-timezone]
```

## Passing example

```yaml
on:
  schedule:
    - cron: '30 5 * * *'
      timezone: 'America/New_York'
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

## Differences from actionlint

- [Schedule time zones do not depend on the host](../differences.md#host-time-zones)
