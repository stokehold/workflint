# unknown-runner-label

| Kind | Category | Default severity |
|---|---|---|
| `runner-label` | correctness | error |

A `runs-on` label is not a known runner label.

A known label is a GitHub-hosted label such as `ubuntu-latest`, one of the labels every self-hosted runner gets (`self-hosted`, `x64`, `arm`, `arm64`, `linux`, `macos`, `windows`), or a label that matches a pattern of the config's `self-hosted-runner.labels`. Labels of the third-party runner providers Blacksmith, Depot, Namespace, RunsOn, Ubicloud, and WarpBuild are known too, in the formats that their documentation gives, such as `depot-ubuntu-24.04-arm` or RunsOn's `runs-on=${{ github.run_id }}/runner=2cpu-linux-x64`. The message does not list them. Built-in labels compare without case. Config patterns use Go's `path.Match` syntax and compare with case. A label of the exact form `${{ matrix.<prop> }}` is checked through each string value of the matrix row `<prop>` and of `<prop>` in each `include` item, and reported at the value.

Any other label with `${{ }}` is evaluated to the strings it can be, and each one is checked and reported at the label. The evaluator knows literals, `matrix.<prop>` in each combination of the matrix, `format()`, `&&`, `||`, `!`, `==`, `!=`, and text around `${{ }}`. A condition it cannot work out, such as `github.repository == 'o/r'` in `cond && 'a' || 'b'`, gives both `a` and `b`. A label with a value it cannot work out, such as `${{ inputs.os }}`, is skipped. So is a label of a matrix with more than 256 combinations, and a label that can be more than 256 strings.

## Why

GitHub queues a job whose labels no runner has until it times out. An unknown label is often a typo, or a GitHub-hosted image that GitHub has retired, such as `macos-10.15`. A custom self-hosted label belongs in the config so the check knows it.

## Failing examples

```yaml
on: push
jobs:
  test:
    runs-on: macos-10.15
    steps:
      - run: echo
```

```text
test.yaml:4:14: label "macos-10.15" is unknown. available labels are "windows-latest", "windows-latest-8-cores", "windows-2025", "windows-2025-vs2026", "windows-2022", "windows-11-arm", "windows-11-vs2026-arm", "ubuntu-slim", "ubuntu-latest", "ubuntu-latest-4-cores", "ubuntu-latest-8-cores", "ubuntu-latest-16-cores", "ubuntu-26.04", "ubuntu-26.04-arm", "ubuntu-24.04", "ubuntu-24.04-arm", "ubuntu-22.04", "ubuntu-22.04-arm", "macos-latest", "macos-latest-xlarge", "macos-latest-large", "xcode-27-xlarge", "xcode-27", "macos-26-intel", "macos-26-xlarge", "macos-26-large", "macos-26", "macos-15-intel", "macos-15-xlarge", "macos-15-large", "macos-15", "macos-14-xlarge", "macos-14-large", "macos-14", "self-hosted", "x64", "arm", "arm64", "linux", "macos", "windows". if it is a custom label for self-hosted runner, set list of labels in actionlint.yaml config file [unknown-runner-label]
```

A matrix value is reported where the matrix sets it:

```yaml
on: push
jobs:
  test:
    strategy:
      matrix:
        os: [ubuntu-latest, linux-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - run: echo
```

```text
test.yaml:6:29: label "linux-latest" is unknown. available labels are "windows-latest", "windows-latest-8-cores", "windows-2025", "windows-2025-vs2026", "windows-2022", "windows-11-arm", "windows-11-vs2026-arm", "ubuntu-slim", "ubuntu-latest", "ubuntu-latest-4-cores", "ubuntu-latest-8-cores", "ubuntu-latest-16-cores", "ubuntu-26.04", "ubuntu-26.04-arm", "ubuntu-24.04", "ubuntu-24.04-arm", "ubuntu-22.04", "ubuntu-22.04-arm", "macos-latest", "macos-latest-xlarge", "macos-latest-large", "xcode-27-xlarge", "xcode-27", "macos-26-intel", "macos-26-xlarge", "macos-26-large", "macos-26", "macos-15-intel", "macos-15-xlarge", "macos-15-large", "macos-15", "macos-14-xlarge", "macos-14-large", "macos-14", "self-hosted", "x64", "arm", "arm64", "linux", "macos", "windows". if it is a custom label for self-hosted runner, set list of labels in actionlint.yaml config file [unknown-runner-label]
```

An evaluated label is reported at the label, once for each value that is unknown. Here the condition is not known, so the label may be `ubuntu-20.04`, an image that GitHub has retired:

```yaml
on: push
jobs:
  test:
    runs-on: ${{ github.repository == 'owner/repo' && 'self-hosted' || 'ubuntu-20.04' }}
    steps:
      - run: echo
```

```text
test.yaml:4:14: label "ubuntu-20.04" is unknown. available labels are "windows-latest", "windows-latest-8-cores", "windows-2025", "windows-2025-vs2026", "windows-2022", "windows-11-arm", "windows-11-vs2026-arm", "ubuntu-slim", "ubuntu-latest", "ubuntu-latest-4-cores", "ubuntu-latest-8-cores", "ubuntu-latest-16-cores", "ubuntu-26.04", "ubuntu-26.04-arm", "ubuntu-24.04", "ubuntu-24.04-arm", "ubuntu-22.04", "ubuntu-22.04-arm", "macos-latest", "macos-latest-xlarge", "macos-latest-large", "xcode-27-xlarge", "xcode-27", "macos-26-intel", "macos-26-xlarge", "macos-26-large", "macos-26", "macos-15-intel", "macos-15-xlarge", "macos-15-large", "macos-15", "macos-14-xlarge", "macos-14-large", "macos-14", "self-hosted", "x64", "arm", "arm64", "linux", "macos", "windows". if it is a custom label for self-hosted runner, set list of labels in actionlint.yaml config file [unknown-runner-label]
```

## Passing example

```yaml
on: push
jobs:
  test:
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - run: echo
  build:
    runs-on: [self-hosted, linux, arm64]
    steps:
      - run: echo
  provider:
    runs-on: blacksmith-4vcpu-ubuntu-2404
    steps:
      - run: echo
  release:
    strategy:
      matrix:
        include:
          - {os: ubuntu, version: '24.04'}
          - {os: macos, version: '15'}
    runs-on: ${{ matrix.os }}-${{ matrix.version }}
    steps:
      - run: echo
```

## Differences from actionlint

- [An invalid value is reported once](../differences.md#invalid-value-reported-once)
- [Expressions in `runs-on` labels are evaluated](../differences.md#runs-on-expressions)
- [Labels of runner providers, and the system that custom labels name](../differences.md#runner-provider-labels)
