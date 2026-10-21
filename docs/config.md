# Config file

workflint reads actionlint's config file unchanged, and a config file of its own at `.github/workflint.yaml` (ADR 0010). This page lists where workflint looks for the file and every key that works today. The keys that workflint adds, `exclude`, `python`, `shellcheck`, and `tool-timeout`, work in an actionlint config file too, and actionlint ignores them.

## Where workflint looks

Each file is linted with the config of its project. The project root is the nearest directory above the file that has a `.github/workflows` directory and a `.git`, `.jj`, or `.hg` marker, or else the nearest directory with a `.github` directory ([differences.md](differences.md#project-root-detection)). workflint uses the first file it finds, in this order, and does not merge them:

1. The file given by `-config-file PATH`, or by `--config PATH`, which is the same flag. It applies to every linted file, also to files outside any project, and the projects' own config files are not read.
2. `.github/workflint.yaml`, then `.github/workflint.yml`, in the project root.
3. `.github/actionlint.yaml`, then `.github/actionlint.yml`, in the project root.

A file that does not exist is skipped. A file that exists but cannot be read, such as a directory named `workflint.yaml`, stops the run. A file in no project, linted without `-config-file`, gets the defaults.

workflint reads the config file of a project once per run, before it lints any file. A config file that does not decode stops the run with exit code 3 and prints `could not parse config file "<path>": <error>` to stderr. For example:

```text
could not parse config file "/repo/.github/workflint.yaml": yaml: unmarshal errors:   line 1: cannot unmarshal !!int `42` into struct { Labels []string "yaml:\"labels\"" }
```

An empty file, or one with only comments, gives the defaults.

## Unknown keys

workflint knows the keys on this page. What it does with any other key depends on the file's name:

- In a file named `workflint.yaml` or `workflint.yml`, an unknown key stops the run with exit code 3. The error names the key and where it is: `could not parse config file "<path>": unknown key "lables" in "self-hosted-runner" at line:2,col:3`.
- In any other file, such as `.github/actionlint.yaml` or a file given by `-config-file`, an unknown key prints a warning to stderr and the run goes on, because actionlint accepts such files: `warning: config file "<path>": unknown key "lables" in "self-hosted-runner" at line:2,col:3 is ignored`. `-quiet` and `-silent` hide the warning.

## Keys

```yaml
self-hosted-runner:
  labels: [gpu, linux-*]
config-variables: [DEPLOY_TARGET, ENVIRONMENT]
paths:
  .github/workflows/release.yaml:
    ignore:
      - 'label "gpu" is unknown'
python:
  linter: ruff
```

### `self-hosted-runner.labels`

The labels of your self-hosted runners, so that `runs-on` accepts them. Each entry is a pattern in the syntax of Go's `path.Match`: `*` matches any text without `/`, `?` matches one character, `[...]` matches a character of a class, and `\` escapes the next character. A pattern must match the whole label, and case counts. A malformed pattern is reported by [`invalid-runner-label-pattern`](rules/invalid-runner-label-pattern.md). `null` and `[]` are the same: no extra labels.

### `config-variables`

The names of the configuration variables of your repository and organization. With a list, [`undefined-config-variable`](rules/undefined-config-variable.md) reports `vars.NAME` when `NAME` is not in it, compared without regard to case. An empty list, `[]`, allows no variable. `null`, or no key, turns the check off, so `vars` accepts any name.

### `paths`

Settings for the files whose path matches a glob. Each key is a glob, and every entry whose glob matches the file applies.

A glob matches the path of the linted file from the project root, with `/` between directories on every system, such as `.github/workflows/ci.yaml`. It matches the same path whatever directory workflint runs in. A file in no project, linted with `-config-file`, is matched by its path from the working directory. In a glob, `*` matches any text without `/`, `**` as a whole path segment matches any number of directories, `?` matches one character, `[...]` matches a character of a class, `{a,b}` matches either alternative, and `\` escapes the next character. An invalid glob, such as `foo.{txt,xml`, stops the run when the file is read: `invalid glob pattern "foo.{txt,xml" in "paths"`.

`paths.<glob>.ignore` is a list of regular expressions in Go's syntax, as for `-ignore` ([cli.md](cli.md#flags)). An error of a matching file is dropped when one of them matches its message. The pattern is unanchored and sees only the message. Each entry must be a string. A number is read as its text, and a list, a mapping, or an empty entry is an error, because actionlint compiles those as the empty pattern, which drops every error. An invalid pattern stops the run and names its position:

```text
could not parse config file "/repo/.github/actionlint.yaml": yaml: unmarshal errors:   line 4: invalid regular expression "(foo" in "ignore" at line:4,col:9: error parsing regexp: missing closing ): `(foo`
```

A glob that matches no linted file does nothing, and workflint does not warn about it.

### `exclude`

Globs that name files workflint should not lint, such as generated workflows:

```yaml
exclude:
  - .github/workflows/*.lock.yml
  - vendor/**
```

A glob matches the path of a file from the project root, as the [`paths`](#paths) globs do, in the same syntax. A file that one of them matches is skipped when workflint looks for files: without file arguments, under a directory argument, and among the matches of a glob argument ([cli.md](cli.md#directories-and-globs)). A file given by name is linted anyway. With `-config-file`, its `exclude` applies to the files of every project, and a file in no project is matched by its path from the working directory. `vendor` alone matches the path `vendor` and no file under it, so use `vendor/**` for a directory. An invalid glob stops the run when the file is read: `invalid glob pattern "foo.{txt,xml" in "exclude"`. `null` and `[]` exclude nothing.

Files that `.gitignore` names are skipped too, without an `exclude` entry.

### `python.linter`

What checks the scripts of `shell: python` steps ([pyflakes-issue](rules/pyflakes-issue.md)). The value is one of these, and case counts:

- `auto`, the default: ruff if `ruff` is on `PATH`, else pyflakes if `pyflakes` is on `PATH`. With neither, the first Python script of the run prints `warning: neither ruff nor pyflakes was found on PATH, so the scripts of shell: python steps are not checked. ...` to stderr, and the run goes on. A run without Python scripts prints nothing.
- `ruff`: ruff restricted to the pyflakes rules, without any ruff config file. Its messages have ruff's rule code and wording, such as `1:7: F821 Undefined name `hello``.
- `pyflakes`: pyflakes, as actionlint runs it.
- `off`: nothing checks the scripts.

A `ruff` or `pyflakes` that is not on `PATH` stops the run with exit code 3: `python.linter in config file "<path>" is "ruff", but ruff was not found on PATH`. The `-pyflakes` flag wins over the key: `-pyflakes=PATH` runs that pyflakes, and `-pyflakes=` checks nothing ([cli.md](cli.md#flags)). Any other value stops the run when the file is read:

```text
could not parse config file "/repo/.github/workflint.yaml": yaml: unmarshal errors:   line 2: "python.linter" must be "auto", "ruff", "pyflakes", or "off", but "black" was found at line:2,col:11
```

workflint reads `python` in actionlint config files too, without a warning.

### `shellcheck`

How shellcheck checks `run:` scripts ([shellcheck-issue](rules/shellcheck-issue.md)):

```yaml
shellcheck:
  rcfile: .shellcheckrc
  args: [--severity=warning]
  cache: true
```

A config file with a `shellcheck` key asks for shellcheck. If shellcheck is not installed, the run stops with exit code 3: `shellcheck that config file "<path>" configures was not found: <error>`. `-shellcheck=` still turns shellcheck off without a message.

### `shellcheck.rcfile`

A shellcheck rc file, as a path from the project root, such as `.shellcheckrc`. workflint passes it with `--rcfile` in place of `--norc`, so its directives, such as `disable=SC2086`, apply to every script. Without the key, workflint passes `--norc` as actionlint does, and no `.shellcheckrc` on the machine changes the results. With `-config-file`, the path starts at the project root of each linted file, or at the working directory for a file in no project. A file that cannot be read stops the run with exit code 3: `could not read shellcheck.rcfile "<path>" of config file "<path>": <error>`.

### `shellcheck.args`

More arguments for shellcheck, such as `[--severity=warning]` or `[-o, all]`. workflint passes them after its own arguments and before the script, as written. An argument that changes the output format, such as `-f gcc`, makes every script fail with [shellcheck-failed](rules/shellcheck-failed.md), because workflint reads shellcheck's JSON output. The `SHELLCHECK_OPTS` environment variable keeps working too.

### `shellcheck.cache`

`true` keeps the findings of each script in a cache on disk, so that a later run starts no shellcheck for a script it has checked before. The default is `false`. `-shellcheck-cache` turns the cache on for every project.

The cache is the directory `$XDG_CACHE_HOME/workflint/shellcheck`. When `XDG_CACHE_HOME` is not set, it is `workflint/shellcheck` in the platform's cache directory: `~/.cache` on Linux, `~/Library/Caches` on macOS, and `%LOCALAPPDATA%` on Windows. An entry belongs to a hash of shellcheck's version, its arguments, the text of the rcfile, `SHELLCHECK_OPTS`, and the script, so a new shellcheck or a changed rcfile misses the cache. A run with the cache on starts `shellcheck --version` once to learn the version. workflint stores only findings, so a script that shellcheck could not check runs again next time. An entry that cannot be read or decoded counts as missing, and the run writes it again. workflint never deletes entries. Delete the directory to empty the cache.

### `tool-timeout`

How many seconds an external tool process, shellcheck, ruff, or pyflakes, may run. When the time passes, workflint kills the process and reports [shellcheck-failed](rules/shellcheck-failed.md) or [pyflakes-failed](rules/pyflakes-failed.md) on its script. The value is a whole number greater than 0, and the default is 60. workflint checks up to 64 scripts in one shellcheck process. When such a process times out, workflint checks each of its scripts alone, so each script gets the whole timeout. A value that is not a whole number greater than 0 stops the run with exit code 3:

```text
could not parse config file "/repo/.github/workflint.yaml": yaml: unmarshal errors:   line 1: "tool-timeout" must be a whole number of seconds greater than 0, but "0" was found at line:1,col:15
```

## Creating a config file

`workflint -init-config` writes `.github/workflint.yaml` in the project of the working directory, prints `Config file was generated at "<path>"`, and exits with code 0. The file has actionlint's template, with every key at its default and comments that explain them. workflint refuses to overwrite a config file: if the project has any of the four files above, it prints `config file already exists at "<path>"` and exits with code 3. Outside a project, it exits with code 3 too. The arguments after the flags, and `-config-file`, are ignored.

## Differences from actionlint

- [`paths` globs match from the project root](differences.md#paths-relative-to-root).
- [Config file names and lookup order](differences.md#config-file-lookup).
- [Unknown config keys are reported](differences.md#config-unknown-keys).
- [Non-string `ignore` entries are rejected](differences.md#config-ignore-entries).
- [Config error messages](differences.md#config-error-messages).
- [ruff is used for Python scripts when installed](differences.md#python-linter-auto).
- [shellcheck settings and its cache](differences.md#shellcheck-settings).
- [`exclude` skips files](differences.md#config-exclude).

## Coming later

- A user config file in `$XDG_CONFIG_HOME/workflint/config.yaml`, and `--no-config`, in M9.4.
- `rules`, to turn rules off and set their severity, in M9.2, and a published JSON Schema with `workflint config schema` in M9.3.
