# invalid-input-default

| Kind | Category | Default severity |
|---|---|---|
| `events` | correctness | error |

The default of a `number` or `boolean` input of `workflow_call` or `workflow_dispatch` does not match its type.

A `number` default must parse as a float, with the syntax that Go's `strconv.ParseFloat` accepts. A `boolean` default must be `true` or `false`, in any case. A `workflow_call` default that contains `${{ }}` is not checked, because GitHub evaluates it. A `workflow_dispatch` default is checked as written.

## Why

The default is the input's value when no value is given. A default that does not parse as the input's type gives the workflow a value of the wrong type.

## Failing examples

```yaml
on:
  workflow_call:
    inputs:
      port:
        type: number
        default: ':8080'
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

```text
test.yaml:6:18: input of workflow_call event "port" is typed as number but its default value ":8080" cannot be parsed as a float number: strconv.ParseFloat: parsing ":8080": invalid syntax [invalid-input-default]
```

```yaml
on:
  workflow_dispatch:
    inputs:
      dry-run:
        type: boolean
        default: 'no'
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

```text
test.yaml:6:18: type of "dry-run" input is "boolean". its default value "no" must be "true" or "false" [invalid-input-default]
```

## Passing example

```yaml
on:
  workflow_call:
    inputs:
      port:
        type: number
        default: 8080
      dry-run:
        type: boolean
        default: ${{ github.event_name != 'push' }}
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

## Differences from actionlint

- [An invalid value is reported once](../differences.md#invalid-value-reported-once)
