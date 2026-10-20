# invalid-shell-name

| Kind | Category | Default severity |
|---|---|---|
| `shell-name` | correctness | error |

A `shell` value is not a shell of the job's platform or a command with `{0}`.

The rule checks `shell` of each `run` step and `defaults.run.shell` of the job against the job's platform, and `defaults.run.shell` of the workflow against every platform. Case does not matter, so `PowerShell` is `powershell`. A value that contains `{0}` or a `${{ }}` expression is not checked.

The platform comes from the labels of `runs-on`. `windows` and labels that start with `windows-` mean Windows. `macos`, `linux`, and labels that start with `macos-` or `ubuntu-` mean macOS or Linux. A label of a runner provider means the system of the provider's runner, so `depot-windows-2022` means Windows. When no label names a system in these ways, a label means the system that one of its words names: `linux`, `ubuntu`, `macos`, or `windows`, alone or followed by a version. So `self-32vcpu-windows-2022` means Windows, `gpu` means none, and `[self-hosted, windows, wsl-ubuntu]` means Windows, because `windows` decides before the words of `wsl-ubuntu`. Words inside `${{ }}` do not count. Labels of two systems, such as `ubuntu-latest` and `macos-latest`, or of none, accept the shells of every platform:

| Platform | Shells |
|---|---|
| Windows | `bash`, `cmd`, `powershell`, `pwsh`, `python` |
| macOS or Linux | `bash`, `pwsh`, `python`, `sh` |
| Any | all six |

A job's `defaults.run.shell` is checked only when the job has `runs-on`, as in actionlint.

## Why

GitHub knows these shell names and nothing else. Any other program runs only through a command template, where `{0}` stands for the path of the script, such as `perl {0}`. A step whose `shell` is neither fails when the job runs.

## Failing examples

`dash` is no shell name on any platform, so the message suggests the command form:

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
        shell: dash
```

```text
test.yaml:7:16: shell name "dash" is invalid. available names are "bash", "pwsh", "python", "sh". use a command with {0} for a custom shell, as in "dash {0}" [invalid-shell-name]
```

`sh` is a shell of macOS and Linux, not of Windows:

```yaml
on: push
jobs:
  test:
    runs-on: windows-latest
    defaults:
      run:
        shell: sh
    steps:
      - run: echo hello
```

```text
test.yaml:7:16: shell name "sh" is invalid on Windows. available names are "bash", "cmd", "powershell", "pwsh", "python" [invalid-shell-name]
```

Depot's `depot-windows-2022` runs Windows:

```yaml
on: push
jobs:
  test:
    runs-on: depot-windows-2022
    steps:
      - run: echo hello
        shell: sh
```

```text
test.yaml:7:16: shell name "sh" is invalid on Windows. available names are "bash", "cmd", "powershell", "pwsh", "python" [invalid-shell-name]
```

## Passing example

```yaml
on: push
defaults:
  run:
    shell: bash
jobs:
  test:
    runs-on: windows-latest
    steps:
      - run: Write-Output hello
        shell: PowerShell
      - run: print('hello')
        shell: python
      - run: echo hello
        shell: dash {0}
```

## Differences from actionlint

- [A `shell` that is no shell name says how to run another program](../differences.md#custom-shell-hint)
- [An invalid value is reported once](../differences.md#invalid-value-reported-once)
- [Labels of runner providers, and the system that custom labels name](../differences.md#runner-provider-labels)
