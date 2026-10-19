# duplicate-matrix-value

| Kind | Category | Default severity |
|---|---|---|
| `matrix` | correctness | error |

A matrix row has the same value twice.

The message names the first copy. Scalars compare by their text, so `22`, `"22"`, and `'22'` are the same value. Mappings compare by their keys without case, in any order, and sequences compare item by item. A row written as one `${{ }}` expression is not checked.

## Why

A duplicate value doubles every job that uses it. The second set of jobs runs the same configuration again and only costs runner time.

## Failing example

```yaml
on: push
jobs:
  test:
    strategy:
      matrix:
        node: [20, 22, 24, 22]
        os:
          - { name: Linux, runner: ubuntu-latest }
          - { runner: ubuntu-latest, name: Linux }
    runs-on: ${{ matrix.os.runner }}
    steps:
      - run: npm test
```

```text
test.yaml:6:28: duplicate value "22" is found in matrix "node". the same value is at line:6,col:20 [duplicate-matrix-value]
test.yaml:9:13: duplicate value {"name": "Linux", "runner": "ubuntu-latest"} is found in matrix "os". the same value is at line:8,col:13 [duplicate-matrix-value]
```

## Passing example

```yaml
on: push
jobs:
  test:
    strategy:
      matrix:
        node: [20, 22, 24]
        os:
          - { name: Linux, runner: ubuntu-latest }
          - { name: Windows, runner: windows-latest }
    runs-on: ${{ matrix.os.runner }}
    steps:
      - run: npm test
```
