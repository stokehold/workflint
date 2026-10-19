# wrong-workflow-call-input-type

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

A `with:` value has a type that the input of the local reusable workflow does not accept.

The rule checks calls to local reusable workflows, written with `./`, against the `type` of each input under the callee's `on.workflow_call.inputs`. A `string` input accepts a string or a number, a `number` input a number, and a `boolean` input a bool. An input without a valid `type` accepts anything. The `workflow-call` rule reports an input that the callee does not declare, so this rule skips it.

A value written as one `${{ }}` has the type of the expression, and a value of type `any`, such as the result of `fromJSON()`, is accepted. A value with text around a `${{ }}` is a string. A literal has the type of its YAML tag: `true` is a bool, `42` is a number, `null`, `~`, and an empty value are null, and a quoted value such as `'true'` is a string. A value whose expression has an error gets no type check, because its type is not known.

## Why

GitHub checks the types of the inputs when the run starts and fails the call if one does not match. The rule finds the mistake before the push.

## Failing examples

```yaml file=.github/workflows/deploy.yaml
on:
  workflow_call:
    inputs:
      replicas:
        type: number
      dry-run:
        type: boolean
jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - run: ./deploy.sh
```

```yaml
on: push
jobs:
  deploy:
    uses: ./.github/workflows/deploy.yaml
    with:
      replicas: '3'
      dry-run: yes
```

```text
test.yaml:6:17: input "replicas" is typed as number by reusable workflow "./.github/workflows/deploy.yaml". string value cannot be assigned [wrong-workflow-call-input-type]
test.yaml:7:16: input "dry-run" is typed as bool by reusable workflow "./.github/workflows/deploy.yaml". string value cannot be assigned [wrong-workflow-call-input-type]
```

An empty value is null, which a `string` input does not accept:

```yaml file=.github/workflows/deploy.yaml
on:
  workflow_call:
    inputs:
      environment:
        type: string
jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - run: ./deploy.sh
```

```yaml
on: push
jobs:
  deploy:
    uses: ./.github/workflows/deploy.yaml
    with:
      environment:
```

```text
test.yaml:6:19: input "environment" is typed as string by reusable workflow "./.github/workflows/deploy.yaml". null value cannot be assigned [wrong-workflow-call-input-type]
```

## Passing example

```yaml file=.github/workflows/deploy.yaml
on:
  workflow_call:
    inputs:
      environment:
        type: string
      replicas:
        type: number
      dry-run:
        type: boolean
jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - run: ./deploy.sh
```

```yaml
on: push
jobs:
  deploy:
    uses: ./.github/workflows/deploy.yaml
    with:
      environment: 'true'
      replicas: ${{ github.event_name == 'push' && 3 || 1 }}
      dry-run: ${{ github.ref != 'refs/heads/main' }}
```

## Differences from actionlint

- [Quoted values are strings](../differences.md#quoted-scalar-types)
- [`bool` no longer accepts every type](../differences.md#bool-assignability)
- [A `with:` value whose expression has an error is not type-checked](../differences.md#with-expression-errors)
