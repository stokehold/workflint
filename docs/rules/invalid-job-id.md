# invalid-job-id

| Kind | Category | Default severity |
|---|---|---|
| `id` | correctness | error |

A job ID, or a job ID in `needs`, does not start with an ASCII letter or `_`, or has a character other than an ASCII letter, a digit, `-`, or `_`.

The rule checks the key of each job and each entry of its `needs`. It skips an ID that contains a `${{ }}`. An empty ID is reported by [empty-string](empty-string.md) instead.

## Why

GitHub accepts only job IDs that start with a letter or `_` and contain only alphanumeric characters, `-`, or `_`. It refuses to run a workflow with any other job ID.

## Failing example

```yaml
on: push
jobs:
  build-v1.2:
    runs-on: ubuntu-latest
    steps:
      - run: make
  2d-game:
    runs-on: ubuntu-latest
    steps:
      - run: make game
```

```text
test.yaml:3:3: invalid job ID "build-v1.2". job ID must start with a letter or _ and contain only alphanumeric characters, -, or _ [invalid-job-id]
test.yaml:7:3: invalid job ID "2d-game". job ID must start with a letter or _ and contain only alphanumeric characters, -, or _ [invalid-job-id]
```

## Passing example

```yaml
on: push
jobs:
  build-v1_2:
    runs-on: ubuntu-latest
    steps:
      - run: make
  game_2d:
    needs: [build-v1_2]
    runs-on: ubuntu-latest
    steps:
      - run: make game
```
