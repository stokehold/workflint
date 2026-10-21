# shellcheck-issue

| Kind | Category | Default severity |
|---|---|---|
| `shellcheck` | correctness | error |

shellcheck reports a problem in a `run:` script.

workflint runs [shellcheck](https://www.shellcheck.net) on each `run:` script whose shell is `bash` or `sh`, as actionlint does. The shell is the step's `shell`, else the job's `defaults.run.shell`, else the workflow's `defaults.run.shell`. Without any of these, it is `pwsh` when a label of `runs-on` is `windows` or starts with `windows-`, ignoring case, and `bash` otherwise. A `shell` of `bash` or `sh` followed by a space and options, such as `bash -e {0}`, counts as that shell. Any other shell is not checked, and the match is case-sensitive.

Before shellcheck reads the script, each `${{ }}` becomes as many `_` characters as it has bytes, and a first line `set -eo pipefail` for `bash` or `set -e` for `sh` adds the options that GitHub runs the shell with. shellcheck runs as `shellcheck --norc -f json -x --shell <shell> -e SC1091,SC2194,SC2050,SC2153,SC2154,SC2157,SC2043 -` with the script on stdin. The checks that `-e` turns off report scripts that work once `${{ }}` is replaced, or scripts that read the variables of `env:`. A `# shellcheck disable=SC2086` comment in the script works as it does in a shell file.

The message is actionlint's: `shellcheck reported issue in this script:`, then the code, the level, the line and column in the script, and shellcheck's message without its final period. The diagnostic points at that line and column in the workflow file. Identical scripts with the same shell run shellcheck once per run, and `-jobs` sets how many shellcheck processes run at once. `-shellcheck=PATH` names the executable, and `-shellcheck=` turns the rule off ([command line](../cli.md)).

## Why

GitHub runs the script as written, so a mistake that shellcheck finds, such as an unquoted variable that splits on spaces, shows up only when the job runs.

## Failing examples

`$HOME` without quotes splits into several words if it has spaces:

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: |
          echo "${{ github.ref }}"
          echo $HOME
```

```text
test.yaml:8:16: shellcheck reported issue in this script: SC2086:info:2:6: Double quote to prevent globbing and word splitting [shellcheck-issue]
```

On Windows the default shell is PowerShell, so only the step with `shell: bash` is checked:

```yaml
on: push
jobs:
  test:
    runs-on: windows-latest
    steps:
      - run: echo $HOME
      - run: echo $HOME
        shell: bash
```

```text
test.yaml:7:19: shellcheck reported issue in this script: SC2086:info:1:6: Double quote to prevent globbing and word splitting [shellcheck-issue]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo "$HOME"
      - run: echo ${{ github.sha }}
      - run: Write-Output $HOME
        shell: pwsh
```

## Differences from actionlint

- [shellcheck and pyflakes findings point into the script](../differences.md#script-finding-positions)
- [Missing tools are reported](../differences.md#missing-tools)
