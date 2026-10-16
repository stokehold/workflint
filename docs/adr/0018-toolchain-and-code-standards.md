# ADR 0018: Rust toolchain and code standards

- Status: accepted
- Date: 2026-10-15

## Context

The reference crashes on some inputs: a `.nan` float, an odd-length mapping in action metadata, and others (`docs/research/05-issues.md`). A linter reads untrusted files written by many people, so it must never crash on input. Agents also need fixed conventions so that code from different sessions looks the same.

## Decision

Toolchain:

- Rust edition 2024.
- `rust-toolchain.toml` pins an exact stable toolchain. A maintainer bumps it deliberately in its own pull request.
- The minimum supported Rust version (`rust-version` in `Cargo.toml`) stays at most two minor versions behind the pinned toolchain. Raising it is noted in the changelog.

Safety and panics:

- `unsafe_code = "forbid"` applies to the whole workspace, including the vendored YAML parser, which is safe Rust.
- No input may cause a panic. Clippy's `unwrap_used`, `expect_used`, and `panic` lints are on, and CI denies warnings. Tests and `xtask` may allow them.
- Release builds keep `panic = "unwind"`, so the driver can catch a panic from a bug, report it for the affected file, and continue (ADR 0007).
- `cargo-fuzz` targets cover the YAML parser, the expression parser, the Go template parser, and the full lint of one file. CI runs each target for a few minutes every night.

Style:

- `rustfmt` with default settings.
- Clippy's `pedantic` group warns. A crate that allows a pedantic lint says why in a comment next to the `allow`.
- Library crates return typed errors defined with `thiserror`. The binary may use `anyhow` for fatal errors.
- Public items have rustdoc comments.

Testing:

- Unit tests sit next to the code in `#[cfg(test)]` modules. Integration tests live in each crate's `tests/` directory.
- Snapshot tests use `insta`. File-driven tests use `datatest-stable`.
- CI runs tests with `cargo-nextest`. It runs each test in its own process, so a test that aborts or overflows its stack fails on its own. Under `cargo test`, the same crash ends the run of every other test in that binary and hides their results. This matters for the parsers, whose tests run hundreds of fixture files. nextest also runs the tests of all test binaries in one parallel pool, where `cargo test` runs the binaries one at a time. `datatest-stable` comes from the nextest authors and is built to run under it.

Platforms:

- Linux, macOS, and Windows on x86_64 and aarch64 are all first-class. CI runs the test suite on each OS, and on Linux for both architectures.

## Consequences

Malformed workflows produce diagnostics, not crashes. Code style stays uniform across agents and sessions.

The no-panic rule makes some code longer, because every fallible lookup needs explicit handling.

nextest does not run doctests, so CI runs them in a separate `cargo test --doc` step. Contributors install one more tool to run the tests the way CI does.
