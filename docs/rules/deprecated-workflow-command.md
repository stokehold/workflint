# deprecated-workflow-command

| Kind | Category | Default severity |
|---|---|---|
| `deprecated-commands` | correctness | error |

A `run:` script uses the deprecated `set-output`, `save-state`, `set-env`, or `add-path` workflow command.

The rule looks for `::set-output name=<name>::<value>`, `::save-state name=<name>::<value>`, `::set-env name=<name>::<value>`, and `::add-path::<path>` anywhere in the script, including inside quotes. A name starts with a letter, followed by letters, digits, `_`, and `-`. Each command gets its own diagnostic. All of them point at the start of the `run:` value, not at the line with the command, and they are sorted by message. The message names the environment file to write to instead.

## Why

GitHub disabled `set-env` and `add-path` in 2020 because any process that printed to the log could set environment variables and `PATH` for later steps. It deprecated `set-output` and `save-state` in 2022 for the same reason. A step's log can contain text from pull requests, issues, or downloaded files, and that text could carry one of these commands. The environment files `$GITHUB_OUTPUT`, `$GITHUB_STATE`, `$GITHUB_ENV`, and `$GITHUB_PATH` can only be written by the step itself.

## Failing example

```yaml
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - id: version
        run: |
          echo "::set-output name=version1::$(cat VERSION)"
          echo "::add-path::$HOME/.local/bin"
```

```text
test.yaml:7:14: workflow command "add-path" was deprecated. use `echo "{path}" >> $GITHUB_PATH` instead: https://docs.github.com/en/actions/using-workflows/workflow-commands-for-github-actions [deprecated-workflow-command]
test.yaml:7:14: workflow command "set-output" was deprecated. use `echo "{name}={value}" >> $GITHUB_OUTPUT` instead: https://docs.github.com/en/actions/using-workflows/workflow-commands-for-github-actions [deprecated-workflow-command]
```

## Passing example

```yaml
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - id: version
        run: |
          echo "version1=$(cat VERSION)" >> "$GITHUB_OUTPUT"
          echo "$HOME/.local/bin" >> "$GITHUB_PATH"
          echo "::debug::Set the version"
```

## Differences from actionlint

- [Deprecated commands with digits in the name](../differences.md#deprecated-command-digits)
