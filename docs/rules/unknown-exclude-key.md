# unknown-exclude-key

| Kind | Category | Default severity |
|---|---|---|
| `matrix` | correctness | error |

An `exclude` entry names a key that is not a matrix row.

Keys compare without case, and the message lists the rows in lower case. A key that only `include` adds is not a row. A row written as one `${{ }}` expression is a row, but its values are not checked and the message does not list it.

## Why

GitHub applies `exclude` to the combinations of the rows before it adds `include`. An entry with a key that no row has matches no combination, so it removes nothing. The key is often misspelled, or the entry was meant for a combination that `include` adds, which `exclude` cannot remove.

## Failing example

```yaml
on: push
jobs:
  test:
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest]
        node: [22, 24]
        include:
          - os: ubuntu-latest
            experimental: true
        exclude:
          - platform: windows-latest
          - os: ubuntu-latest
            experimental: true
    runs-on: ${{ matrix.os }}
    steps:
      - run: npm test
```

```text
test.yaml:12:13: "platform" in "exclude" section does not exist in matrix. available matrix configurations are "node", "os" [unknown-exclude-key]
test.yaml:14:13: "experimental" in "exclude" section does not exist in matrix. available matrix configurations are "node", "os" [unknown-exclude-key]
```

## Passing example

```yaml
on: push
jobs:
  test:
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest]
        node: [22, 24]
        exclude:
          - OS: windows-latest
            node: 22
    runs-on: ${{ matrix.os }}
    steps:
      - run: npm test
```

## Differences from actionlint

- [`exclude` is matched against the matrix rows only](../differences.md#matrix-exclude-before-include)
- [`${{ insert }}` as a mapping key](../differences.md#insert-mapping-key)
