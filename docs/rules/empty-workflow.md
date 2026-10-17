# empty-workflow

| Kind | Category | Default severity |
|---|---|---|
| `syntax-check` | correctness | error |

The file has no YAML document, or only comments.

The message is `workflow is empty`, at the first line.

## Why

A workflow needs at least `on` and `jobs`. GitHub rejects an empty file.

## Failing example

```yaml
# TODO: write the workflow
```

```text
test.yaml:1:1: workflow is empty [empty-workflow]
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
