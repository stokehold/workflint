# reusable-workflow-not-found

| Kind | Category | Default severity |
|---|---|---|
| `workflow-call` | correctness | error |

The file of a called local reusable workflow cannot be read.

The path in `uses:` is relative to the project root, the directory that holds `.git`. The message ends with the error from reading the file, which names its absolute path in a real run. Without a project, local reusable workflows are not checked. A file that several jobs or workflows call is reported once per run, at its first call.

## Why

GitHub cannot start a job whose reusable workflow does not exist, so the whole run fails. The usual causes are a misspelled file name, a `.yml` that should be `.yaml`, and a path relative to the calling workflow instead of the repository root.

## Failing example

```yaml
on: push
jobs:
  build:
    uses: ./.github/workflows/biuld.yaml
```

```text
test.yaml:4:11: could not read reusable workflow file for "./.github/workflows/biuld.yaml": open .github/workflows/biuld.yaml: no such file or directory [reusable-workflow-not-found]
```

## Passing example

```yaml file=.github/workflows/build.yaml
on: workflow_call
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
