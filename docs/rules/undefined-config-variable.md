# undefined-config-variable

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

`vars.NAME` reads a configuration variable that the configuration does not list.

When the configuration file lists the configuration variables of the repository, `vars.NAME` must name one of them, in any case. An empty list allows none. Without a list, `vars` accepts any name.

## Why

A configuration variable that the repository does not define evaluates to an empty string, so a typo in its name passes an empty value to the job.

## Unreachable

workflint does not read a configuration file yet, so `vars` accepts any name and the rule never reports. Configuration loading arrives in M5.2, which adds the failing example to this page.

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ vars.DEPLOY_TARGET }}'
```
