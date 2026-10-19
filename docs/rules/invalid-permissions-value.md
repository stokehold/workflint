# invalid-permissions-value

| Kind | Category | Default severity |
|---|---|---|
| `permissions` | correctness | error |

The string form of `permissions` is not `read-all` or `write-all`.

The rule checks `permissions` of the workflow and of each job. A string sets every scope at once, and only `read-all` and `write-all` do that. To give no permission at all, write the empty mapping `{}`. `permissions:` with no value is an empty string, so it gets this error as well as [empty-string](empty-string.md).

## Why

GitHub rejects the workflow. A level such as `write` or `none` applies to one scope, so it needs a scope name in front of it.

## Failing example

```yaml
on: push
permissions: write
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

```text
test.yaml:2:14: "write" is invalid for permission for all the scopes. available values are "read-all", "write-all" or {} [invalid-permissions-value]
```

## Passing examples

```yaml
on: push
permissions: read-all
jobs:
  test:
    runs-on: ubuntu-latest
    permissions: {}
    steps:
      - run: echo hello
```

```yaml
on: push
permissions:
  contents: write
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```
