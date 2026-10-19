# invalid-dockerfile-name

| Kind | Category | Default severity |
|---|---|---|
| `action` | correctness | error |

A local Docker action's `image` is a file that is not named `Dockerfile`.

An `image` on a registry, such as `docker://alpine:3` or `ghcr.io/owner/image`, is not checked. The metadata of each local action is checked once per run, at the first step that uses it.

## Why

GitHub builds a local image only from a file named `Dockerfile`.

## Failing example

```yaml file=.github/actions/greet/action.yml
name: Greet
description: Prints a greeting
runs:
  using: docker
  image: Dockerfile.dev
```

```yaml file=.github/actions/greet/Dockerfile.dev
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
test.yaml:6:15: the local file "Dockerfile.dev" referenced from "image" key must be named "Dockerfile" in "Greet" action. the action is defined at ".github/actions/greet" [invalid-dockerfile-name]
```

## Passing example

```yaml file=.github/actions/greet/action.yml
name: Greet
description: Prints a greeting
runs:
  using: docker
  image: docker/Dockerfile
```

```yaml file=.github/actions/greet/docker/Dockerfile
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
