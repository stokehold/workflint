# workflint-yaml

The YAML layer of [workflint](https://github.com/stokehold/workflint). It parses the first YAML document of a file into a span tree: an arena of nodes with byte spans, tags, styles, anchors, resolved aliases, and attached comments (ADR 0005). Each scalar can build a source map that turns a byte offset in its decoded value into a byte offset in the file. The tree is built from the event parser of a vendored fork of [libyaml-safer](https://github.com/simonask/libyaml-safer) 0.3.0, which the crate also exposes.

This is an internal crate of workflint. Its API has no stability promise and may change in any release.

`src/libyaml/ORIGIN.md` records where the vendored code comes from and what changed. `tests/yaml-test-suite/` holds cases from the [YAML test suite](https://github.com/yaml/yaml-test-suite), under its MIT license in `tests/yaml-test-suite/LICENSE`. The fuzz targets in the repository's `fuzz/` directory run the checks in `tests/invariants/`. `tests/fuzz_regressions.rs` runs the same checks on each input in `tests/fuzz-regressions/`.
