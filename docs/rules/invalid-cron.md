# invalid-cron

| Kind | Category | Default severity |
|---|---|---|
| `events` | correctness | error |

A `schedule` cron expression does not parse.

The expression has five fields: minute, hour, day of the month, month, and day of the week. A field is a comma-separated list of numbers, ranges such as `1-5`, or `*`, each with an optional step such as `*/15`. Months and days of the week also take the names `jan` to `dec` and `sun` to `sat`, in any case. The expression may start with `TZ=` or `CRON_TZ=` and a time zone name. Descriptors such as `@daily` are rejected. The message ends with the parser's error, which says which field is wrong.

## Why

GitHub does not run a schedule whose cron expression it cannot parse.

## Failing examples

```yaml
on:
  schedule:
    - cron: '0 */3 * *'
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

```text
test.yaml:3:13: invalid CRON format "0 */3 * *" in schedule event: expected exactly 5 fields, found 4: [0 */3 * *] [invalid-cron]
```

The day of the week goes from 0 for Sunday to 6 for Saturday:

```yaml
on:
  schedule:
    - cron: '0 0 * * 7'
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

```text
test.yaml:3:13: invalid CRON format "0 0 * * 7" in schedule event: end of range (7) above maximum (6): 7 [invalid-cron]
```

## Passing example

```yaml
on:
  schedule:
    - cron: '30 5 * * mon-fri'
    - cron: '0 */6 1,15 * *'
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

## Differences from actionlint

- [Schedule time zones do not depend on the host](../differences.md#host-time-zones)
- [A cron time zone prefix without fields](../differences.md#cron-time-zone-without-fields)
