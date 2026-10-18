# missing-action-description

| Kind | Category | Default severity |
|---|---|---|
| `action` | correctness | error |

A local action's metadata has no `description`.

The metadata of each local action is checked once per run, at the first step that uses it.

## Why

GitHub requires `description` in action metadata. It tells the reader of a workflow what the action does without opening its code.

## Failing example

```yaml file=.github/actions/greet/action.yml
name: Greet
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

```text
test.yaml:6:15: description is required in metadata of "Greet" action at ".github/actions/greet/action.yml" [missing-action-description]
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
