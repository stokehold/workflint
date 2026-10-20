# runner-label-conflict

| Kind | Category | Default severity |
|---|---|---|
| `runner-label` | correctness | error |

Two `runs-on` labels of a job cannot be on the same runner.

Each GitHub-hosted label, and each of the self-hosted labels `linux`, `macos`, and `windows`, stands for a set of runner images. A label conflicts with an earlier label of the job when their sets share no image, as `ubuntu-latest` and `windows-latest` do, or `ubuntu-24.04` and `ubuntu-22.04`. A `-latest` label stands for the image that GitHub runs it on, so `windows-latest` and `windows-2022` conflict. While GitHub moves a label to a new image, or when GitHub's pages disagree about its image, the label stands for both images: `ubuntu-latest` is Ubuntu 24.04 or 26.04, and `macos-latest-xlarge` is macOS 15 or 26 on an extra-large runner. The message names the earliest label it conflicts with. The values of a label with `${{ }}`, such as `${{ matrix.os }}` or `${{ matrix.os }}-latest`, are checked against the labels before it, so they do not conflict with each other. [`unknown-runner-label`](unknown-runner-label.md) describes how such a label gets its values.

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
- [Expressions in `runs-on` labels are evaluated](../differences.md#runs-on-expressions)
