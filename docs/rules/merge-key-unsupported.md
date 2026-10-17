# merge-key-unsupported

| Kind | Category | Default severity |
|---|---|---|
| `syntax-check` | correctness | error |

A mapping has a `<<` merge key, which GitHub Actions does not apply.

The key may be quoted or not. The parser skips the entry, so the keys of the merged mapping are not part of the section.

## Why

GitHub does not apply YAML merge keys. A job or a step written with `<<` does not get the keys it was meant to merge.

## Failing example

```yaml
on: push
env: &common
  CI: true
jobs:
  test:
    runs-on: ubuntu-latest
    env:
      <<: *common
      MODE: test
    steps:
      - run: ./test.sh
```

```text
test.yaml:8:7: GitHub Actions does not support YAML merge key "<<" [merge-key-unsupported]
```

## Passing example

```yaml
on: push
env:
  CI: true
jobs:
  test:
    runs-on: ubuntu-latest
    env:
      MODE: test
    steps:
      - run: ./test.sh
```
