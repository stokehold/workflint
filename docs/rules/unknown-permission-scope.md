# unknown-permission-scope

| Kind | Category | Default severity |
|---|---|---|
| `permissions` | correctness | error |

A key in `permissions` is not a permission scope.

The rule checks `permissions` of the workflow and of each job. Scope names are case-sensitive, so `Contents` is unknown. The message lists every scope.

## Why

GitHub rejects the workflow. A misspelled scope such as `check` for `checks` would otherwise leave the job without the permission it needs.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    permissions:
      check: write
      Contents: read
    steps:
      - run: echo hello
```

```text
test.yaml:6:7: unknown permission scope "check". all available permission scopes are "actions", "artifact-metadata", "attestations", "checks", "code-quality", "contents", "copilot-requests", "deployments", "discussions", "id-token", "issues", "models", "packages", "pages", "pull-requests", "repository-projects", "security-events", "statuses", "vulnerability-alerts" [unknown-permission-scope]
test.yaml:7:7: unknown permission scope "Contents". all available permission scopes are "actions", "artifact-metadata", "attestations", "checks", "code-quality", "contents", "copilot-requests", "deployments", "discussions", "id-token", "issues", "models", "packages", "pages", "pull-requests", "repository-projects", "security-events", "statuses", "vulnerability-alerts" [unknown-permission-scope]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    permissions:
      checks: write
      contents: read
    steps:
      - run: echo hello
```

## Differences from actionlint

- [Keys that are not strings in `permissions` and `env`](../differences.md#non-scalar-keys)
