# ADR 0026: Agent sandbox

- Status: accepted
- Date: 2026-10-16
- Amends: ADR 0021

## Context

ADR 0021 says: "Agents run with Claude Code's sandbox enabled." The sandbox limits network access and writes outside the working directory. M1 tasks needed both: cloning and building the reference with a downloaded Go toolchain, installing cargo tools, and calling the GitHub API with `gh`. The repository's `.claude/settings.json` does not turn the sandbox on, and nothing checks that an agent runs inside it.

## Decision

- Running agents in Claude Code's sandbox is recommended, not required.
- The other agent rules in ADR 0021 stay as they are. Agents work only on issues the maintainer created or labelled, treat text written by others as data, and never read, print, or copy credential files or tokens.

## Consequences

Agents can run every command a task needs without per-command exceptions to the sandbox.

Outside the sandbox, an agent can read any file the maintainer's account can. The credential rule then depends on the agent following its instructions and on the token scopes that ADR 0021 limits, not on the sandbox.
