# pyflakes-issue

| Kind | Category | Default severity |
|---|---|---|
| `pyflakes` | correctness | error |

pyflakes or ruff reports a problem in a Python `run:` script.

workflint checks each `run:` script whose shell is `python`, as actionlint does with [pyflakes](https://github.com/PyCQA/pyflakes). The shell is the step's `shell`, else the job's `defaults.run.shell`, else the workflow's `defaults.run.shell`. A `shell` of `python` followed by a space, such as `python {0}`, counts too. The runner does not matter, and the match is case-sensitive, so `Python` and `python3` are not checked.

Before the linter reads the script on stdin, each `${{ }}` becomes as many `_` characters as it has bytes. The config key [`python.linter`](../config.md#pythonlinter) picks the linter:

- `auto`, the default, runs [ruff](https://docs.astral.sh/ruff/) if it is on `PATH`, and pyflakes otherwise.
- `ruff` runs `ruff check --isolated --no-cache --select F --output-format json -`: only the pyflakes rules, and no ruff config file.
- `pyflakes` runs `pyflakes` without arguments, as actionlint does.
- `off` checks nothing.

`-pyflakes=PATH` runs that pyflakes whatever `python.linter` says, and `-pyflakes=` turns the rule off ([command line](../cli.md)). Identical scripts run the linter once per run, and `-jobs` sets how many tool processes run at once.

The message is actionlint's: `pyflakes reported issue in this script:`, then the line and column in the script and the linter's message. With ruff, the message has ruff's rule code and wording, such as `1:7: F821 Undefined name `hello``. The diagnostic points at that line and column in the workflow file.

## Why

A Python script with a typo in a name or a missing parenthesis fails only when the job runs.

## Failing examples

`hello` is not defined, and `os` is not used:

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: |
          import os
          print("${{ github.ref }}", hello)
        shell: python
```

```text
test.yaml:7:11: pyflakes reported issue in this script: 1:1: 'os' imported but unused [pyflakes-issue]
test.yaml:8:38: pyflakes reported issue in this script: 2:28: undefined name 'hello' [pyflakes-issue]
```

The job runs Python by default, so the step needs no `shell`:

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    defaults:
      run:
        shell: python
    steps:
      - run: print(
```

```text
test.yaml:9:19: pyflakes reported issue in this script: 1:6: '(' was never closed [pyflakes-issue]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: print("${{ github.ref }}")
        shell: python
      - run: print(hello)
        shell: bash
```

## Differences from actionlint

- [shellcheck and pyflakes findings point into the script](../differences.md#script-finding-positions)
- [ruff is used for Python scripts when installed](../differences.md#python-linter-auto)
- [Missing tools are reported](../differences.md#missing-tools)
