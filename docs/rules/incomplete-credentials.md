# incomplete-credentials

| Kind | Category | Default severity |
|---|---|---|
| `syntax-check` | correctness | error |

`credentials` of a container or service lacks `username` or `password`.

A `credentials` value that is one `${{ }}` expression is not checked, and neither is a mapping with an `${{ insert }}` expression, which may hold the missing key. The parser drops incomplete credentials, so the rules do not check them.

## Why

A registry login needs both a user name and a password. GitHub rejects credentials with only one of them.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    container:
      image: ghcr.io/example/test:latest
      credentials:
        username: ${{ github.actor }}
    steps:
      - run: ./test.sh
```

```text
test.yaml:7:7: both "username" and "password" must be specified in "credentials" section [incomplete-credentials]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    container:
      image: ghcr.io/example/test:latest
      credentials:
        username: ${{ github.actor }}
        password: ${{ secrets.GITHUB_TOKEN }}
    steps:
      - run: ./test.sh
```

## Differences from actionlint

- [`${{ insert }}` as a mapping key](../differences.md#insert-mapping-key)
