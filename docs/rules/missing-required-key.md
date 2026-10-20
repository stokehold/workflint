# missing-required-key

| Kind | Category | Default severity |
|---|---|---|
| `syntax-check` | correctness | error |

A required section or key is missing.

Required keys include `on` and `jobs` at the workflow root, `runs-on` and `steps` in a job without `uses`, `image` in a container or a service, and `type` of a `workflow_call` input. A job whose `steps` is present but invalid also lacks `steps`. A mapping with an `${{ insert }}` expression is not checked for its required keys, such as `image` in a container, because the inserted mapping may hold them.

## Why

GitHub rejects a workflow that lacks a required key.

## Failing example

```yaml
on: push
jobs:
  test:
    steps:
      - run: ./test.sh
```

```text
test.yaml:3:3: "runs-on" section is missing in job "test" [missing-required-key]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: ./test.sh
```

## Differences from actionlint

- [`${{ insert }}` as a mapping key](../differences.md#insert-mapping-key)
