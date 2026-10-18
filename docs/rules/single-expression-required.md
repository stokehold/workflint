# single-expression-required

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

A value that must be one `${{ }}` holds another number of them.

## Why

A typed key, such as `continue-on-error`, takes either a literal or one `${{ }}` that GitHub evaluates to the value. The rule exists for parity with actionlint, which has the same check.

## Unreachable

The rule never reports. The parser reports a value of a typed key that is other text as [expected-single-expression](expected-single-expression.md), and it stores a whole-expression value only when the value holds one `${{ }}`. So the `expression` rule never sees a value with no expression or with two.

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    continue-on-error: ${{ github.event_name == 'schedule' }}
    steps:
      - run: ./test.sh
```
