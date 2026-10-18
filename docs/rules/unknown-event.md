# unknown-event

| Kind | Category | Default severity |
|---|---|---|
| `events` | correctness | error |

`on` names an event that is not a GitHub webhook event.

Event names are case-sensitive, so `Push` is unknown.

## Why

GitHub runs a workflow only for the events it knows. A misspelled event never fires, so the workflow never runs for it.

## Failing example

```yaml
on:
  pullreq:
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

```text
test.yaml:2:3: unknown Webhook event "pullreq". see https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#webhook-events for list of all Webhook event names [unknown-event]
```

## Passing example

```yaml
on:
  pull_request:
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

## Differences from actionlint

- [An invalid value is reported once](../differences.md#invalid-value-reported-once)
