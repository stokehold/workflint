# Command line

workflint takes actionlint's flags in the syntax of Go's `flag` package, keeps actionlint's exit codes, and adds a few flags of its own (ADR 0003). A script that runs `actionlint` with flags keeps working when it runs `workflint` instead. `workflint -h` prints the usage and the list of flags.

```sh
workflint [check] [FLAGS] [FILES...] [-]
```

`check` is the default command and may be left out. Each file argument is linted, and its errors are printed in the order of the arguments. workflint finds the project of each file and checks the file's local actions and local reusable workflows against that project.

Without file arguments, workflint finds the project of the current directory and lints every file under its `.github/workflows` directory, subdirectories included, whose name ends in `.yml` or `.yaml`. The names are case-sensitive, so `ci.YAML` is skipped. It prints their errors in the order of their paths. Without a project, or without such a file, workflint prints `no project was found in any parent directories of "<dir>". ...` or `no YAML file was found in "<dir>"` and exits with code 3.

## Directories and globs

A directory argument, such as `workflint .github`, lints every file under it whose name ends in `.yml` or `.yaml`, subdirectories included, in the order of their paths. Symbolic links inside it are not followed. A directory without such a file prints `no YAML file was found in "<dir>"` and exits with code 3. actionlint stops at any directory argument ([differences.md](differences.md#directory-and-glob-arguments)).

An argument that does not exist and contains `*`, `?`, `[`, or `{` is a glob, and workflint expands it itself. So `workflint '.github/workflows/*.yml'` works when quoted, and on Windows shells that do not expand globs. The syntax is that of [`paths` globs](config.md#paths): `*` does not match `/`, and `**` matches any number of directories, so `'**/*.yaml'` finds files at any depth. A glob matches from the current directory, and it lints the matched files whose names end in `.yml` or `.yaml`, in the order of their paths. On Windows, `\` separates directories in a glob. A glob that matches no such file prints `no YAML file matches the glob "<glob>"`, and a malformed one, such as `a.{yml`, prints `invalid glob pattern "<glob>"`. Both exit with code 3. A file whose name contains these characters is linted as a file when it exists.

Discovery, directories, and globs skip the files that the repository's `.gitignore` files or `.git/info/exclude` name, in a Git or Jujutsu repository, and those that the [`exclude` key](config.md#exclude) of the config file names. The global excludes file of the user, `core.excludesFile`, is not read, so that every machine lints the same files ([differences.md](differences.md#gitignore)). The `.git`, `.jj`, and `.hg` directories are never walked. A file given by name is always linted, even when `.gitignore` or `exclude` names it.

With `-` as the only argument, workflint reads a workflow from stdin and names it `<stdin>`, or the value of `-stdin-filename`. If a file exists at that name, the workflow gets that file's project, so its local actions and reusable workflows are checked. `-` among other arguments is a file named `-`.

## Project root

The project root of a file is the nearest directory above it that has a `.github/workflows` directory and a `.git`, `.jj`, or `.hg` marker. `.git` may be a file, as in a Git worktree or submodule. If no directory above the file has both, the nearest one with a `.github` directory is the root. actionlint accepts only `.git` and has no fallback ([differences.md](differences.md#project-root-detection)).

A file without a project has no local actions or local reusable workflows to check. Printed paths are relative to the current directory, so a run from a subdirectory prints `../.github/workflows/ci.yaml`.

## Flag syntax

The rules are those of Go's `flag` package:

- `-flag` and `--flag` are the same flag.
- A flag with a value takes it as `-flag=value` or `-flag value`. The value may be empty, as in `-shellcheck=`.
- A boolean flag is `-flag`, `-flag=true`, or `-flag=false`. `-flag false` does not turn it off: `false` is the first file. The values `1`, `t`, `T`, `TRUE`, `true`, `True` are true, and `0`, `f`, `F`, `FALSE`, `false`, `False` are false.
- When a flag is given twice, the last value wins, except for `-ignore`, which collects every value.
- The flags end at `--` or at the first argument that is not a flag. `workflint a.yaml -oneline` lints a file named `-oneline`.
- `-h` and `-help` print the usage to stderr and exit with code 0.
- An unknown flag, a flag without its value, or an invalid boolean prints Go's error, such as `flag provided but not defined: -x`, and the usage to stderr, and exits with code 2.

## Flags

| Flag | What it does |
|---|---|
| `-ignore PATTERN` | Drops the errors whose message the regular expression matches. The pattern is unanchored and sees only the message, not the path or the rule ID. Give the flag again for more patterns. The pattern has Go's syntax, so `\d`, `\w`, `\s`, and `\b` match ASCII only. An invalid pattern prints Go's error and exits with code 3. |
| `-oneline` | Prints one line per error, without the source snippet. It has no effect with `-format`. |
| `-color` | Prints color even when stdout is not a terminal, `NO_COLOR` is set, or `TERM` is `dumb`. |
| `-no-color` | Prints no color. It wins over `-color`. |
| `-verbose` | Writes log lines that start with `verbose: ` to stderr. |
| `-debug` | Writes the verbose lines and debug lines that start with `[Linter] ` to stderr. |
| `-version` | Prints the version and exits with code 0. See [Version](#version). |
| `-quiet`, `-q` | Prints only errors and fatal messages: no summary and no log. |
| `-silent` | Prints only fatal messages. The exit code says whether errors were found. |
| `-summary` | Prints one line with the number of errors by severity to stderr, such as `3 problems in 2 files: 2 errors, 1 warning, 0 info`. |
| `-format TEMPLATE` | Prints the errors through a Go template. See [Format templates](#format-templates). `-format sarif` prints them as SARIF instead ([sarif.md](sarif.md)). |
| `-config-file PATH`, `--config PATH` | Lints every file with the config file at `PATH`, in place of the config file of each project. A file that cannot be read or decoded exits with code 3. See [config.md](config.md). |
| `-init-config` | Writes the default config file to `.github/workflint.yaml` in the project of the working directory and exits. The file arguments are ignored. See [config.md](config.md#creating-a-config-file). |
| `-stdin-filename NAME` | Names the workflow read from stdin. The default is `<stdin>`. If a file exists at `NAME`, the workflow gets that file's project. |
| `-shellcheck COMMAND` | The shellcheck that checks `run:` scripts ([shellcheck-issue](rules/shellcheck-issue.md)): a name to look up on `PATH`, a path, or a command line whose first word is one of these. The default is `shellcheck`, and an empty value turns shellcheck off. Without the flag, a missing `shellcheck` prints one warning to stderr and the run goes on. A shellcheck that the flag names but that is not found exits with code 3, and so does a missing shellcheck that a config file sets keys for ([config.md](config.md#shellcheck)). |
| `-shellcheck-cache` | Keeps the findings of shellcheck in a cache on disk, so that a later run does not check the same script again. The config key `shellcheck.cache` does the same for one project. See [config.md](config.md#shellcheckcache). |
| `-jobs N` | Runs at most `N` external tool processes, such as shellcheck, pyflakes, and ruff, at once. The default is the number of CPUs. |
| `-pyflakes COMMAND` | The pyflakes that checks the scripts of `shell: python` steps ([pyflakes-issue](rules/pyflakes-issue.md)), looked up as for `-shellcheck`. With a value, that pyflakes runs whatever the `python.linter` of the config file says, and a pyflakes that is not found exits with code 3. An empty value turns the Python check off. Without the flag, `python.linter` picks ruff or pyflakes ([config.md](config.md#pythonlinter)). |

`check`, `--config`, `-quiet`, `-q`, `-silent`, `-summary`, `-jobs`, and `-shellcheck-cache` are workflint's own. [differences.md](differences.md#workflint-flags) lists them with the other differences.

## Output

Errors go to stdout. Logs, warnings, the usage, the summary, and fatal messages such as `could not read "x.yaml": ...` go to stderr. A warning starts with `warning: `, such as the one about a missing shellcheck, about a Python script that neither ruff nor pyflakes can check, or about an unknown key in an actionlint config file ([config.md](config.md#unknown-keys)). `-quiet` and `-silent` hide warnings.

The default format prints the header line `path:line:col: message [rule-id]`, the source line, and a line with `^~~~` under the span. `-oneline` prints only the header line. The layout is actionlint's, except that the brackets hold workflint's rule ID instead of actionlint's kind ([differences.md](differences.md#rule-id-in-brackets)). The problem matcher in actionlint's `.github/actionlint-matcher.json` matches the header lines with and without color.

Color is on when stdout is a terminal, `NO_COLOR` is unset or empty, and `TERM` is not `dumb`. `-color` turns it on and `-no-color` turns it off, whatever the environment. On a Windows console that does not understand escape sequences, workflint sets the colors through the console's own calls, as actionlint does.

## Format templates

`-format` takes a template in the syntax of Go's `text/template` package and prints the errors through it, as actionlint does. Templates written for actionlint work unchanged:

```sh
workflint -format '{{json .}}'
workflint -format '{{range $err := .}}{{json $err}}{{end}}'
workflint -format '{{range $err := .}}::error file={{$err.Filepath}},line={{$err.Line}},col={{$err.Column}}::{{$err.Message}}\n{{end}}'
```

The template must contain `{{`, except for `sarif`, which names the [SARIF output](sarif.md). Before parsing, workflint replaces the escapes `\a`, `\b`, `\f`, `\n`, `\r`, `\t`, `\v`, and `\\` in the flag value with the characters they stand for, so `-format '{{range .}}{{.Message}}\n{{end}}'` prints one message per line. The template runs once, over the list of the errors of every file in the order of the output. The output has no color, and `-oneline` changes nothing.

Each error has these fields:

| Field | Value |
|---|---|
| `Message` | The message. |
| `Filepath` | The path of the file, as the default format prints it, or the name of the workflow read from stdin. |
| `Line`, `Column` | The 1-based line and column of the start of the error. The column counts Unicode code points. |
| `Kind` | The actionlint kind of the rule, such as `expression` or `syntax-check`. |
| `Rule` | The workflint rule ID, such as `undefined-property`, which the default format prints in brackets. |
| `Snippet` | The source line, a line break, and the `^~~~` line of the default format. It is empty when the line does not exist. |
| `EndColumn` | The column of the last character under the `^~~~`, counted as `Column` is, or `Column` when there is no snippet. |

`Rule` is workflint's own, and `Snippet` and `EndColumn` cover the whole span of the error ([differences.md](differences.md#column-units)). The other fields have actionlint's values ([differences.md](differences.md#rule-id-in-brackets)).

The template can call Go's built-in functions and these:

| Function | Result |
|---|---|
| `json VALUE` | `VALUE` as JSON, as Go's `encoding/json` writes it: `<`, `>`, and `&` escaped, and a line break at the end. An error has actionlint's seven keys `message`, `filepath`, `line`, `column`, `kind`, `snippet`, and `end_column`, without `filepath` and `snippet` when they are empty. No errors give `[]`. |
| `replace S OLD NEW ...` | `S` with each `OLD` replaced by the `NEW` after it, as Go's `strings.NewReplacer`. |
| `toPascalCase S` | `S` split at each character that is not an ASCII letter or digit, with each part capitalized, as in `foo-bar` to `FooBar`. |
| `getVersion` | The version, as the first line of `-version` prints it. |
| `allKinds` | The kinds of the rules that the run uses, sorted by name, each with `Name` and `Description`. `shellcheck` is in the list only when shellcheck runs ([differences.md](differences.md#all-kinds-of-the-run)). |

The SARIF template of actionlint, `testdata/format/sarif_template.txt`, builds its `rules` from `allKinds` and its `results` from the fields.

A template that does not parse prints Go's error, such as `template "{{xxx}}" to format error messages could not be parsed: template: error formatter:1: function "xxx" not defined`, and exits with code 3 before any file is read. A template that fails while it runs, such as `{{.Foo}}`, prints its output up to the failing action, then the error to stderr, and exits with code 3 ([differences.md](differences.md#template-execution-errors)). Otherwise the exit code says whether errors were found, as with the other formats. `-silent` prints nothing, but still checks that the template parses. [differences.md](differences.md#format-template-subset) lists where the template language differs from Go's.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | No errors, also when `-ignore` or the config dropped every error. `-h`, `-help`, `-version`, and a successful `-init-config` exit with 0 too. |
| 1 | At least one error. |
| 2 | The command line is invalid. |
| 3 | A fatal error: a file, stdin, or a config file cannot be read, a run without file arguments finds no project or no workflow file, a directory argument has no workflow file, a glob argument matches none or is malformed, a config file, an `-ignore` pattern, or the `-format` template is invalid, the shellcheck that `-shellcheck` names is not found, `-init-config` fails, the `-format` template fails while it runs, the output cannot be written, or workflint hit a bug in itself and reported an `internal-error`. A shellcheck that fails on a script is no fatal error: it gives a [shellcheck-failed](rules/shellcheck-failed.md) diagnostic. |

## Version

`workflint -version` prints three lines to stdout:

```text
0.1.0
installed by building from source
built with rustc 1.97.0 for linux/amd64
```

The first line is the bare version, so `workflint -version | head -n 1` gives it. The second says how workflint was installed. The `WORKFLINT_INSTALLED_FROM` environment variable sets it, which the package launchers use, and else the release pipeline sets it when it builds the binary. A build from source prints `installed by building from source`. The third line names the compiler and the target, with Go's names for the operating system and the architecture, which the release archives use too: `linux`, `darwin`, or `windows`, and `amd64` or `arm64`.

The documentation links in `-help` point at the files of the release for a binary of the release pipeline, and at `main` for any other build.

## Logs

`-verbose` writes what workflint does to stderr. A run without file arguments in `/home/me/repo` prints:

```text
verbose: Linting all workflow files in repository: /home/me/repo
verbose: Detected project: /home/me/repo
verbose: Collected 2 YAML files
verbose: Linting 2 files
verbose: Linting .github/workflows/ci.yaml
verbose: Using project at /home/me/repo
verbose: Found total 0 errors in 1 ms for .github/workflows/ci.yaml
verbose: Linting .github/workflows/release.yaml
verbose: Using project at /home/me/repo
verbose: Found total 2 errors in 1 ms for .github/workflows/release.yaml
verbose: Found 2 errors in 2 files
```

The files are linted in parallel. The lines of one file are written together when the file is done, so they never mix with the lines of another file, and the files come in the order they finish. `-debug` adds the config file of each file, such as `[Linter] Config: /home/me/repo/.github/workflint.yaml` or `[Linter] No config was found`, lines such as `[Linter] unused-anchor found 2 errors` for each rule that found errors, and one line for each error that `-ignore` or the config dropped. A run that reads stdin starts with `verbose: Reading the input from stdin`. The times in the log vary from run to run, and the log is not part of the output that ADR 0013 keeps deterministic.

## Environment variables

| Variable | Effect |
|---|---|
| `NO_COLOR` | When not empty, turns color off unless `-color` is given. |
| `TERM` | `dumb` turns color off unless `-color` is given. |
| `WORKFLINT_INSTALLED_FROM` | The second line of `-version`. |
| `RAYON_NUM_THREADS` | The number of files linted at once. The output is the same for every value. |
| `PATH` | Where `-shellcheck` looks up a bare name. |
| `SHELLCHECK_OPTS` | Options that shellcheck reads. workflint passes its environment on to shellcheck. |
