# unmatched-closing-braces

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | warning |

A `}}` in a string that GitHub evaluates has no opening `${{`.

The rule checks the strings where GitHub evaluates `${{ }}`, such as `run`, `name`, `env` values, `with` inputs, and `if`. It finds the end of each `${{ }}` as GitHub does: at the first `}}` after `${{` that is not inside a single-quoted string. A `}}` outside every `${{ }}` is reported unless one of its braces closes a `{` earlier in the string. So `docker inspect --format '{{.Id}}'`, Go and Jinja templates, and JSON such as `{"a": {"b": 1}}` give no warning.

In an `if` condition written without `${{ }}`, a `}}` outside a string literal is reported.

The default severity is warning, because the braces may be text that the author meant to write.

## Why

GitHub keeps a `}}` that has no opening `${{` as text, so the expression it was meant to end is never evaluated. `run: echo secrets.TOKEN }}` prints `secrets.TOKEN }}`. In an `if` condition, the text makes the condition a non-empty string, which is always true. In an `if` condition without `${{ }}`, GitHub rejects the `}}`.

## Failing examples

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo secrets.TOKEN }}
      - run: echo ${{ github.ref }} }}
```

```text
test.yaml:6:33: "}}" has no opening "${{". if it ends an expression, add the missing "${{" [unmatched-closing-braces]
test.yaml:7:37: "}}" has no opening "${{". if it ends an expression, add the missing "${{" [unmatched-closing-braces]
```

The first line of this condition closes its `${{ }}`, so GitHub reads the second line as text, and the condition is always true. The rule [if-always-true](if-always-true.md) reports that too:

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo deploy
        if: |
          ${{ github.event_name == 'push' }}
          || github.ref_name == 'main' }}
```

```text
test.yaml:7:13: if: condition "${{ github.event_name == 'push' }}\n|| github.ref_name == 'main' }}\n" is always evaluated to true because extra characters are around ${{ }} [if-always-true]
test.yaml:9:40: "}}" has no opening "${{". if it ends an expression, add the missing "${{" [unmatched-closing-braces]
```

## Passing examples

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo ${{ secrets.TOKEN }}
      - run: echo deploy
        if: ${{ github.event_name == 'push' || github.ref_name == 'main' }}
```

A `}}` inside a string literal of an expression, and braces that the string opens, are not reported:

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo ${{ github.ref == '}}' }}
      - run: |
          docker inspect --format '{{.State.Status}}' app
          echo '{"sha": {"value": "${{ github.sha }}"}}'
```

## Differences from actionlint

- [A `}}` with no opening `${{`](../differences.md#unmatched-closing-braces)
