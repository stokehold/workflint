# undefined-property

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

An expression reads a property that its strict object type does not have.

Many contexts have strict object types that list every property, such as `job.container`, `inputs`, `matrix`, and an object that `fromJSON()` parses from a string literal. Reading another property is an error. A loose object, such as `github.event`, accepts any property. Property names are case insensitive, with a dot or with a string literal index.

The diagnostic also has a help text with the closest property of the type, such as `did you mean "temp"?` for `runner.tmp`. The default and one-line formats do not print it (ADR 0032).

## Why

A property that does not exist evaluates to an empty string, so a typo in a property name or an input name passes a wrong value to the step without failing it.

## Failing examples

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ job.container.os }}'
```

```text
test.yaml:6:24: property "os" is not defined in object type {id: string; network: string} [undefined-property]
```

`inputs` has only the inputs that the workflow's triggers define:

```yaml
on:
  workflow_dispatch:
    inputs:
      environment:
        type: string
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ inputs.enviroment }}'
```

```text
test.yaml:10:24: property "enviroment" is not defined in object type {environment: string} [undefined-property]
```

`steps` has only the steps with an `id` that run before the current step:

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - id: build
        run: echo "version=1" >> "$GITHUB_OUTPUT"
      - run: echo '${{ steps.biuld.outputs.version }}'
```

```text
test.yaml:8:24: property "biuld" is not defined in object type {build: {conclusion: string; outcome: string; outputs: {string => string}}} [undefined-property]
```

## Passing example

```yaml
on:
  workflow_dispatch:
    inputs:
      environment:
        type: string
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ inputs.Environment }} ${{ job.container['Network'] }}'
      - run: echo '${{ fromJSON('{"Foo":1}').foo }} ${{ github.event.whatever }}'
```

## Differences from actionlint

- [Case-insensitive literal index keys](../differences.md#literal-index-case)
- [Case-insensitive keys from `fromJSON` literals](../differences.md#fromjson-key-case)
- [Object filters do not change shared types](../differences.md#object-filter-mutation)
- [`jobs.<job_id>.result` in reusable workflow outputs](../differences.md#jobs-context-result)
- [`secrets` in workflows with several triggers](../differences.md#secrets-context-mixed-triggers)
- [A job that needs itself](../differences.md#self-needs-case)
