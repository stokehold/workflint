# unmatched-exclude-value

| Kind | Category | Default severity |
|---|---|---|
| `matrix` | correctness | error |

An `exclude` value matches no value of its matrix row.

A mapping in `exclude` matches a row mapping that has each of its keys with a matching value, so `{ runner: windows-latest }` matches `{ name: Windows, runner: windows-latest }`. A sequence matches a sequence of the same length, item by item. Scalars compare by their text. A scalar with a `${{ }}` expression matches anything, on either side, because its value is only known when the workflow runs. A value that only `include` adds is not a row value.

## Why

GitHub applies `exclude` to the combinations of the rows before it adds `include`. A value that no row has matches no combination, so the entry removes nothing, and the jobs it was meant to remove still run.

## Failing example

```yaml
on: push
jobs:
  test:
    strategy:
      matrix:
        os:
          - { name: Linux, runner: ubuntu-latest }
          - { name: Windows, runner: windows-latest }
        node: [22, 24]
        include:
          - os: { name: macOS, runner: macos-latest }
            node: 24
        exclude:
          - os: { runner: windows-latest }
            node: 20
          - os: { runner: macos-latest }
    runs-on: ${{ matrix.os.runner }}
    steps:
      - run: npm test
```

```text
test.yaml:15:19: value "20" in "exclude" does not match in matrix "node" combinations. possible values are "22", "24" [unmatched-exclude-value]
test.yaml:16:17: value {"runner": "macos-latest"} in "exclude" does not match in matrix "os" combinations. possible values are {"name": "Linux", "runner": "ubuntu-latest"}, {"name": "Windows", "runner": "windows-latest"} [unmatched-exclude-value]
```

## Passing example

```yaml
on: push
jobs:
  test:
    strategy:
      matrix:
        os:
          - { name: Linux, runner: ubuntu-latest }
          - { name: Windows, runner: windows-latest }
        node: [22, 24]
        exclude:
          - os: { runner: windows-latest }
            node: 22
          - os: { runner: ubuntu-latest }
            node: ${{ vars.SKIPPED_NODE }}
    runs-on: ${{ matrix.os.runner }}
    steps:
      - run: npm test
```

## Differences from actionlint

- [`exclude` is matched against the matrix rows only](../differences.md#matrix-exclude-before-include)
