# undefined-action-input

| Kind | Category | Default severity |
|---|---|---|
| `action` | correctness | error |

`with:` sets an input that the action does not declare.

workflint knows the inputs of popular actions at their common versions, such as `actions/setup-node@v4`. Input names are case-insensitive. An action that reads inputs its metadata does not declare is not checked.

## Why

GitHub passes an undeclared input to the action, which ignores it, and prints only a warning. A misspelled input name therefore leaves the input at its default without failing the run.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/setup-node@v4
        with:
          node_version: 22
```

```text
test.yaml:8:11: input "node_version" is not defined in action "actions/setup-node@v4". available inputs are "always-auth", "architecture", "cache", "cache-dependency-path", "check-latest", "mirror", "mirror-token", "node-version", "node-version-file", "registry-url", "scope", "token" [undefined-action-input]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/setup-node@v4
        with:
          node-version: 22
```
