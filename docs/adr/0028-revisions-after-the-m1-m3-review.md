# ADR 0028: Revisions after the M1–M3 review

- Status: accepted
- Date: 2026-10-17
- Amends: ADR 0006, ADR 0011, ADR 0016, ADR 0018, ADR 0019, ADR 0022, ADR 0024

## Context

The review after M3 compared the accepted ADRs with the code on `main` at commit `2e10467`. Seven ADRs no longer describe the code or the process. ADR 0001 forbids editing an accepted ADR for substance, so this ADR records the changes and the amended ADRs link to it.

- ADR 0022, which pull request #56 rewrote after pull request #59 had accepted it, contradicts itself. Its Context says the vendored scanner "records comments where libyaml already skips them". Its Decision says the scanner reads comments with the code of go-yaml's `scanComments` and `scanLineComment`, which is what the scanner does. Its list of the places where workflint does not follow go-yaml also leaves out `docs/differences.md#tag-before-flow-comma`, which `crates/workflint-yaml/src/libyaml/scanner.rs` implements.
- ADR 0024 lets a crate that reports diagnostics while parsing define their `RuleMeta`. `workflint-expr` also defines the `RuleMeta` of the type checker's and the untrusted input check's diagnostics in `crates/workflint-expr/src/rules.rs`. `TypeEnv::check` reports them when a rule calls it, after parsing. M3.5 (#86) read ADR 0024 as covering them.
- ADR 0019 says the nightly job flags a regression when "wall time regresses by more than 10%". `cargo xtask bench-history` (`xtask/src/bench/history.rs`) compares workflint's wall time divided by the reference's from the same run, as `CONTRIBUTING.md` documents. It compares plain wall time only when one of the two runs has no time for the reference.
- ADR 0018 says library crates define their errors with `thiserror`. `DecodeError` in `workflint-yaml`, the vendored parser's `Error`, `ExprError` in `workflint-expr`, and `MetadataError` in `workflint-syntax` implement `std::error::Error` by hand. The vendored `Error` came that way from libyaml-safer. The other three are structs whose `Display` writes a message, so a derive would save a few lines in each and add a build dependency to three crates.
- ADR 0016's definition of done, and the checklist of `.github/pull_request_template.md`, ask for an allow-list record for every difference. ADR 0023 limits records to differences that change the output of a fixture or of the corpus.
- ADR 0006 and ADR 0011 name `cargo xtask codegen` as if it existed. It arrives with the data generators in M7. Until then, the data in `workflint-data` is written by hand from the reference's tables.

## Decision

ADR 0022, comments:

- The vendored scanner reads comments where go-yaml v4.0.0-rc.3 reads them, with a port of `scanComments` and `scanLineComment` from go-yaml's `internal/libyaml/scanner.go`. It records each comment, including the comment after a directive, which go-yaml drops, and the parser reports each one as a comment event. The tree attaches comments by its own rules (ADR 0009).
- The sentence in ADR 0022's Context about comments describes an earlier scanner and no longer applies.
- `NOTICE` credits the ported code in go-yaml's entry.

ADR 0022, parity with go-yaml:

- workflint does what go-yaml does, except where go-yaml rejects valid YAML or reads it wrongly. Then workflint follows the YAML specification, and `docs/differences.md` lists the case. The cases are the `\/` escape (`yaml-solidus-escape`), the `%YAML 1.2` directive (`yaml-1-2-directive`), and a tag followed by `,`, `[`, or `]` in a flow collection, which ends the tag there, as in libyaml (`tag-before-flow-comma`).

ADR 0024, where rule metadata lives:

- The crate that reports a diagnostic defines the `RuleMeta` of its rule, whether the rule runs while parsing or while checking. Today these crates are `workflint-yaml`, `workflint-syntax`, and `workflint-expr`.
- The rules that `workflint-expr` defines are checker rules: `TypeEnv::check` reports them when a rule in `workflint-rules`, such as `expression`, calls it. Like parse-stage rules, they have no module in `workflint-rules`, the registry in `workflint-rules` lists them, and each gets a page in `docs/rules/<id>.md`. M4.2 (#139) writes the pages of the parse-stage rules and M4.11 (#148) those of the `workflint-expr` rules.

ADR 0019, the nightly regression check:

- For each scenario, the check divides workflint's mean wall time by the reference's from the same run. It flags a regression when that ratio grows by more than 10% against the latest scheduled run in the history. When either run has no time for the reference, it compares workflint's mean wall time instead. A run started by hand goes into the history, but no later run is compared with it.

ADR 0018, error types:

- A library crate may implement `std::error::Error` and `Display` by hand instead of deriving them with `thiserror`. Either way, errors stay typed, and the binary may still use `anyhow` for fatal errors.

ADR 0016, definition of done:

- Every new difference from the reference has an entry in `docs/differences.md`. A difference that changes the output of a fixture or of the corpus also has allow-list records, as ADR 0023 says. The pull request template says the same.

ADR 0006 and ADR 0011, `cargo xtask codegen`:

- `cargo xtask codegen` is planned for M7, with the data generators. Until then, the tables in `workflint-data` are ported by hand from the reference, and their tests compare them with the reference's tables or fixtures. The other `xtask` commands that ADR 0006 names exist.

## Consequences

The ADRs agree with the code again, and each amended ADR names this one in its header. Agents that read ADR 0024 put the rule metadata of a checker rule in the crate that reports it, and M4.1 (#138) lists those rules in the registry.

A hand-written error implementation is a few more lines to review than a derive, and it keeps `thiserror` out of crates that would use it for a few lines.

## Alternatives considered

- Edit ADR 0022 and ADR 0024 in place. Rejected, because ADR 0001 forbids substantive edits to accepted ADRs.
- One amending ADR for each amended ADR. Rejected, because the changes come from one review and are small, and seven ADRs would be harder to find than one.
- Move the `RuleMeta` of the `workflint-expr` rules into `workflint-rules` and have the checker return its own error type. Rejected for the reason ADR 0024 gives: each check would need a second error model and a conversion that keeps spans and messages identical.
- Add `thiserror` to the crates whose errors are written by hand. Rejected, because each hand-written implementation is a few lines long.
