# undefined-config-variable

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

`vars.NAME` reads a configuration variable that the configuration does not list.

When the configuration file lists the configuration variables of the repository, `vars.NAME` must name one of them, in any case. An empty list allows none. Without a list, `vars` accepts any name.

## Why

A configuration variable that the repository does not define evaluates to an empty string, so a typo in its name passes an empty value to the job.

## Failing examples

```yaml file=.github/actionlint.yaml
config-variables: [DEPLOY_TARGET]
```

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ vars.DEPLOY_TARGT }}'
```

```text
test.yaml:6:24: undefined configuration variable "deploy_targt". defined configuration variables in actionlint.yaml are "DEPLOY_TARGET" [undefined-config-variable]
```

An empty list allows no variable:

```yaml file=.github/actionlint.yaml
config-variables: []
```

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ vars.DEPLOY_TARGET }}'
```

```text
test.yaml:6:24: no configuration variable is allowed since the variables list is empty in actionlint.yaml. you may forget adding the variable "deploy_target" to the list [undefined-config-variable]
```

## Passing example

```yaml file=.github/actionlint.yaml
config-variables: [DEPLOY_TARGET]
```

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ vars.DEPLOY_TARGET }}'
```
