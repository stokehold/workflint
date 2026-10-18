# constant-condition

| Kind | Category | Default severity |
|---|---|---|
| `if-cond` | correctness | error |

An `if:` condition has the same value in every run.

The condition reads no context and calls only functions whose result depends on their arguments alone, such as `contains` and `format`. `success()`, `always()`, and `hashFiles()` are not constant. The check runs on a condition written without `${{ }}` and on one written as a single `${{ }}`.

## Why

A condition that is always true does nothing, and one that is always false turns the job or step off. Either way the `if:` is a leftover from debugging or a mistake.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo "DEBUG! ${{ github.event_name }}"
        if: false
      - run: echo 'Always true'
        if: ${{ contains(format('{0} {1}', 'foo', 'bar'), 'o b') }}
```

```text
test.yaml:7:13: constant expression "false" in condition. remove the if: section [constant-condition]
test.yaml:9:13: constant expression "contains(format('{0} {1}', 'foo', 'bar'), 'o b')" in condition. remove the if: section [constant-condition]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo "DEBUG! ${{ github.event_name }}"
        if: runner.debug == '1'
```
