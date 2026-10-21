# invalid-runner-label-pattern

| Kind | Category | Default severity |
|---|---|---|
| `runner-label` | correctness | error |

A runner label pattern in the config is not a valid glob.

The patterns of the config's [`self-hosted-runner.labels`](../config.md#self-hosted-runnerlabels) use Go's `path.Match` syntax: `*` matches any text without `/`, `?` matches one character, `[...]` and `[^...]` match a character in or out of a class, and `\` escapes the next character. A pattern such as `gpu-[` is malformed. The rule reports it at each label that is not a built-in label and that reaches the pattern, because no earlier pattern matched it. The message ends with `syntax error in pattern`.

## Why

A malformed pattern matches no label, so the labels it was meant to allow would all be unknown.

## Failing example

```yaml file=.github/actionlint.yaml
self-hosted-runner:
  labels: ['gpu-[']
```

```yaml
on: push
jobs:
  test:
    runs-on: gpu-a100
    steps:
      - run: echo
```

```text
test.yaml:4:14: label pattern "gpu-[" is an invalid glob. kindly check list of labels in actionlint.yaml config file: syntax error in pattern [invalid-runner-label-pattern]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: [self-hosted, linux]
    steps:
      - run: echo
```
