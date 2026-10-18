# glob-path-dot

| Kind | Category | Default severity |
|---|---|---|
| `glob` | correctness | error |

A path pattern is `.` or `..`, or starts with `./` or `../`.

The rule checks the patterns of `paths` and `paths-ignore` of every event, after a leading `!` too. It reports the problem at the first character of the pattern. A pattern with this problem gets no [invalid-glob](invalid-glob.md) report.

## Why

GitHub matches path patterns against paths relative to the repository root, such as `src/main.rs`, which never start with `./`. A pattern that starts with `./` matches no file.

## Failing example

```yaml
on:
  push:
    paths:
      - ./src/**
      - '!../docs/**'
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo
```

```text
test.yaml:4:9: '.' and '..' are not allowed in glob path. note: filter pattern syntax is explained at https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#filter-pattern-cheat-sheet [glob-path-dot]
test.yaml:5:10: '.' and '..' are not allowed in glob path. note: filter pattern syntax is explained at https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#filter-pattern-cheat-sheet [glob-path-dot]
```

## Passing example

```yaml
on:
  push:
    paths:
      - src/**
      - '!docs/**'
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo
```
