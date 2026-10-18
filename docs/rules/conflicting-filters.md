# conflicting-filters

| Kind | Category | Default severity |
|---|---|---|
| `events` | correctness | error |

A filter and its `-ignore` filter are both set on one event.

The pairs are `branches` and `branches-ignore`, `tags` and `tags-ignore`, and `paths` and `paths-ignore`. The diagnostic is at the key written later. A filter with no entries does not count.

## Why

GitHub does not allow a filter and its `-ignore` filter on the same event. One filter with `!` patterns can include and exclude at once.

## Failing example

```yaml
on:
  push:
    branches: [main]
    branches-ignore: [main-old]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

```text
test.yaml:4:5: both "branches" and "branches-ignore" filters cannot be used for the same event "push". note: use '!' to negate patterns [conflicting-filters]
```

## Passing example

```yaml
on:
  push:
    branches:
      - main*
      - '!main-old'
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```
