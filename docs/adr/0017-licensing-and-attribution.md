# ADR 0017: Licensing and attribution

- Status: accepted
- Date: 2026-10-15

## Context

workflint ports actionlint's design, messages, and tests, so it is a derivative work of actionlint, which is MIT-licensed (Copyright (c) 2021 rhysd). It also vendors a fork of `libyaml-safer` (MIT) and actionlint's test data (MIT). The external tools it runs include shellcheck, which is GPL-3.0.

## Decision

- workflint is licensed under the MIT license.
- `NOTICE` lists every third-party work whose code or data workflint includes, with its license and copyright: actionlint (design, messages, test data), `libyaml-safer` and libyaml (vendored YAML parser), and the `unslop` skill. Each vendored directory also keeps the original license file.
- Every dependency must use a license on the `deny.toml` allow list: MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Unicode-3.0, or Zlib. `cargo deny` enforces this in CI.
- GPL-licensed tools such as shellcheck are only ever run as separate processes. workflint never links them. The `-full` Docker image ships them as separate executables next to workflint.
- Contributions are accepted under the MIT license, with no contributor license agreement.

## Consequences

Anyone can use, modify, and embed workflint and its crates. The attribution duties from actionlint and the YAML parser are met by `NOTICE` and the kept license files.
