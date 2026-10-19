# invalid-branding-icon

| Kind | Category | Default severity |
|---|---|---|
| `action` | correctness | error |

A local action's `branding.icon` is not an icon that GitHub supports.

GitHub supports a subset of the Feather icons, listed in its metadata syntax reference. The name is compared without regard to case. The metadata of each local action is checked once per run, at the first step that uses it.

## Why

GitHub Marketplace shows the icon of a published action and rejects a name it does not support.

## Failing example

```yaml file=.github/actions/greet/action.yml
name: Greet
description: Prints a greeting
branding:
  icon: dog
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
test.yaml:6:15: incorrect icon name "dog" at branding.icon in metadata of "Greet" action at ".github/actions/greet/action.yml". see the official document to know the exhaustive list of supported icons: https://docs.github.com/en/actions/creating-actions/metadata-syntax-for-github-actions#brandingicon [invalid-branding-icon]
```

## Passing example

```yaml file=.github/actions/greet/action.yml
name: Greet
description: Prints a greeting
branding:
  icon: message-circle
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
- [Paths of local actions in messages](../differences.md#local-action-paths)
