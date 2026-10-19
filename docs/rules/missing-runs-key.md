# missing-runs-key

| Kind | Category | Default severity |
|---|---|---|
| `action` | correctness | error |

The `runs` section of a local action lacks the key that its kind of action needs.

A JavaScript action needs `main`, a Docker action needs `image`, and a composite action needs `steps`. An empty `steps: []` counts as set. The metadata of each local action is checked once per run, at the first step that uses it.

## Why

The key says what GitHub runs: the script of a JavaScript action, the image of a Docker action, or the steps of a composite action. GitHub cannot run the action without it.

## Failing example

```yaml file=.github/actions/greet/action.yml
name: Greet
description: Prints a greeting
runs:
  using: composite
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
test.yaml:6:15: "steps" is required in "runs" section because "Greet" is a Composite action. the action is defined at ".github/actions/greet" [missing-runs-key]
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
