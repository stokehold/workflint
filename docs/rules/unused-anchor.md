# unused-anchor

| Kind | Category | Default severity |
|---|---|---|
| `syntax-check` | correctness | error |

No alias refers to an anchor.

The diagnostic points at the anchor's `&` token, or at the tag when the tag comes before the anchor.

## Why

An anchor that no alias uses has no effect. It often means that an alias was removed, or that its name has a typo.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: ./build.sh
        env: &env
          CI: true
```

```text
test.yaml:7:14: anchor "env" is defined but not used [unused-anchor]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: ./build.sh
        env: &env
          CI: true
      - run: ./test.sh
        env: *env
```
