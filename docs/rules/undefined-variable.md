# undefined-variable

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

An expression reads a variable that is not a context.

The variables of an expression are the contexts, such as `github`, `env`, and `matrix`. The message lists the contexts that exist. Names are case insensitive, so `GitHub` is `github`.

The diagnostic also has a help text with the closest context, such as `did you mean "github"?` for `gihub`. A built-in function that takes no arguments counts too, so `if: always` gets `did you mean "always()"?`. Only the contexts and functions available where the expression is count, so `matix` under `strategy` and `always` in `run:` get no help text. The default and one-line formats do not print the help text (ADR 0032).

## Why

GitHub rejects a workflow whose expression reads a variable that does not exist. The cause is usually a typo in a context name.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ gihtub.sha }}'
```

```text
test.yaml:6:24: undefined variable "gihtub". available variables are "env", "github", "inputs", "job", "matrix", "needs", "runner", "secrets", "steps", "strategy", "vars" [undefined-variable]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ GitHub.sha }}'
```
