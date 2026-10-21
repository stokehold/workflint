# Command line

workflint takes actionlint's flags in the syntax of Go's `flag` package, keeps actionlint's exit codes, and adds a few flags of its own (ADR 0003). A script that runs `actionlint` with flags keeps working when it runs `workflint` instead. `workflint -h` prints the usage and the list of flags.

```sh
workflint [check] [FLAGS] [FILES...] [-]
```

`check` is the default command and may be left out. Each file argument is linted, and its errors are printed in the order of the arguments.

This version lints only the files it is given. Running without arguments, which lints the files in `.github/workflows` of the current repository, and reading a workflow from stdin with `-` come with M5.1 (#246). Until then both exit with code 2.

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
| `-oneline` | Prints one line per error, without the source snippet. |
| `-color` | Prints color even when stdout is not a terminal, `NO_COLOR` is set, or `TERM` is `dumb`. |
| `-no-color` | Prints no color. It wins over `-color`. |
| `-verbose` | Writes log lines that start with `verbose: ` to stderr. |
| `-debug` | Writes the verbose lines and debug lines that start with `[Linter] ` to stderr. |
| `-version` | Prints the version and exits with code 0. See [Version](#version). |
| `-quiet`, `-q` | Prints only errors and fatal messages: no summary and no log. |
| `-silent` | Prints only fatal messages. The exit code says whether errors were found. |
| `-summary` | Prints one line with the number of errors by severity to stderr, such as `3 problems in 2 files: 2 errors, 1 warning, 0 info`. |
| `-format TEMPLATE` | Accepted and ignored until M5.8 (#251) prints errors with a Go template. |
| `-config-file PATH` | Accepted and ignored until M5.2 (#247) reads the config. |
| `-init-config` | Accepted and ignored until M5.2 (#247) writes a default config. |
| `-stdin-filename NAME` | Accepted and ignored until M5.1 (#246) reads stdin. The default is `<stdin>`. |
| `-shellcheck COMMAND` | The shellcheck that checks `run:` scripts ([shellcheck-issue](rules/shellcheck-issue.md)): a name to look up on `PATH`, a path, or a command line whose first word is one of these. The default is `shellcheck`, and an empty value turns shellcheck off. Without the flag, a missing `shellcheck` prints one warning to stderr and the run goes on. A shellcheck that the flag names but that is not found exits with code 3. |
| `-jobs N` | Runs at most `N` external tool processes, such as shellcheck, at once. The default is the number of CPUs. |
| `-pyflakes COMMAND` | Accepted and ignored until M5.12 (#249) runs pyflakes. The default is `pyflakes`, and an empty value turns pyflakes off. |

`check`, `-quiet`, `-q`, `-silent`, `-summary`, and `-jobs` are workflint's own. [differences.md](differences.md#workflint-flags) lists them with the other differences.

## Output

Errors go to stdout. Logs, the usage, the summary, the warning about a missing shellcheck, and fatal messages such as `could not read "x.yaml": ...` go to stderr. `-quiet` and `-silent` hide the warning.

The default format prints the header line `path:line:col: message [rule-id]`, the source line, and a line with `^~~~` under the span. `-oneline` prints only the header line. The layout is actionlint's, except that the brackets hold workflint's rule ID instead of actionlint's kind ([differences.md](differences.md#rule-id-in-brackets)). The problem matcher in actionlint's `.github/actionlint-matcher.json` matches the header lines with and without color.

Color is on when stdout is a terminal, `NO_COLOR` is unset or empty, and `TERM` is not `dumb`. `-color` turns it on and `-no-color` turns it off, whatever the environment. On a Windows console that does not understand escape sequences, workflint sets the colors through the console's own calls, as actionlint does.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | No errors, also when `-ignore` dropped every error. `-h`, `-help`, and `-version` exit with 0 too. |
| 1 | At least one error. |
| 2 | The command line is invalid. Until M5.1 (#246), also no file argument, or `-` alone. |
| 3 | A fatal error: a file cannot be read, an `-ignore` pattern is invalid, the shellcheck that `-shellcheck` names is not found, the output cannot be written, or workflint hit a bug in itself and reported an `internal-error`. A shellcheck that fails on a script is no fatal error: it gives a [shellcheck-failed](rules/shellcheck-failed.md) diagnostic. |

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

`-verbose` writes what workflint does to stderr:

```text
verbose: Linting 2 files
verbose: Linting .github/workflows/ci.yaml
verbose: Using project at /home/me/repo
verbose: Found total 0 errors in 1 ms for .github/workflows/ci.yaml
verbose: Linting .github/workflows/release.yaml
verbose: Using project at /home/me/repo
verbose: Found total 2 errors in 1 ms for .github/workflows/release.yaml
verbose: Found 2 errors in 2 files
```

The files are linted in parallel. The lines of one file are written together when the file is done, so they never mix with the lines of another file, and the files come in the order they finish. `-debug` adds lines such as `[Linter] unused-anchor found 2 errors` for each rule that found errors, and one line for each error that `-ignore` dropped. The times in the log vary from run to run, and the log is not part of the output that ADR 0013 keeps deterministic.

## Environment variables

| Variable | Effect |
|---|---|
| `NO_COLOR` | When not empty, turns color off unless `-color` is given. |
| `TERM` | `dumb` turns color off unless `-color` is given. |
| `WORKFLINT_INSTALLED_FROM` | The second line of `-version`. |
| `RAYON_NUM_THREADS` | The number of files linted at once. The output is the same for every value. |
| `PATH` | Where `-shellcheck` looks up a bare name. |
| `SHELLCHECK_OPTS` | Options that shellcheck reads. workflint passes its environment on to shellcheck. |
