# invalid-activity-type

| Kind | Category | Default severity |
|---|---|---|
| `events` | correctness | error |

`types` lists an activity type that the event does not have.

The message lists the event's activity types. They are case-sensitive.

## Why

GitHub runs the workflow only for the listed activity types. A type that the event does not have never matches, so the workflow does not run when the author expects it to.

## Failing example

```yaml
on:
  issues:
    types: [created]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

```text
test.yaml:3:13: invalid activity type "created" for "issues" Webhook event. available types are "assigned", "closed", "deleted", "demilestoned", "edited", "labeled", "locked", "milestoned", "opened", "pinned", "reopened", "transferred", "typed", "unassigned", "unlabeled", "unlocked", "unpinned", "untyped" [invalid-activity-type]
```

## Passing example

```yaml
on:
  issues:
    types: [opened]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

## Differences from actionlint

- [An invalid value is reported once](../differences.md#invalid-value-reported-once)
