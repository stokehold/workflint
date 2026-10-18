# filter-not-supported

| Kind | Category | Default severity |
|---|---|---|
| `events` | correctness | error |

A branch, tag, or path filter is set on an event that does not support it.

`paths` and `paths-ignore` are for `push`, `pull_request`, and `pull_request_target`. `branches` and `branches-ignore` are for those three, `merge_group`, and `workflow_run`. `tags` and `tags-ignore` are only for `push`.

## Why

An event that does not support a filter does not apply it, so the filter cannot narrow when the workflow runs.

## Failing example

```yaml
on:
  release:
    tags: [v*]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

```text
test.yaml:3:5: "tags" filter is not available for release event. it is only for push event [filter-not-supported]
```

## Passing example

```yaml
on:
  push:
    tags: [v*]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```
