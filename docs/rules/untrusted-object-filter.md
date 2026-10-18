# untrusted-object-filter

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

A script reads an object filter that extracts values that someone outside the repository can set.

The rule checks `run:` scripts and the `script` input of `actions/github-script`. An object filter, such as `github.event.*.body`, reads a property of every value of the object, and the message lists the untrusted values among them.

## Why

GitHub replaces `${{ }}` in a script with the value before the shell runs it, so any of the extracted values can inject a command. An environment variable passes the values as data.

## Failing example

```yaml
on: issue_comment
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ toJSON(github.event.*.body) }}'
```

```text
test.yaml:6:31: object filter extracts potentially untrusted properties "github.event.comment.body", "github.event.discussion.body", "github.event.issue.body", "github.event.pull_request.body", "github.event.review.body", "github.event.review_comment.body". avoid using the value directly in inline scripts. instead, pass the value through an environment variable. see https://docs.github.com/en/actions/reference/security/secure-use#good-practices-for-mitigating-script-injection-attacks for more details [untrusted-object-filter]
```

## Passing example

```yaml
on: issue_comment
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo "$BODIES"
        env:
          BODIES: ${{ toJSON(github.event.*.body) }}
```

## Differences from actionlint

- [Every `${{ }}` in a string is checked](../differences.md#all-expressions-in-string)
