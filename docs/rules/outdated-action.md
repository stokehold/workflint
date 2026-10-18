# outdated-action

| Kind | Category | Default severity |
|---|---|---|
| `action` | correctness | error |

A popular action's version runs on a runtime that GitHub no longer has.

workflint knows the popular actions whose old versions run on a Node.js runtime that GitHub has removed, such as `node12` or `node16`. The spec in `uses:` must match one of those versions exactly, as `actions/checkout@v3` does. A version pinned to a commit SHA or to a patch release, such as `actions/checkout@v3.6.0`, is not checked.

## Why

GitHub runs these versions on a newer runtime than the one they were written for, or refuses to run them, so the step may break. Updating to the action's current major version fixes it.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
```

```text
test.yaml:6:15: the runner of "actions/checkout@v3" action is too old to run on GitHub Actions. update the action's version to fix this issue [outdated-action]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
```
