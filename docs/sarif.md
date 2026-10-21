# SARIF output

`-format sarif` prints the errors as a SARIF 2.1.0 log, the format that GitHub code scanning reads:

```sh
workflint -format sarif > workflint.sarif
```

The exit code is the same as with the other formats: 0 without errors, 1 with errors, and 3 after a fatal error ([cli.md](cli.md#exit-codes)). `-silent` prints nothing, and `-oneline` and the color flags change nothing.

## What the log holds

The log has one run. Its tool is `workflint`, with the version that `-version` prints and a link to the repository.

The tool's `rules` list has an entry for every rule workflint has, whether it ran or not, sorted by rule ID. Each entry has:

- `id`: the rule ID, such as `undefined-property`.
- `shortDescription`: the one-line description of the rule.
- `helpUri`: the rule's page under [rules/](rules/README.md).
- `defaultConfiguration`: the default level of the rule, and `"enabled": false` for a rule outside the `correctness` category, which is off unless the config turns it on.
- `properties.tags`: the rule's category, such as `correctness` or `style`.

Each error gives one result, in the order of the default output:

- `ruleId` and `ruleIndex` name the rule and its place in `rules`.
- `level` is `error`, `warning`, or `note` for the severities `error`, `warning`, and `info`. A severity set in the config shows here.
- `message.text` is the message of the default output. When the error has a hint on how to fix it, such as `did you mean "github"?`, the hint follows after a blank line, as `help: did you mean "github"?`.
- `locations` holds the file and the region of the error: its start and end line and column. The region covers the whole token the error is about.
- `relatedLocations` holds the other places the error refers to, each with a note, such as each alias through which a YAML anchor repeats the error.
- `partialFingerprints` holds a fingerprint under the key `workflint/v1`. See [Fingerprints](#fingerprints).

Lines and columns count from 1. Columns count UTF-16 code units, as the run's `columnKind` of `utf16CodeUnits` says, where the default output counts Unicode code points. The two differ after a character such as an emoji, which is one code point and two code units.

A file path that the default output prints relative to the working directory becomes a URI relative to `%SRCROOT%`, the root of the sources. An absolute path becomes a `file:` URI. Characters other than letters, digits, `-`, `.`, `_`, `~`, and `/` are percent-encoded, and `\` becomes `/`. The workflow read from stdin is `%3Cstdin%3E`, the encoded `<stdin>`, unless `-stdin-filename` names it.

## Fingerprints

Code scanning matches the results of one upload with the alerts of earlier ones by their fingerprints, so an alert stays open, or stays dismissed, across commits. The fingerprint has two parts, such as `3648f5a87fde5600:1`:

- The first 16 hexadecimal digits of the SHA-256 hash of the rule ID, the file path, and the text of the error's region. For an error whose region is empty, the rest of its line takes the place of the region's text.
- The number of errors before it in the same file with the same hash, plus 1, so that two equal errors in one file keep apart.

Line numbers and columns are not part of it. Adding or removing lines elsewhere in the file keeps the fingerprint. Editing the token the error is about, renaming the file, or adding an equal error earlier in the file changes it.

## Uploading to code scanning

Run workflint in a workflow, then upload the log with the `upload-sarif` action of `github/codeql-action`. The job needs the `security-events: write` permission. A sketch, with workflint already installed on the runner:

```yaml
on:
  push:
    branches: [main]
  pull_request:

permissions:
  contents: read

jobs:
  workflint:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      security-events: write
    steps:
      - uses: actions/checkout@<commit SHA> # v5
        with:
          persist-credentials: false
      - run: workflint -format sarif > workflint.sarif
      - uses: github/codeql-action/upload-sarif@<commit SHA> # v4
        if: ${{ !cancelled() }}
        with:
          sarif_file: workflint.sarif
          category: workflint
```

workflint exits with code 1 when it finds errors, which fails the lint step. `if: ${{ !cancelled() }}` still uploads the log then, so the job fails and the alerts appear in code scanning. To keep the job green and let code scanning alone report the errors, add `|| [ $? -eq 1 ]` to the `run:` line.

The log has no `automationDetails`, so the `category` input names the run. A second upload with another category, such as one per directory, keeps its own alerts.
