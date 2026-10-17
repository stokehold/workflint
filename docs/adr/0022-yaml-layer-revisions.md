# ADR 0022: YAML layer revisions

- Status: accepted
- Date: 2026-10-16
- Amends: ADR 0005
- Amended by: ADR 0028

## Context

ADR 0005 set the design of `workflint-yaml` before M1 built it. The M1 review compared the built layer with that design and with go-yaml, the YAML library of the reference, and found four places where ADR 0005 no longer describes the right goal:

- ADR 0005 has each scalar carry a source map, with folded and quoted scalars mapped "with the best available precision". The layer builds maps on request instead, and they are exact in every style.
- ADR 0005 asks for positions that match go-yaml, but gives no rule for inputs on which go-yaml, libyaml and the YAML specification disagree. The review found such inputs (#50): nesting depth, anchor names, the simple-key length limit, control characters, byte order marks, UTF-16 input, and errors just after the first document.
- ADR 0005 says comment events are "ported from go-yaml's comment handling". The vendored scanner records comments where libyaml already skips them, and the tree attaches them by its own rules.
- ADR 0005 lists recovery as a requirement: "one malformed section must not hide errors in the rest of the file". actionlint stops at the first YAML syntax error, and libyaml's parser stops there too.

## Decision

Source maps:

- `Node::source_map` builds a scalar's source map from the scalar's source text and style when a caller asks for it. The tree stores no maps.
- The map is exact in all five scalar styles: plain, single-quoted, double-quoted, literal, and folded. The fuzz targets check this on every scalar (ADR 0018).

Parity with go-yaml:

- `workflint-yaml` accepts and rejects the same input as go-yaml, with the same error messages. Error positions are go-yaml's, except where `docs/differences.md` moves them.
- Where go-yaml rejects valid YAML, workflint accepts it and `docs/differences.md` lists the case. Today these cases are the `\/` escape in a double-quoted scalar and the `%YAML 1.2` directive.
- Everywhere else workflint does what go-yaml does, also where libyaml or the specification differ. That includes:
  - A depth limit of 10,000 flow levels or block indents, with go-yaml's `exceeded max depth of 10000` error.
  - The characters go-yaml allows in anchor and alias names.
  - The 1,024-character simple-key limit, counted in characters, not bytes.
  - Reading tokens ahead of the current event as go-yaml's parser does, so that an error just after the first document is reported.
  - The control character check per 512-byte read chunk.
  - A byte order mark after the start of the stream staying in the content.
- File bytes are decoded as go-yaml decodes them: UTF-8 with or without a byte order mark, and UTF-16LE or UTF-16BE when the file starts with the matching byte order mark. Other input fails with go-yaml's error.
- Duplicate keys are found as actionlint finds them. Keys compare after Unicode simple lowercase mapping, as Go's `strings.ToLower` does, and `<<` keys are skipped.

Comments:

- The scanner reads comments where go-yaml's scanner reads them, with the code of go-yaml's `scanComments` and `scanLineComment`, so the same input passes and block ends get go-yaml's positions. It records each comment, and the parser reports it as a comment event. The comment events are workflint's own, not go-yaml's head, line, and foot comments.
- The tree attaches comments to nodes by its own rules (ADR 0009), not by go-yaml's head, line, and foot comments.

Errors:

- The first YAML syntax error ends parsing of the file, as in actionlint. The file gets that one diagnostic, no tree, and no other rule runs on it. Recovery from YAML syntax errors is out of scope.

## Consequences

`cargo xtask yaml-parity` can demand identical results on every input it compares, apart from the listed cases. A new disagreement with go-yaml is a bug in `workflint-yaml` unless go-yaml rejects valid YAML, and then it needs an entry in `docs/differences.md`.

Rules that never ask for a source map pay nothing for it. Diagnostics inside `run:` scripts and quoted strings point at the exact source character.

A file with a YAML syntax error shows one diagnostic, as in actionlint, and the user fixes it before seeing others.

## Alternatives considered

- Follow the YAML 1.2 specification and the YAML test suite instead of go-yaml. Rejected, because a file that go-yaml rejects would then pass workflint, with a different exit code from actionlint.
- Match go-yaml also where it rejects valid YAML. Rejected, because these inputs are valid YAML, so an error on them is a false positive.
- Store a source map in every scalar. Rejected, because most scalars never need one.
- Recover after a YAML syntax error. Rejected, because the reference has no behaviour to test the result against, and the libyaml parser stops at its first error.
