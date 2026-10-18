# broken-reusable-workflow

| Kind | Category | Default severity |
|---|---|---|
| `workflow-call` | correctness | error |

A called local reusable workflow has no valid `on.workflow_call` section.

The callee is read as actionlint reads it: it must be valid YAML, have an `on:` key, and list `workflow_call` as an event. Its `inputs`, `outputs`, and `secrets` must be mappings. The message ends with the reason. A file that several jobs or workflows call is reported once per run, at its first call.

## Why

GitHub runs a workflow as a reusable workflow only if it has the `workflow_call` trigger, and rejects a call to any other workflow. A callee whose `on.workflow_call` cannot be read also cannot be checked against its callers.

## Failing example

```yaml file=.github/workflows/build.yaml
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: make
```

```yaml
on: push
jobs:
  build:
    uses: ./.github/workflows/build.yaml
```

```text
test.yaml:4:11: error while parsing reusable workflow "./.github/workflows/build.yaml": "workflow_call" event trigger is not found in "on:" at line:1, column:5 [broken-reusable-workflow]
```

## Passing example

```yaml file=.github/workflows/build.yaml
on:
  push:
  workflow_call:
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: make
```

```yaml
on: push
jobs:
  build:
    uses: ./.github/workflows/build.yaml
```

## Differences from actionlint

- [Errors in shared local actions and reusable workflows](../differences.md#cached-metadata-errors)
