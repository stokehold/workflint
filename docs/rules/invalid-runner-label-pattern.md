# invalid-runner-label-pattern

| Kind | Category | Default severity |
|---|---|---|
| `runner-label` | correctness | error |

A runner label pattern in the config is not a valid glob.

The patterns of the config's `self-hosted-runner.labels` use Go's `path.Match` syntax: `*` matches any text without `/`, `?` matches one character, `[...]` and `[^...]` match a character in or out of a class, and `\` escapes the next character. A pattern such as `gpu-[` is malformed. The rule reports it at each label that is not a built-in label and that reaches the pattern, because no earlier pattern matched it. The message ends with `syntax error in pattern`.

## Why

A malformed pattern matches no label, so the labels it was meant to allow would all be unknown.

## Unreachable

The rule never reports yet. workflint does not load the config until M5.2, so there are no configured patterns. The rule's unit tests pass patterns to it directly.

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: [self-hosted, linux]
    steps:
      - run: echo
```
