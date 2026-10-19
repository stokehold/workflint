# action-file-not-found

| Kind | Category | Default severity |
|---|---|---|
| `action` | correctness | warning |

A file that the `runs` section of a local action names does not exist.

The files are `main`, `pre`, and `post` of a JavaScript action, and `image`, `pre-entrypoint`, `entrypoint`, and `post-entrypoint` of a Docker action, each relative to the action's directory. An `image` on a registry, such as `docker://alpine:3` or `ghcr.io/owner/image`, is not a file. The metadata of each local action is checked once per run, at the first step that uses it.

The default severity is warning, because an earlier step of the job may build the file, as `npm run build` builds the `dist/index.js` of a JavaScript action.

## Why

The action fails when GitHub cannot find the file it should run.

## Failing example

```yaml file=.github/actions/greet/action.yml
name: Greet
description: Prints a greeting
runs:
  using: node24
  main: dist/index.js
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
test.yaml:6:15: file "dist/index.js" does not exist in ".github/actions/greet". it is specified at "main" key in "runs" section in "Greet" action [action-file-not-found]
```

## Passing example

```yaml file=.github/actions/greet/action.yml
name: Greet
description: Prints a greeting
runs:
  using: node24
  main: dist/index.js
```

```yaml file=.github/actions/greet/dist/index.js
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

- [A missing file of a local action is a warning](../differences.md#action-file-not-found-warning)
- [Errors in shared local actions and reusable workflows](../differences.md#cached-metadata-errors)
- [Paths of local actions in messages](../differences.md#local-action-paths)
