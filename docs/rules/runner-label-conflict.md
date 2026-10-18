# runner-label-conflict

| Kind | Category | Default severity |
|---|---|---|
| `runner-label` | correctness | error |

Two `runs-on` labels of a job cannot be on the same runner.

Each GitHub-hosted label, and each of the self-hosted labels `linux`, `macos`, and `windows`, stands for a set of runner images. A label conflicts with an earlier label of the job when their sets share no image, as `ubuntu-latest` and `windows-latest` do, or `ubuntu-24.04` and `ubuntu-22.04`. The message names the earliest label it conflicts with. The values of a `${{ matrix.<prop> }}` label are checked against the labels before it, so they do not conflict with each other.

## Why

A job runs on one runner that has all of its labels. No runner has two operating systems, so GitHub queues the job until it times out. A job meant to run on several systems needs a matrix.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: [ubuntu-latest, windows-latest, macos-latest]
    steps:
      - run: echo
```

```text
test.yaml:4:30: label "windows-latest" conflicts with label "ubuntu-latest" defined at line:4,col:15. note: to run your job on each workers, use matrix [runner-label-conflict]
test.yaml:4:46: label "macos-latest" conflicts with label "ubuntu-latest" defined at line:4,col:15. note: to run your job on each workers, use matrix [runner-label-conflict]
```

## Passing example

```yaml
on: push
jobs:
  test:
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - run: echo
  build:
    runs-on: [self-hosted, linux, ubuntu-24.04]
    steps:
      - run: echo
```

## Differences from actionlint

- [Deterministic job order](../differences.md#deterministic-order)
