# broken-action-metadata

| Kind | Category | Default severity |
|---|---|---|
| `action` | correctness | error |

A local action's metadata file is not valid YAML or cannot be decoded.

The metadata is read from `action.yaml`, or from `action.yml` if there is no `action.yaml`, in the directory that `uses:` names. The message ends with the reason, such as a section of the wrong type, an input or output that is defined twice, or an unknown key in an input definition. An action used by several steps or workflows is reported once per run, at its first use. A directory without a metadata file is not reported, because it is often checked out by an earlier step.

## Why

GitHub cannot run an action whose metadata it cannot read, and a broken file also cannot be checked against the steps that use it.

## Failing example

```yaml file=.github/actions/greet/action.yml
name: Greet
description: Prints a greeting
outputs: greeting
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
test.yaml:6:15: could not parse action metadata in ".github/actions/greet": line 3: yaml: outputs must be mapping node but scalar node was found at line:3, col:10 [broken-action-metadata]
```

## Passing example

```yaml file=.github/actions/greet/action.yml
name: Greet
description: Prints a greeting
outputs:
  greeting:
    description: The greeting
    value: hello
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
- [Aliases and merge keys in `action.yml` inputs](../differences.md#action-metadata-aliases)
- [Paths of local actions in messages](../differences.md#local-action-paths)
