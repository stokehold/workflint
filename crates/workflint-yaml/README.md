# workflint-yaml

The YAML layer of [workflint](https://github.com/stokehold/workflint). It contains a vendored fork of [libyaml-safer](https://github.com/simonask/libyaml-safer) 0.3.0 and exposes its event parser (ADR 0005).

This is an internal crate of workflint. Its API has no stability promise and may change in any release.

`src/libyaml/ORIGIN.md` records where the vendored code comes from and what changed. `tests/yaml-test-suite/` holds cases from the [YAML test suite](https://github.com/yaml/yaml-test-suite), under its MIT license in `tests/yaml-test-suite/LICENSE`.
