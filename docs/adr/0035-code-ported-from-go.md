# ADR 0035: Code ported from Go

- Status: accepted
- Date: 2026-10-20
- Amends: ADR 0017

## Context

ADR 0017 licenses workflint under MIT and lists the third-party works it includes in `NOTICE`. Those works are MIT-licensed, so their code fits under workflint's license.

`workflint-gotemplate` (M5.7) must print the same bytes as Go's `text/template` and give the same errors (ADR 0003). The reliable way to do that is to port Go's `text/template`, `text/template/parse`, `fmt`, and `strconv` code function by function. Go's code is under the BSD 3-Clause license, which requires each copy and derivative to keep its copyright notice, conditions, and disclaimer. The license does not let us relicense the ported code as MIT alone.

## Decision

- A crate that ports Go source code declares `license = "MIT AND BSD-3-Clause"` in its `Cargo.toml`, ships Go's license as `LICENSE-GO`, and includes it in the published package.
- `NOTICE` has an entry for Go that names the ported packages and the crate.
- `deny.toml` and `fuzz/deny.toml` allow BSD-3-Clause, which ADR 0017 already permits for dependencies.
- The rest of workflint stays MIT-only. Code written to match Go's behaviour without copying its structure, such as `workflint_diagnostics::quote`, is not a port.

## Consequences

Users of `workflint-gotemplate` and of the binary that includes it must keep Go's notice, which `NOTICE` and `LICENSE-GO` already carry. BSD-3-Clause is permissive, so anyone can still use, modify, and embed workflint.

## Alternatives considered

- Write the engine from Go's documentation without porting its code, and keep it MIT-only. Rejected, because matching Go's output and error messages byte for byte, including `fmt`'s padding and float formatting, needs Go's exact algorithms. A clean-room version would still follow them line by line.
- Use the `gtmpl` crate. Rejected in `docs/research/06-rust-ecosystem.md` section 5: it is unmaintained and lacks `break`, `continue`, and a complete `printf`.
