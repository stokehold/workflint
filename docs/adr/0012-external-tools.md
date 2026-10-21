# ADR 0012: External tools

- Status: accepted
- Date: 2026-10-15

## Context

actionlint checks `run:` scripts by running shellcheck, and Python scripts by running pyflakes. These subprocesses dominate run time: about 18 seconds for 50 files with shellcheck, compared with 0.3 seconds for 1,503 files without it. On macOS, a run with shellcheck over the 1,503-file corpus hung for more than 10 minutes on the reference commit, even after its deadlock fix (`docs/research/00-baseline.md`). Upstream issues report hangs lasting hours in CI (rhysd/actionlint#702, #704, #712).

Other problems in the reference:

- If shellcheck is missing, the check is skipped silently, so results differ between machines.
- A tool failure in one file aborts the whole run.
- Every shellcheck finding points at the `run:` key, not at the line inside the script.

## Decision

Process handling:

- `workflint-linter` runs external tools through one subprocess module. It spawns the process first, then writes stdin from a dedicated thread while reading stdout and stderr concurrently. Child processes inherit no other file descriptors.
- Every process has a timeout, 60 seconds by default and set by the config key `tool-timeout`. On timeout, workflint kills the process, reports a diagnostic for that script, and continues.
- At most `available_parallelism()` tool processes run at once, configurable with `--jobs`.
- On SIGINT or SIGTERM, workflint kills its child processes and exits.
- A failure of a tool on one script produces a diagnostic for that script. It does not abort the run.
- A test feeds a 200 KiB script through the module on every OS in CI.

shellcheck:

- workflint invokes shellcheck with the same arguments and script preprocessing as the reference, so findings match.
- workflint maps each finding's line and column through the `run:` scalar's source map to the exact position in the workflow file.
- Identical scripts with the same shell and arguments run once per workflint run.
- An opt-in on-disk cache in `$XDG_CACHE_HOME/workflint/shellcheck`, or the platform's cache directory, stores the findings of each script, keyed by a hash of the shellcheck version, arguments, rcfile text, `SHELLCHECK_OPTS`, and script. The config key `shellcheck.cache` or the flag `-shellcheck-cache` turns it on. A failure is not stored, and an entry that cannot be read is a miss.
- `--norc` stays the default for reproducible results. Config can set `shellcheck.rcfile`, a path from the project root, and extra `shellcheck.args`. A config file with a `shellcheck` key asks for shellcheck.
- One shellcheck process checks up to 64 scripts that share a shell and settings, each in a file of a temporary directory, with `-f json`, whose findings name their file. The driver queues the scripts of every file before it lints, so that batches span files. A batch that fails runs each of its scripts alone on stdin. M5.13 measured it on the corpus, in a Linux container with 4 CPUs: 61.3 s without batches and 8.7 s with them, against 85.3 s for the reference, with byte-identical output. `json1` was rejected, because it counts a tab as one column where `json` and the reference move to the next tab stop.

Python:

- Config `python.linter` takes `auto`, `ruff`, `pyflakes`, or `off`. `auto` uses ruff if it is installed, restricted to the pyflakes rule set, and otherwise pyflakes.
- The `-pyflakes` flag keeps working as in actionlint.

Missing tools:

- When a tool is enabled by default but not installed, workflint prints one warning to stderr per run and continues.
- When the user explicitly requests a tool by flag or config and it is not installed, workflint exits with code 3.
- `-shellcheck=` and `-pyflakes=` disable the tool without a warning, as in actionlint.

Later:

- A built-in bash syntax check, so that syntax errors are caught without shellcheck, is planned after 1.0.
- A bundled shellcheck build is a possible later addition. shellcheck is GPL-licensed, so it can only ship as a separate executable, never linked into workflint (ADR 0017).

## Consequences

Runs with shellcheck cannot hang, and they are faster through deduplication and caching. Diagnostics inside scripts point at the right line.

Using ruff by default when installed changes some Python messages compared with actionlint. `docs/differences.md` records it.
