# missing-pre-post-script

| Kind | Category | Default severity |
|---|---|---|
| `action` | correctness | error |

A local JavaScript action has `pre-if` without `pre`, or `post-if` without `post`.

The metadata of each local action is checked once per run, at the first step that uses it.

## Why

`pre-if` and `post-if` are the conditions of the `pre` and `post` scripts. Without the script there is nothing to run, so the condition is likely meant for a script that the author forgot to add.

## Failing example

```yaml file=.github/actions/greet/action.yml
name: Greet
description: Prints a greeting
runs:
  using: node24
  main: index.js
  post-if: always()
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
test.yaml:6:15: "post" is required when "post-if" is specified in "runs" section in "Greet" action. the action is defined at ".github/actions/greet" [missing-pre-post-script]
```

## Passing example

```yaml file=.github/actions/greet/action.yml
name: Greet
description: Prints a greeting
runs:
  using: node24
  main: index.js
  post: cleanup.js
  post-if: always()
```

```yaml file=.github/actions/greet/index.js
```

```yaml file=.github/actions/greet/cleanup.js
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
