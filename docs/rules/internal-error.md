# internal-error

| Kind | Category | Default severity |
|---|---|---|
| `internal-error` | correctness | error |

workflint stopped linting a file because of a bug.

The driver reports it when the lint of a file panics, with the panic message, or when a panic while linting another file left the cache of local actions and reusable workflows unusable. It points at line 1, column 1 of the file. The file's other diagnostics are missing, the other files are linted as usual, and workflint exits with code 3 after printing every file's output. `internal-error` is not an actionlint kind, because a panic stops actionlint. A panic also prints a message with its source location to standard error.

## Why

No input should make workflint panic (ADR 0007). A panic is a bug, and reporting it on the file it hit keeps the results of every other file. Please report it at https://github.com/stokehold/workflint/issues with the file that caused it.

## Unreachable

No workflow triggers it. The rule reports only a bug in workflint, and the bugs found so far are fixed.

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

## Differences from actionlint

- [No panic on NaN](../differences.md#nan-panic)
