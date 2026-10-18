# invalid-glob

| Kind | Category | Default severity |
|---|---|---|
| `glob` | correctness | error |

A filter pattern is not valid glob syntax, or contains a newline.

The rule checks the patterns of `branches`, `branches-ignore`, `tags`, `tags-ignore`, `paths`, and `paths-ignore` of every event, the `versions` of `image_version`, and `snapshot.version` of each job. It skips an empty pattern, which [empty-string](empty-string.md) reports. A `${{ }}` in a pattern is checked as plain text, because GitHub does not evaluate expressions in filters.

It reports an empty character match `[]`, a `[` without its `]`, a range with no end or with its start after its end, a character match of one character, `?` or `+` after another special character or at the start, a `!` with nothing after it, a newline, and a NUL character. Each message ends with a link to GitHub's filter pattern cheat sheet.

## Why

GitHub's filter pattern syntax does not define these patterns, so it is unclear which branches, tags, or paths they match. The workflow may run for refs or paths the author did not mean, or for none. A one-character match such as `[x]` is valid but is just `x` written the long way.

## Failing example

```yaml
on:
  push:
    tags:
      - 'v*+'
      - 'v[9-1]'
    paths:
      - 'src/[x].txt'
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo
```

```text
test.yaml:4:12: invalid glob pattern. unexpected character '+' while checking special character + (one or more). the preceding character must not be special character. note: filter pattern syntax is explained at https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#filter-pattern-cheat-sheet [invalid-glob]
test.yaml:5:14: invalid glob pattern. unexpected character '1' while checking character range in []. start of range '9' (57) is larger than end of range '1' (49). note: filter pattern syntax is explained at https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#filter-pattern-cheat-sheet [invalid-glob]
test.yaml:7:16: invalid glob pattern. unexpected character ']' while checking character match []. character match with single character is useless. simply use x instead of [x]. note: filter pattern syntax is explained at https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#filter-pattern-cheat-sheet [invalid-glob]
```

## Passing example

```yaml
on:
  push:
    tags:
      - 'v*'
      - 'v[1-9]'
    paths:
      - 'src/**/*.txt'
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo
```

## Differences from actionlint

- [Exact positions inside multi-line and quoted strings](../differences.md#positions-inside-scalars)
