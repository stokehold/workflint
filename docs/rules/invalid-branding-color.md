# invalid-branding-color

| Kind | Category | Default severity |
|---|---|---|
| `action` | correctness | error |

A local action's `branding.color` is not a color that GitHub supports.

The colors are `white`, `black`, `yellow`, `blue`, `green`, `orange`, `red`, `purple`, and `gray-dark`, compared without regard to case. The metadata of each local action is checked once per run, at the first step that uses it.

## Why

GitHub Marketplace shows the badge of a published action in this color and rejects a color it does not support.

## Failing example

```yaml file=.github/actions/greet/action.yml
name: Greet
description: Prints a greeting
branding:
  color: pink
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
test.yaml:6:15: incorrect color "pink" at branding.color in metadata of "Greet" action at ".github/actions/greet/action.yml". see the official document to know the exhaustive list of supported colors: https://docs.github.com/en/actions/creating-actions/metadata-syntax-for-github-actions#brandingcolor [invalid-branding-color]
```

## Passing example

```yaml file=.github/actions/greet/action.yml
name: Greet
description: Prints a greeting
branding:
  color: purple
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

- [Branding color message names `branding.color`](../differences.md#branding-color-message)
- [Errors in shared local actions and reusable workflows](../differences.md#cached-metadata-errors)
- [Paths of local actions in messages](../differences.md#local-action-paths)
