# if-always-true

| Kind | Category | Default severity |
|---|---|---|
| `if-cond` | correctness | error |

An `if:` condition has text around its `${{ }}`, so it is always true.

A condition with any text before or after its `${{ }}`, or with more than one `${{ }}`, gets this error. Spaces and the line break at the end of a `|` block scalar count as text. When the placeholders are joined by operators, as in `${{ success() }} || ${{ failure() }}`, the message suggests the single `${{ }}` that puts them together. There is no suggestion when a placeholder is inside a quoted string or part of a longer name, as in `'refs/heads/${{ env.BRANCH }}'` or `steps.${{ env.ID }}.outcome`, because joining it with the text around it would change the condition.

## Why

GitHub evaluates a condition with text around its `${{ }}` as a string template. The result is a non-empty string, which is true, so the job or step always runs.

## Failing examples

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo 'Commit is pushed to main'
        if: ${{ github.event_name == 'push' }} && ${{ github.ref_name == 'main' }}
```

```text
test.yaml:7:13: if: condition "${{ github.event_name == 'push' }} && ${{ github.ref_name == 'main' }}" is always evaluated to true because extra characters are around ${{ }}. did you mean "${{ github.event_name == 'push' && github.ref_name == 'main' }}"? [if-always-true]
```

A placeholder inside a quoted string gets no suggestion. Compare against `format()` instead:

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo 'Commit is pushed to the branch'
        if: github.ref == 'refs/heads/${{ env.BRANCH }}'
```

```text
test.yaml:7:13: if: condition "github.ref == 'refs/heads/${{ env.BRANCH }}'" is always evaluated to true because extra characters are around ${{ }} [if-always-true]
```

A `|` block scalar ends with a line break, which is text after the `${{ }}`:

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo 'Commit is pushed'
        if: |
          ${{ github.event_name == 'push' }}
```

```text
test.yaml:7:13: if: condition "${{ github.event_name == 'push' }}\n" is always evaluated to true because extra characters are around ${{ }} [if-always-true]
```

## Passing examples

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo 'Commit is pushed to main'
        if: ${{ github.event_name == 'push' && github.ref_name == 'main' }}
```

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo 'Commit is pushed to main'
        if: github.event_name == 'push' && github.ref_name == 'main'
```

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo 'Commit is pushed to the branch'
        if: github.ref == format('refs/heads/{0}', env.BRANCH)
```

## Differences from actionlint

- [`}}` inside a string literal in `if:`](../differences.md#if-cond-closing-braces)
- [Always-true conditions suggest one `${{ }}`](../differences.md#if-cond-suggestion)
