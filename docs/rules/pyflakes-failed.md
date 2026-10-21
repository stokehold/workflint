# pyflakes-failed

| Kind | Category | Default severity |
|---|---|---|
| `pyflakes` | correctness | error |

pyflakes or ruff could not check a Python `run:` script.

workflint reports it at the `run:` key of a script that [pyflakes-issue](pyflakes-issue.md) sends to pyflakes or ruff, when the linter gives no findings for it:

- The linter cannot be started.
- pyflakes exits with a status other than 0 and prints no `<stdin>:` line, or prints a `<stdin>:` line without a line break at its end. The message has its output.
- ruff exits with a status other than 0 or 1, or prints something that is not its JSON. The message has its stderr or its stdout.
- A signal kills the linter.
- It runs longer than 60 seconds, or the seconds of the config key [`tool-timeout`](../config.md#tool-timeout), and workflint kills it.

The other scripts and files are linted as usual, and the run exits with code 1.

## Why

A broken Python linter, such as a pyflakes whose Python cannot import it, would otherwise leave the scripts unchecked without a word. actionlint stops the whole run with exit code 3 for some of these cases, and reports nothing for the others.

## Failing example

With a pyflakes that exits with status 2 and prints `failed`:

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: print(1)
        shell: python
```

```text
test.yaml:6:9: pyflakes did not run successfully while checking this script: pyflakes exited with status 2 without reporting a problem. output: "failed" [pyflakes-failed]
```

## Passing example

Only Python scripts go to the linter:

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

## Differences from actionlint

- [A tool failure does not abort the run](../differences.md#tool-failures)
- [External processes cannot hang the run](../differences.md#external-process-hangs)
