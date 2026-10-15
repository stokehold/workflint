# ADR 0005: YAML layer

- Status: accepted
- Date: 2026-10-15

## Context

Every diagnostic points at a position in a YAML file, so the YAML layer decides how accurate workflint can be. The requirements come from parity and from planned features:

- Line and column for every key and value, matching go-yaml (which actionlint uses) so that fixture positions match.
- Byte offsets and end positions, for SARIF regions, editor ranges, and autofix edits.
- Anchors, aliases, and tags, including the anchor's own position.
- Scalar style (plain, single-quoted, double-quoted, literal `|`, folded `>`) and chomping, so that quoted `'true'` stays a string and so that positions inside `run:` scripts can be mapped back to the source line.
- Comments, for inline ignore comments (ADR 0009).
- Recovery: one malformed section must not hide errors in the rest of the file.

`docs/research/01-parsing-ast.md` and `06-rust-ecosystem.md` compared the Rust YAML crates. `libyaml-safer` 0.3.0 is a safe Rust port of libyaml, and go-yaml is also derived from libyaml. On 1,733 test files, it matched go-yaml's positions, values, and anchors on all 1,719 files that parse, and it was about four times faster. It has one known defect: it panics on a `|` block scalar at end of file without a trailing newline (9 real-world files). It does not report comments.

`saphyr-parser` was the runner-up. It loses anchor names, reports content positions instead of anchor positions, and its "byte index" counts characters. Crates that deserialize YAML into typed Rust structs through serde lose positions and reject a whole file on the first malformed key, which rules them out.

## Decision

- `crates/workflint-yaml` contains a vendored fork of `libyaml-safer` 0.3.0 (MIT), with its license and origin commit recorded. The fork fixes the end-of-file panic and adds comment events, ported from go-yaml's comment handling.
- On top of the event stream, `workflint-yaml` builds its own tree: one arena per file, nodes with byte spans, tag, scalar style, anchor name, and attached comments. Line and column are computed on demand from a line index.
- Mappings keep source order and report duplicate keys instead of silently dropping them.
- Each scalar carries a source map from decoded content back to source bytes. Literal block scalars map exactly. Folded and quoted scalars map with the best available precision, falling back to the scalar's start.
- Scalars are typed by tag and style under the YAML 1.2 core schema. `on`, `yes`, and `no` are strings. A quoted or block scalar is a string.
- Only the first document in a file is linted, as in actionlint.
- The crate exposes no serde deserialization of workflows. The parser in `workflint-syntax` walks the tree by hand (ADR 0006).

## Consequences

Positions match the reference out of the box, which removes a whole class of conformance failures. Diagnostics inside `run:` scripts, ignore comments, and later autofix all use the same spans and source maps.

We own a YAML parser fork and must keep it in sync with upstream security fixes ourselves. The fork is small and stable, and fuzzing covers it (ADR 0018).
