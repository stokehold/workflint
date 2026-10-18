# invalid-ref-name

| Kind | Category | Default severity |
|---|---|---|
| `glob` | correctness | error |

A branch or tag pattern has a character that a Git ref name cannot have.

The rule checks the patterns of `branches`, `branches-ignore`, `tags`, and `tags-ignore` of every event, the `versions` of `image_version`, and `snapshot.version` of each job. A pattern must not contain a space, a tab, `~`, `^`, or `:`, must not start with `/`, and must not end with `/` or `.`. A `\` may escape only `[`, `?`, `+`, `*`, `\`, and `!`, and an escaped `[`, `?`, or `*` is reported too, because a ref name cannot contain it. It skips an empty pattern, which [empty-string](empty-string.md) reports.

## Why

`git check-ref-format` rejects these names, so no branch or tag can match the pattern. The usual cause is a regular expression, such as `^release-` or `/v\d+/`, where GitHub expects a glob.

## Failing example

```yaml
on:
  push:
    branches:
      - '^release-'
    tags:
      - /v\d+/
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo
```

```text
test.yaml:4:10: character '^' is invalid for branch and tag names. ref name cannot contain spaces, ~, ^, :, [, ?, *. see `man git-check-ref-format` for more details. note that regular expression is unavailable. note: filter pattern syntax is explained at https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#filter-pattern-cheat-sheet [invalid-ref-name]
test.yaml:6:9: character '/' is invalid for branch and tag names. ref name must not start with /. see `man git-check-ref-format` for more details. note that regular expression is unavailable. note: filter pattern syntax is explained at https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#filter-pattern-cheat-sheet [invalid-ref-name]
test.yaml:6:11: character '\' is invalid for branch and tag names. only special characters [, ?, +, *, \, ! can be escaped with \. see `man git-check-ref-format` for more details. note that regular expression is unavailable. note: filter pattern syntax is explained at https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#filter-pattern-cheat-sheet [invalid-ref-name]
test.yaml:6:14: character '/' is invalid for branch and tag names. ref name must not end with / and .. see `man git-check-ref-format` for more details. note that regular expression is unavailable. note: filter pattern syntax is explained at https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#filter-pattern-cheat-sheet [invalid-ref-name]
```

## Passing example

```yaml
on:
  push:
    branches:
      - 'release-*'
    tags:
      - 'v[0-9]+'
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo
```

## Differences from actionlint

- [Exact positions inside multi-line and quoted strings](../differences.md#positions-inside-scalars)
- [Escaped characters in branch and tag globs](../differences.md#glob-escape-character)
