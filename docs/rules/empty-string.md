# empty-string

| Kind | Category | Default severity |
|---|---|---|
| `syntax-check` | correctness | error |

A string that must not be empty is empty.

`run: ~` and `run: null` are not empty. They are the strings `~` and `null`.

## Why

An empty value such as `run: ''` or `runs-on: ''` gives the step nothing to run, or the job no runner to run on.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - name: Test
        run: ''
```

```text
test.yaml:7:14: string should not be empty [empty-string]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - name: Test
        run: ./test.sh
```

## Differences from actionlint

- [An invalid value is reported once](../differences.md#invalid-value-reported-once)
