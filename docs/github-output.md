# GitHub output

Two formats are made for a workflint run inside GitHub Actions. `github` writes workflow commands, which the runner turns into annotations on the lines of the pull request. `summary` writes a Markdown report for the job summary. `-output` writes extra formats to files in the same run, so one step can print annotations, add a job summary, and save a SARIF file:

```yaml
- run: >
    workflint -format github
    -output "summary=$GITHUB_STEP_SUMMARY"
    -output sarif=workflint.sarif
```

The exit code is the same as with the other formats: 0 without errors, 1 with errors, and 3 after a fatal error ([cli.md](cli.md#exit-codes)).

workflint does not pick `github` by itself when `GITHUB_ACTIONS` is `true`. Without `-format`, a run in Actions prints the default format, as it does anywhere else.

## Annotations

`-format github` prints one workflow command per error:

```text
::error file=.github/workflows/ci.yaml,line=12,endLine=12,col=14,endColumn=27,title=unknown-runner-label::label "ubuntu-lates" is unknown. ...
```

- The command is `::error`, `::warning`, or `::notice` for the severities `error`, `warning`, and `info`.
- `file` is the path that the default output prints. GitHub reads it from the root of the repository, so run workflint there, or the annotations do not land on the files.
- `line` and `endLine` are the first and the last line of the error's span. `col` and `endColumn` are its first column and the column just after it, counted in Unicode code points as in the default output. GitHub takes columns only for a span on one line, so an error over several lines has no `col` and `endColumn`.
- `title` is the rule ID.
- The message follows the second `::`. When the error has a hint on how to fix it, the hint follows after a blank line, as `help: did you mean "github"?`.

workflint escapes the values as GitHub's toolkit does. In the message, `%` becomes `%25`, CR becomes `%0D`, and LF becomes `%0A`. In a property, such as `file`, `:` becomes `%3A` and `,` becomes `%2C` too.

GitHub shows at most 10 annotations of each type, error, warning, and notice, for one step, and drops the rest. So the commands come in this order: every error, every warning, then every info notice. Within a severity they keep the order of the default output. When a severity has more than 10 errors, a notice after the warnings says how many have no annotation:

```text
::notice title=workflint::7 of 27 problems have no annotation, because GitHub shows at most 10 annotations of each type per step. The job summary and the log of this step list every problem.
```

That notice comes before the info notices, so it is among the 10 notices that GitHub shows. It counts itself as one of them. GitHub also caps a job at 50 annotations over all of its steps, and workflint cannot see the other steps, so the count leaves that cap out.

## Job summary

`-format summary` prints Markdown: a heading, the line that `-summary` prints, and a table with the severity, the location, the rule ID with a link to its page, and the message of each error, in the order of the default output. A run without errors prints the heading and the line only.

```markdown
## workflint

2 problems in 1 file: 2 errors, 0 warnings, 0 info

| Severity | Location | Rule | Message |
| --- | --- | --- | --- |
| error | anchors\.yaml\:3\:6 | [unused-anchor](https://github.com/stokehold/workflint/blob/main/docs/rules/unused-anchor.md) | anchor \"b\" is defined but not used |
```

workflint puts a backslash before each ASCII punctuation character of the location and the message, so text such as `${{ a | b }}` or `<b>` shows as written and breaks neither the table nor the page. A line break becomes `<br>`. The help text follows the message after `<br>help: `.

GitHub rejects a step summary over 1 MiB. When the table would pass that, workflint leaves out the rows after the limit and ends with a line such as `12 more problems are not listed, because GitHub limits the summary of a step to 1 MiB.`

## Several formats in one run

`-output FORMAT=PATH` writes the errors in the format `FORMAT` to the file `PATH`. `FORMAT` is one of `github`, `json`, `jsonl`, `sarif`, and `summary`. Give the flag once for each file. The format of `-format`, or the default format, still goes to stdout.

- Each file holds what a run with that format alone prints to stdout.
- The file is created, or emptied if it exists. A `summary` file is appended to instead, because the file in `GITHUB_STEP_SUMMARY` collects the summaries of every command in the step.
- workflint writes the files after stdout. `-silent` keeps stdout empty, but the files are still written.
- A value without `=`, an unknown format, or an empty path prints an error and exits with code 3 before any file is read. A file that cannot be written prints `could not write the output to "PATH": ...` and exits with code 3, after the other files are written.
- `-output` takes no template. Pass a template with `-format`.
