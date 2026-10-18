# invalid-action-format

| Kind | Category | Default severity |
|---|---|---|
| `action` | correctness | error |

A step's `uses:` is not a local action, a Docker image, or `{owner}/{repo}@{ref}`.

A value that starts with neither `./` nor `docker://` must name an action in a repository as `{owner}/{repo}@{ref}` or `{owner}/{repo}/{path}@{ref}`. The message says what is wrong: the `@{ref}` is missing, the `{owner}/` is missing, or the owner, the repo, or the ref is empty. A value with a `${{ }}` expression is not checked, because its value is known only when the workflow runs.

## Why

GitHub cannot resolve such a value to an action, so the job fails before its first step runs. A local action written without `./`, such as `.github/actions/build`, is read as a repository action and gets this error.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout
      - uses: checkout@v4
      - uses: .github/actions/build
```

```text
test.yaml:6:15: specifying action "actions/checkout" in invalid format because ref is missing. available formats are "{owner}/{repo}@{ref}" or "{owner}/{repo}/{path}@{ref}" [invalid-action-format]
test.yaml:7:15: specifying action "checkout@v4" in invalid format because owner is missing. available formats are "{owner}/{repo}@{ref}" or "{owner}/{repo}/{path}@{ref}" [invalid-action-format]
test.yaml:8:15: specifying action ".github/actions/build" in invalid format because ref is missing. available formats are "{owner}/{repo}@{ref}" or "{owner}/{repo}/{path}@{ref}" [invalid-action-format]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: github/codeql-action/init@v3
      - uses: docker://alpine:3.20
```

## Differences from actionlint

- [An invalid value is reported once](../differences.md#invalid-value-reported-once)
