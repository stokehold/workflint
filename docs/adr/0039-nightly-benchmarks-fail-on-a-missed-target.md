# ADR 0039: The nightly benchmark job fails on a missed target

- Status: accepted
- Date: 2026-10-22
- Amends: ADR 0019

## Context

ADR 0019 says the targets apply from release 0.1.0, and that before 0.1.0 they are tracked but do not block merges. The nightly job runs `cargo xtask bench`, which exited with an error only when a measurement failed. `cargo xtask bench-history` listed missed targets in the `performance` issue only for workflint 0.1.0 and later. workflint is at 0.0.1, so the nightly job stayed green while it measured the corpus with external tools off at 0.37 to 0.96 of the reference's wall time, against a target of 1/3.

The release (M5.10, #258) needs three consecutive scheduled nightly runs that meet every target. A job that stays green when a target is missed cannot show that.

## Decision

- `cargo xtask bench` writes the results, then exits with an error when a target is missed, as it does when a measurement fails. The error names each missed target and the value measured.
- The nightly job records the results in the history and in the `performance` issue first, then fails.
- This applies to every version, before 0.1.0 too. `cargo xtask bench-history` lists missed targets in the issue for every version.
- A failed nightly job blocks no merge. Pull requests do not run `cargo xtask bench`, so before 0.1.0 the targets still do not block merges, as ADR 0019 says.

## Consequences

- A missed target makes the next morning's nightly run red and adds a comment to the `performance` issue, before the release workflow refuses to publish.
- Three consecutive green scheduled runs show that the targets hold on the nightly runner.
- `cargo xtask bench` on a developer machine exits with an error when that machine misses a target. It writes the results first.
- The nightly runner has had different CPUs on different nights, an AMD EPYC 7763 and an AMD EPYC 9V45 so far, so a target with little headroom can fail on one night and pass on the next.

## Alternatives

- Fail the job only from 0.1.0. Rejected, because the job would stay green until the release, so a green run would say nothing about the targets, and the release workflow would be the first to find a miss.
- Make `cargo xtask bench-history` fail instead. Rejected, because the job appends to the history and updates the issue only after that command succeeds.
