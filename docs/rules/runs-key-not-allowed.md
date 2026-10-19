# runs-key-not-allowed

| Kind | Category | Default severity |
|---|---|---|
| `action` | correctness | error |

The `runs` section of a local action has a key that its kind of action does not use.

JavaScript actions use `main`, `pre`, `pre-if`, `post`, and `post-if`. Docker actions use `image`, `pre-entrypoint`, `entrypoint`, `post-entrypoint`, `args`, and `env`. Composite actions use `steps`. Each kind of action is reported for a key that another kind uses. An empty string, an empty `steps: []`, and a null `args` or `env` count as unset. The metadata of each local action is checked once per run, at the first step that uses it.

## Why

GitHub ignores the keys of another kind of action. A key in the wrong place usually means the action is not the kind its author thinks, such as `env` on a JavaScript action, which does not set any variable.

## Failing example

```yaml file=.github/actions/greet/action.yml
name: Greet
description: Prints a greeting
runs:
  using: node24
  main: index.js
  env:
    GREETING: hello
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
test.yaml:6:15: "env" is not allowed in "runs" section because "Greet" is a JavaScript action. the action is defined at ".github/actions/greet" [runs-key-not-allowed]
```

## Passing example

```yaml file=.github/actions/greet/action.yml
name: Greet
description: Prints a greeting
runs:
  using: docker
  image: Dockerfile
  env:
    GREETING: hello
```

```yaml file=.github/actions/greet/Dockerfile
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
