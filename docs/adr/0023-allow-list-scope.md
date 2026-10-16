# ADR 0023: Allow-list scope

- Status: accepted
- Date: 2026-10-16
- Amends: ADR 0003

## Context

ADR 0003 says: "Every intentional difference is listed in `docs/differences.md` and in the conformance suite's allow-list." The allow-list built in M1.9, `tests/conformance/allowed-differences.toml`, records changed output lines. Each entry names a fixture, and the suite checks that the fixture exists and that each `upstream` line is in its `.out` file.

Many entries in `docs/differences.md` change no fixture's output: a new flag, a message on input that no fixture has, or a behaviour that is still planned. An allow-list record for such a difference has no line to replace and no fixture to name, so the suite cannot check it.

## Decision

- Every intentional difference from the reference has an entry in `docs/differences.md`. A difference that is not listed there is a bug in workflint.
- The allow-list records a difference only where it changes the output of a fixture or of the corpus. A difference that changes neither has no allow-list record.
- Every allow-list record names its fixture, and `fixture` stays a required key. Records for corpus files name the file as `realworld/<file name>` once M5.11 adds them (#49).

## Consequences

Every allow-list record is checked against real output, and a record whose difference has gone fails the suite. Reviewers check the remaining differences against `docs/differences.md` alone.

An entry in `docs/differences.md` can exist without an allow-list record. When a later fixture starts to show the difference, the pull request that adds the fixture adds the record.

## Alternatives considered

- Make `fixture` optional, so that every difference gets a record. Rejected, because a record without a fixture checks nothing.
