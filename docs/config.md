# Config file

workflint reads actionlint's config file unchanged, and a config file of its own at `.github/workflint.yaml` (ADR 0010). This page lists where workflint looks for the file and every key that works today.

## Where workflint looks

Each file is linted with the config of its project. The project root is the nearest directory above the file that has a `.github/workflows` directory and a `.git` file or directory. workflint uses the first file it finds, in this order, and does not merge them:

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

## Creating a config file

`workflint -init-config` writes `.github/workflint.yaml` in the project of the working directory, prints `Config file was generated at "<path>"`, and exits with code 0. The file has actionlint's template, with every key at its default and comments that explain them. workflint refuses to overwrite a config file: if the project has any of the four files above, it prints `config file already exists at "<path>"` and exits with code 3. Outside a project, it exits with code 3 too. The arguments after the flags, and `-config-file`, are ignored.

## Differences from actionlint

- [`paths` globs match from the project root](differences.md#paths-relative-to-root).
- [Config file names and lookup order](differences.md#config-file-lookup).
- [Unknown config keys are reported](differences.md#config-unknown-keys).
- [Non-string `ignore` entries are rejected](differences.md#config-ignore-entries).
- [Config error messages](differences.md#config-error-messages).

## Coming later

- A user config file in `$XDG_CONFIG_HOME/workflint/config.yaml`, and `--no-config`, in M9.4.
- `rules`, to turn rules off and set their severity, in M9.2, and a published JSON Schema with `workflint config schema` in M9.3.
- `python` settings with pyflakes in M5.12, `shellcheck` settings in M5.13, and `exclude` in M5.18.
