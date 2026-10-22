# ADR 0038: Target for the corpus with shellcheck

- Status: accepted
- Date: 2026-10-22
- Amends: ADR 0019

## Context

ADR 0019 asks that workflint lint the 1,503-file corpus with shellcheck on in at most one fifth of the reference's wall time. workflint misses that target on every machine measured so far:

| Machine | workflint | Reference | Ratio |
|---|---|---|---|
| Nightly runner, 4 logical CPUs, 2026-10-22 | 5.09 s | 22.2 s | 0.229 |
| Intel i5-9500T, 6 logical CPUs | 2.60 s | 8.87 s | 0.294 |

Almost all of workflint's time in this scenario goes to shellcheck. workflint already does the least shellcheck work the output allows. It checks each distinct script once per run, passes the same arguments as the reference, and checks scripts that share a shell and options in one process. On the 6-CPU machine, the shellcheck processes use about 14 CPU seconds, and workflint keeps the CPUs about 90% busy. Batches of 8 to 128 scripts per process all take 13.6 to 14.4 CPU seconds. The shellcheck 0.11.0 release binary rejects GHC runtime options, so its garbage collector cannot be tuned from outside.

The reference spends most of its extra time starting one shellcheck process per script: 27 CPU seconds in user mode and 22 in the kernel. The ratio therefore depends on how fast a machine starts processes, not on anything workflint controls. One fifth of 8.87 s is 1.77 s, and 14 CPU seconds in 1.77 s needs 8 fully busy CPUs.

## Decision

- The target for the corpus with shellcheck on is a wall time of at most one third of the reference's, the same ratio as the corpus with external tools off.
- No run may hang, and every subprocess has a timeout, as in ADR 0019.
- The other targets of ADR 0019 stay as they are.

## Consequences

- `cargo xtask bench` reports the shellcheck scenario against 1/3.
- The target still catches a regression that doubles workflint's time in this scenario: twice 0.229 and twice 0.294 are both above one third.
- Lowering shellcheck's own cost, for example with a faster shellcheck release, would let a later ADR tighten the target again.

## Alternatives

- Keep one fifth. Rejected, because meeting it needs either more CPUs than CI runners have or checking fewer scripts than the reference checks, which changes the output.
- Track the ratio without a target and keep only the no-hang target. Rejected, because the ratio would then catch no regression in how workflint schedules shellcheck.
