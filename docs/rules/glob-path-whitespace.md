# glob-path-whitespace

| Kind | Category | Default severity |
|---|---|---|
| `glob` | correctness | error |

A path pattern has white space at either end.

The rule checks the patterns of `paths` and `paths-ignore` of every event. White space includes a line break, so the final line feed of a `|` block counts. The rule reports the problem at the first character of the pattern. A pattern with white space at either end gets no [invalid-glob](invalid-glob.md) report.

## Why

GitHub compares the pattern with paths as written, white space included, and paths in a repository rarely start or end with white space. The pattern then matches no changed file, and the workflow does not run.

## Failing example

```yaml
on:
  push:
    paths:
      - ' src/**'
      - |
        docs/**
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo
```

```text
test.yaml:4:10: leading and trailing spaces are not allowed in glob path. note: filter pattern syntax is explained at https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#filter-pattern-cheat-sheet [glob-path-whitespace]
test.yaml:6:9: leading and trailing spaces are not allowed in glob path. note: filter pattern syntax is explained at https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#filter-pattern-cheat-sheet [glob-path-whitespace]
```

## Passing example

```yaml
on:
  push:
    paths:
      - 'src/**'
      - |-
        docs/**
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo
```

## Differences from actionlint

- [Exact positions inside multi-line and quoted strings](../differences.md#positions-inside-scalars)
