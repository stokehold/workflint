# hardcoded-password

| Kind | Category | Default severity |
|---|---|---|
| `credentials` | correctness | error |

`credentials.password` of a container or service is not a single `${{ }}` expression.

The rule checks the form of the value, not where it comes from, so `${{ 'pass' }}` passes and `pass-${{ secrets.SUFFIX }}` does not. `username` is not checked. A `credentials` value that is one `${{ }}` expression is not checked either.

With a YAML anchor, a password shared by several services is reported once for each service, all at the anchored value.

## Why

Anyone who can read the repository can read a password written in the workflow. A password from `secrets` stays out of the source and is masked in the logs.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    container:
      image: ghcr.io/example/test:latest
      credentials:
        username: ci-bot
        password: hunter2
    services:
      redis:
        image: ghcr.io/example/redis:latest
        credentials:
          username: ci-bot
          password: hunter2
    steps:
      - run: ./test.sh
```

```text
test.yaml:9:19: "password" section in "container" section should be specified via secrets. do not put password value directly [hardcoded-password]
test.yaml:15:21: "password" section in "redis" service should be specified via secrets. do not put password value directly [hardcoded-password]
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
