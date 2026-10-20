# invalid-reusable-workflow-ref

| Kind | Category | Default severity |
|---|---|---|
| `workflow-call` | correctness | error |

A job's `uses:` is not `./path/to/workflow.yml` or `owner/repo/path/to/workflow.yml@ref`.

A local reusable workflow is a path that starts with `./`, has at least one more character, and has no `@` after that character. `./@x` counts as a local path, as in actionlint, and [`reusable-workflow-not-found`](reusable-workflow-not-found.md) reports that its file could not be read. A workflow in another repository needs an owner, a repo, a path, and a ref after `@`, none of them empty. A value with a `${{ }}` expression is not checked here.

## Why

GitHub cannot resolve such a value to a workflow file, so it rejects the workflow before any job runs. Two mistakes are common: a ref on a local workflow, which always runs from the caller's commit, and a repository workflow without the path to its file.

## Failing example

```yaml
on: push
jobs:
  local:
    uses: ./.github/workflows/build.yml@main
  remote:
    uses: octo-org/example-repo@v1
```

```text
test.yaml:4:11: reusable workflow call "./.github/workflows/build.yml@main" at "uses" is not following the format "owner/repo/path/to/workflow.yml@ref" nor "./path/to/workflow.yml". see https://docs.github.com/en/actions/learn-github-actions/reusing-workflows for more details [invalid-reusable-workflow-ref]
test.yaml:6:11: reusable workflow call "octo-org/example-repo@v1" at "uses" is not following the format "owner/repo/path/to/workflow.yml@ref" nor "./path/to/workflow.yml". see https://docs.github.com/en/actions/learn-github-actions/reusing-workflows for more details [invalid-reusable-workflow-ref]
```

## Passing example

```yaml file=.github/workflows/build.yml
on: workflow_call
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: make
```

```yaml
on: push
jobs:
  local:
    uses: ./.github/workflows/build.yml
  remote:
    uses: octo-org/example-repo/.github/workflows/release.yml@v1
```
