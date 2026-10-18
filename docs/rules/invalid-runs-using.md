# invalid-runs-using

| Kind | Category | Default severity |
|---|---|---|
| `action` | correctness | error |

A local action's `runs.using` is not `composite`, `docker`, or a Node.js runtime that GitHub has.

The values are `composite`, `docker`, `node20`, and `node24`, written in lower case. An action whose value starts with `node` is also checked as a JavaScript action, so an old runtime such as `node16` gets the checks of [missing-runs-key](missing-runs-key.md) and the other `runs` rules too. The metadata of each local action is checked once per run, at the first step that uses it.

## Why

GitHub removes old Node.js runtimes from its runners, and an action that asks for one fails to start.

## Failing example

```yaml file=.github/actions/greet/action.yml
name: Greet
description: Prints a greeting
runs:
  using: node16
  main: index.js
```

```yaml file=.github/actions/greet/index.js
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
test.yaml:6:15: invalid runner name "node16" at runs.using in "Greet" action defined at ".github/actions/greet". valid runners are "composite", "docker", "node20", and "node24". see https://docs.github.com/en/actions/creating-actions/metadata-syntax-for-github-actions#runs [invalid-runs-using]
```

## Passing example

```yaml file=.github/actions/greet/action.yml
name: Greet
description: Prints a greeting
runs:
  using: node24
  main: index.js
```

```yaml file=.github/actions/greet/index.js
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
