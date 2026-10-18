# invalid-env-var-name

| Kind | Category | Default severity |
|---|---|---|
| `env-var` | correctness | error |

A name in `env` contains `&`, `=`, a space, or a tab.

The rule checks the `env` of the workflow, of each job, of a job's container, of each service, and of each step. It skips a whole `env: ${{ }}` and a name that contains a `${{ }}`, because the name is known only when the workflow runs.

## Why

A shell cannot read a variable whose name has one of these characters. A process gets its environment as `NAME=value` strings, so the first `=` ends the name, and `FOO=BAR: foo` sets `FOO`.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    env:
      FOO=BAR: foo
      FOO BAR: foo
    steps:
      - run: echo "$FOO"
```

```text
test.yaml:6:7: environment variable name "FOO=BAR" is invalid. '&', '=' and spaces should not be contained [invalid-env-var-name]
test.yaml:7:7: environment variable name "FOO BAR" is invalid. '&', '=' and spaces should not be contained [invalid-env-var-name]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    env:
      FOO_BAR: foo
      ${{ github.event_name }}-mode: on
    steps:
      - run: echo "$FOO_BAR"
```
