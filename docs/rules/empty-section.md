# empty-section

| Kind | Category | Default severity |
|---|---|---|
| `syntax-check` | correctness | error |

A mapping or sequence that must have entries is empty or null.

Examples are `jobs`, `steps`, and the body of a job.

## Why

GitHub rejects an empty section where it needs entries. Remove the section, or add the entries it is missing.

## Failing example

```yaml
on: push
jobs:
```

```text
test.yaml:2:6: "jobs" section should not be empty. please remove this section if it's unnecessary [empty-section]
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

- [`snapshot` errors name the right section](../differences.md#snapshot-section-name)
