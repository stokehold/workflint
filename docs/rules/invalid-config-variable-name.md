# invalid-config-variable-name

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

The name in `vars.NAME` breaks GitHub's naming rules for configuration variables.

A name has only letters, digits, and `_`, does not start with a digit, and does not start with `GITHUB_` in any case.

## Why

GitHub does not let anyone create a configuration variable with such a name, so `vars` never has it and the expression reads an empty string.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ vars.GITHUB_TOKEN }} ${{ vars.deploy-target }}'
```

```text
test.yaml:6:24: configuration variable name "github_token" must not start with the GITHUB_ prefix (case insensitive). note: see the convention at https://docs.github.com/en/actions/learn-github-actions/variables#naming-conventions-for-configuration-variables [invalid-config-variable-name]
test.yaml:6:49: configuration variable name "deploy-target" can only contain alphabets, decimal numbers, and '_'. note: see the convention at https://docs.github.com/en/actions/learn-github-actions/variables#naming-conventions-for-configuration-variables [invalid-config-variable-name]
```

actionlint stops at the first expression with an error in a string, so it reports only the first of these.

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ vars.DEPLOY_TARGET }}'
```

## Differences from actionlint

- [Every `${{ }}` in a string is checked](../differences.md#all-expressions-in-string)
