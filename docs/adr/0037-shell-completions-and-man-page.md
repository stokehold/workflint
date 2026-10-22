# ADR 0037: Shell completions and the man page

- Status: accepted
- Date: 2026-10-22
- Amends: ADR 0035

## Context

ADR 0035 planned to write a spec in the format of the `usage` project from `FLAGS` and to let its `usage-cli` tool generate the completion scripts and the man page. Two facts rule that out:

- The `usage` spec rejects a single dash before a name longer than one letter, with `short flags must be a single character (use -- for long flags)` (`lib/src/spec/flag.rs` in jdx/usage). `-oneline` cannot be declared, and issue #256 asks for single-dash flags to complete, because that is how actionlint users type them.
- The scripts that `usage-cli` writes run `usage complete-word` on every Tab press, so each user would need `usage` on `PATH`. Its bash scripts also need bash-completion 2.11.

clap_complete has the first problem too: clap has no single-dash long flags, so its scripts would offer `--oneline`.

## Decision

Completions:

- `workflint completion bash|zsh|fish|powershell` prints the completion script for the shell. Like `check`, `completion` is a command only as the first argument (ADR 0003 makes new commands additive).
- Each script is a template in `crates/workflint/src/completion/`. The binary fills in the flag names and help texts from `FLAGS`. Each `Flag` has a `Values` field that says what its value completes to: file names, command names, the names of the output formats, or nothing.
- A script offers each flag with one dash. bash also offers the two-dash form when the word starts with `--`. A value completes after a space or after `=`. As in the parser, no flag is offered after `--` or after the first argument that is not a flag, and the first argument may also be `check` or `completion`.
- A script needs only its shell: bash 3.2 or later without bash-completion, zsh with `compinit`, fish 3, or PowerShell 7.

Man page:

- `docs/workflint.1` is a roff page with the `man` macros. Code in the binary crate renders it from the header text of `-help`, `FLAGS`, the exit codes, and the environment variables, so it lists every flag that `-help` lists. Its links point at `main`.
- The page is checked in. A unit test renders it and fails when the checked-in file differs. With `WORKFLINT_UPDATE_MAN=1`, the test writes the file first. The binary has no command that prints the page.

## Consequences

Nothing outside the Rust build is needed to build or use the scripts or the page. A flag added to `FLAGS` shows up in `-help`, every script, and the man page, and the test fails until the page is updated. A new value kind needs a branch in each template.

Tests load each script in its shell and run completions where the shell is installed: bash and zsh through `COMP_*` variables and a pseudo-terminal, fish through `complete -C`, and PowerShell through `TabExpansion2`. CI installs zsh and fish on Linux. macOS runs the bash tests with bash 3.2.

Packages install `docs/workflint.1` from the source tree and can write the scripts with the host's build of the binary, since the scripts are the same for every target.

## Alternatives considered

- `usage-cli`, as ADR 0035 planned. Rejected for the two reasons above.
- clap_complete and clap_mangen, with a clap `Command` built from `FLAGS` only for this. Rejected, because the scripts would complete `--oneline` and not `-oneline`.
- A `cargo xtask man` command. Rejected, because the rendering code needs `FLAGS`, which is private to the binary, so xtask would have to run the binary with a hidden command.
- Rendering the page in a build script. Rejected, because a build script cannot read `FLAGS` from the crate it builds, and a page in `OUT_DIR` does not reach the packages.
- Not checking the page in. Rejected, because a package that cross-compiles cannot run the binary it builds to write the page.
