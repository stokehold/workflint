# missing-runs-using

| Kind | Category | Default severity |
|---|---|---|
| `action` | correctness | error |

A local action has no `runs.using`.

The metadata of each local action is checked once per run, at the first step that uses it.

## Why

`runs.using` tells GitHub whether the action is a JavaScript, Docker, or composite action. GitHub cannot run an action without it.

## Failing example

```yaml file=.github/actions/greet/action.yml
name: Greet
description: Prints a greeting
runs:
  steps:
    - run: echo hello
      shell: bash
```

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: ./.github/actions/greet
```

```text
test.yaml:6:15: "runs.using" is missing in local action "Greet" defined at ".github/actions/greet" [missing-runs-using]
```

## Passing example

```yaml file=.github/actions/greet/action.yml
name: Greet
description: Prints a greeting
runs:
  using: composite
  steps:
    - run: echo hello
      shell: bash
```

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: ./.github/actions/greet
```

## Differences from actionlint

- [Errors in shared local actions and reusable workflows](../differences.md#cached-metadata-errors)
- [Paths of local actions in messages](../differences.md#local-action-paths)
