# wrong-input-default-type

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

A `boolean` or `number` input of `workflow_call` has a default of another type.

The rule checks a default written as one `${{ }}`. A `boolean` input needs a bool, and a `number` input needs a number. A value of type `any`, such as the result of `fromJSON()`, is accepted. A default with text around the `${{ }}` is a string, and [`invalid-input-default`](invalid-input-default.md) does not check it either, because GitHub evaluates it.

A default may read the inputs declared before it. `undefined-property` reports a default that reads its own input or a later one.

## Why

GitHub evaluates the default when a caller leaves the input out, and the workflow then gets a value of the wrong type. A string default for a `boolean` input makes every `if: inputs.flag` true, because a non-empty string is truthy.

## Failing example

```yaml
on:
  workflow_call:
    inputs:
      environment:
        type: string
        default: staging
      deploy:
        type: boolean
        default: ${{ inputs.environment }}
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```

```text
test.yaml:9:18: type of input "deploy" must be bool but found type string [wrong-input-default-type]
```

## Passing example

```yaml
on:
  workflow_call:
    inputs:
      environment:
        type: string
        default: staging
      deploy:
        type: boolean
        default: ${{ inputs.environment == 'production' }}
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
```
