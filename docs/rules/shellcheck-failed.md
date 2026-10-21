# shellcheck-failed

| Kind | Category | Default severity |
|---|---|---|
| `shellcheck` | correctness | error |

shellcheck could not check a `run:` script.

workflint reports it at the `run:` key of a script that [shellcheck-issue](shellcheck-issue.md) sends to shellcheck, when shellcheck gives no findings for it:

- shellcheck cannot be started.
- It exits with a status other than 0 and prints nothing to stdout. The message has its exit status and stderr.
- A signal kills it.
- It runs longer than 60 seconds, and workflint kills it.
- Its stdout is not the JSON that `-f json` prints. The message has the stdout.

The other scripts and files are linted as usual, and the run exits with code 1.

## Why

A broken or misconfigured shellcheck, such as one that `SHELLCHECK_OPTS` gives an unknown option, would otherwise leave the scripts unchecked without a word. actionlint stops the whole run with exit code 3 and prints no results instead.

## Failing example

With a shellcheck that exits with status 2 and prints `failed` to stderr:

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

```text
test.yaml:6:9: shellcheck did not run successfully while checking this script: shellcheck exited with status 2 but stdout was empty. stderr: "failed" [shellcheck-failed]
```

## Passing example

shellcheck does not check PowerShell scripts:

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: Write-Output hello
        shell: pwsh
```

## Differences from actionlint

- [A tool failure does not abort the run](../differences.md#tool-failures)
- [External processes cannot hang the run](../differences.md#external-process-hangs)
