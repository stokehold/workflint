# deprecated-action-input

| Kind | Category | Default severity |
|---|---|---|
| `action` | correctness | error |

`with:` sets an input that the action has deprecated.

An input is deprecated when the action's metadata gives it a `deprecationMessage`. This covers popular actions and local actions. The error ends with that message, joined onto one line. An input that is both deprecated and required is not reported, because a step cannot leave it out.

## Why

A deprecated input may stop working in a later version of the action. The message usually names the input to use instead.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: reviewdog/action-actionlint@v1
        with:
          fail_on_error: true
```

```text
test.yaml:8:11: avoid using deprecated input "fail_on_error" in action "reviewdog/action-actionlint@v1": Deprecated, use `fail_level` instead [deprecated-action-input]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: reviewdog/action-actionlint@v1
        with:
          fail_level: error
```
