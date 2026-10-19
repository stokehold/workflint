# missing-required-action-input

| Kind | Category | Default severity |
|---|---|---|
| `action` | correctness | error |

A step does not set an input that its action requires.

An input is required when the action's metadata has `required: true` and no `default`. workflint knows the inputs of popular actions at their common versions, such as `actions/cache@v4`, and reads the `action.yml` of local actions. Each missing input gets its own error at `uses:`.

## Why

GitHub does not check required inputs. The action gets an empty string for the input, and usually fails with its own error or does something other than what was meant.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/cache@v4
        with:
          path: ~/.npm
```

```text
test.yaml:6:15: missing input "key" which is required by action "actions/cache@v4". all required inputs are "key", "path" [missing-required-action-input]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/cache@v4
        with:
          path: ~/.npm
          key: npm-${{ hashFiles('package-lock.json') }}
```

## Differences from actionlint

- [`${{ insert }}` as a mapping key](../differences.md#insert-mapping-key)
