# invalid-permission-level

| Kind | Category | Default severity |
|---|---|---|
| `permissions` | correctness | error |

A permission scope is set to a level that the scope does not accept.

The rule checks `permissions` of the workflow and of each job. Most scopes accept `read`, `write`, and `none`. `id-token` accepts only `write` and `none`, and `models` only `read` and `none`. Levels are case-sensitive, so `READ` is invalid. The message lists the levels of the scope.

## Why

GitHub rejects the workflow. `id-token: read` is a common mistake: requesting an OIDC token needs `write`.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    permissions:
      issues: readable
      id-token: read
      models: write
    steps:
      - run: echo hello
```

```text
test.yaml:6:15: "readable" is invalid as permission of scope "issues". available values are "read", "write", "none" [invalid-permission-level]
test.yaml:7:17: "read" is invalid as permission of scope "id-token". available values are "write", "none" [invalid-permission-level]
test.yaml:8:15: "write" is invalid as permission of scope "models". available values are "read", "none" [invalid-permission-level]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    permissions:
      issues: read
      id-token: write
      models: read
    steps:
      - run: echo hello
```

## Differences from actionlint

- [An invalid value is reported once](../differences.md#invalid-value-reported-once)
