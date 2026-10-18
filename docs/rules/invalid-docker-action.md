# invalid-docker-action

| Kind | Category | Default severity |
|---|---|---|
| `action` | correctness | error |

A `docker://` action has an invalid URI or an empty tag.

The tag starts at the first `:` after `docker://`. A `:` with nothing after it is an empty tag. The text before the tag must parse as a URI, and the message quotes the parse error, which is the error text of Go's `url.Parse`, as in actionlint.

## Why

GitHub pulls the image that the URI names. An image name with characters a host name cannot have, or with an empty tag, cannot be pulled, so the step fails.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: 'docker://alpine:'
      - uses: docker://my image:3.20
```

```text
test.yaml:6:15: tag of Docker action should not be empty: "docker://alpine" [invalid-docker-action]
test.yaml:7:15: URI for Docker container "docker://my image" is invalid: parse "docker://my image": invalid character " " in host name (tag=3.20) [invalid-docker-action]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: docker://alpine
      - uses: docker://ghcr.io/owner/image:1.0
```
