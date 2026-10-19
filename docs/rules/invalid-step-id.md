# invalid-step-id

| Kind | Category | Default severity |
|---|---|---|
| `id` | correctness | error |

A step ID does not start with an ASCII letter or `_`, or has a character other than an ASCII letter, a digit, `-`, or `_`.

The rule skips an `id` that contains a `${{ }}`. An empty `id` is reported by [empty-string](empty-string.md) instead.

## Why

GitHub requires a step's `id` to start with a letter or `_` and contain only alphanumeric characters, `-`, or `_`. Other steps read its outputs as `steps.<id>.outputs`, and an ID with a `.` or a space cannot be written there with the dot syntax.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo "version=1" >> "$GITHUB_OUTPUT"
        id: get version
```

```text
test.yaml:7:13: invalid step ID "get version". step ID must start with a letter or _ and contain only alphanumeric characters, -, or _ [invalid-step-id]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo "version=1" >> "$GITHUB_OUTPUT"
        id: get_version
      - run: echo "$VERSION"
        env:
          VERSION: ${{ steps.get_version.outputs.version }}
```
