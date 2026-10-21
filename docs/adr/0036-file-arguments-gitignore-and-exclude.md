# ADR 0036: Directory and glob arguments, `.gitignore`, and `exclude`

- Status: accepted
- Date: 2026-10-21
- Amends: ADR 0010

## Context

actionlint treats every argument as a file. A directory argument stops the run with `could not read "<dir>": read <dir>: is a directory` and exit code 3. A glob only works when the shell expands it, so a quoted glob fails, and so does a glob on Windows shells that do not expand it (rhysd/actionlint#310). actionlint has no way to skip files when it lints the whole repository, so a generated workflow, such as a `*.lock.yml` file of GitHub Agentic Workflows, can only be hidden with `paths.<glob>.ignore: ['.*']`, which still parses it (rhysd/actionlint#667). Discovery does not read `.gitignore` either.

ADR 0010 lists the keys that workflint adds to the config file, and `exclude` is not among them. ADR 0013 asks for the files of a run to be sorted by path.

## Decision

Arguments:

- An argument that names a directory lints the files under it whose names end in `.yml` or `.yaml`, with the same walk as discovery: subdirectories included, symbolic links below it not followed, and the paths sorted as strings.
- An argument that does not exist and contains `*`, `?`, `[`, or `{` is a glob. workflint expands it itself, in the syntax of the `paths` globs, from the working directory, and lints the matched files whose names end in `.yml` or `.yaml`, sorted by path. On Windows, `\` in a glob separates directories.
- A glob that matches no such file stops the run with exit code 3 and `no YAML file matches the glob "<glob>"`. A malformed glob stops it with `invalid glob pattern "<glob>"`. A directory with no such file stops it with `no YAML file was found in "<dir>"`, as discovery does.
- Any other argument is a file and is linted as before. The arguments keep their order, and a file given twice is linted twice.

`.gitignore`:

- Discovery, directory arguments, and glob arguments skip the files that the `.gitignore` files of the repository and `.git/info/exclude` name, read with the `ignore` crate. They apply in Git and Jujutsu repositories only, as Git applies them.
- The user's global excludes file (`core.excludesFile`) is not read, so that CI and every machine find the same files.
- Version control directories (`.git`, `.jj`, `.hg`) are never walked.

`exclude`:

- ADR 0010's list of workflint keys gets `exclude`: a list of globs, in the syntax of `paths`, that match the path of a file from its project root. Discovery, directory arguments, and glob arguments skip the files they match. With `-config-file`, its `exclude` applies to every project, and a file in no project is matched by its path from the working directory, as for `paths`.
- An invalid glob in `exclude` stops the run when the config file is read: `invalid glob pattern "<glob>" in "exclude"`.

Files given by name:

- A file given by name is linted even when `.gitignore` or `exclude` names it. The user asked for that file.

## Consequences

`workflint .github` and `workflint '.github/workflows/*.yml'` work on every OS, and a repository with generated workflows can skip them in one place. The directory argument is a difference from actionlint, where it was a fatal error, and so is the effect of `.gitignore` on discovery. Both are listed in `docs/differences.md`.

A tool that passes the changed files by name, such as a pre-commit hook, lints a generated file that `exclude` names. It has to filter those files itself.

workflint gets the `ignore` crate and its dependencies, `globset` among them. Discovery reads the config file of a project before the lint does, so a broken config file stops discovery with the same message as before.
