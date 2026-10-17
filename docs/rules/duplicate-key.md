# duplicate-key

| Kind | Category | Default severity |
|---|---|---|
| `syntax-check` | correctness | error |

A mapping repeats a key.

The first entry is kept, and the duplicate's value is not parsed. Some sections compare their keys case insensitively, such as job IDs, `env`, and matrix keys, and the message then says so.

## Why

GitHub rejects a mapping with a repeated key. Two job IDs that differ only in case are the same job.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: ./test.sh
  TEST:
    runs-on: windows-latest
    steps:
      - run: ./test.sh
```

```text
test.yaml:7:3: key "TEST" is duplicated in "jobs" section. previously defined at line:3,col:3. note that this key is case insensitive [duplicate-key]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: ./test.sh
  test-windows:
    runs-on: windows-latest
    steps:
      - run: ./test.sh
```
