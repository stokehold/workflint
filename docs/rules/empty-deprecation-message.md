# empty-deprecation-message

| Kind | Category | Default severity |
|---|---|---|
| `action` | correctness | error |

An input of a local action has an empty `deprecationMessage`.

A null value or one made only of white space counts as empty. The metadata of each local action is checked once per run, at the first step that uses it.

## Why

The key marks the input as deprecated, and GitHub shows the message as a warning when a step sets the input. An empty message tells the user that the input is going away but not what to use instead.

## Failing example

```yaml file=.github/actions/greet/action.yml
name: Greet
description: Prints a greeting
inputs:
  who:
    description: Who to greet
    deprecationMessage: ""
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
test.yaml:6:15: input "who" is deprecated but "deprecationMessage" is empty in metadata of "Greet" action at ".github/actions/greet/action.yml" [empty-deprecation-message]
```

## Passing example

```yaml file=.github/actions/greet/action.yml
name: Greet
description: Prints a greeting
inputs:
  who:
    description: Who to greet
    deprecationMessage: Use the name input instead.
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
