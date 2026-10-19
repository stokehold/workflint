# exclude-without-matrix

| Kind | Category | Default severity |
|---|---|---|
| `matrix` | correctness | error |

`exclude` is written in a matrix that has no rows.

The rows are the keys of `matrix` other than `include` and `exclude`. A row written as one `${{ }}` expression counts as a row. An `exclude` written as one `${{ }}` expression is not checked.

## Why

GitHub applies `exclude` to the combinations of the rows, and adds the `include` combinations afterwards. With no rows, `exclude` has nothing to remove, and the jobs that `include` adds all run.

## Failing example

```yaml
on: push
jobs:
  test:
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            node: 24
          - os: windows-latest
            node: 22
        exclude:
          - os: windows-latest
    runs-on: ${{ matrix.os }}
    steps:
      - run: npm test
```

```text
test.yaml:5:7: "exclude" section exists but no matrix variation exists [exclude-without-matrix]
```

## Passing example

```yaml
on: push
jobs:
  test:
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            node: 24
    runs-on: ${{ matrix.os }}
    steps:
      - run: npm test
```

## Differences from actionlint

- [`exclude` is matched against the matrix rows only](../differences.md#matrix-exclude-before-include)
