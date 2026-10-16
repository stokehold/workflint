# ADR 0025: Fuzz workspace

- Status: accepted
- Date: 2026-10-16
- Amends: ADR 0006, ADR 0017, ADR 0018

## Context

ADR 0006 puts the repository in one Cargo workspace, and ADR 0018 pins one stable toolchain in `rust-toolchain.toml`. The fuzz targets that ADR 0018 asks for are built and run by `cargo fuzz`, which needs a nightly toolchain and its own compiler flags. As members of the main workspace, they would be built by every `cargo build --workspace`, clippy, and nextest run on the stable toolchain, and their dependencies would enter the main `Cargo.lock`.

The fuzz targets depend on `libfuzzer-sys`, whose license is `(MIT OR Apache-2.0) AND NCSA`, because it bundles LLVM's libFuzzer. ADR 0017's license list does not include NCSA.

## Decision

- `fuzz/` is its own Cargo workspace, outside the main one, with its own `Cargo.lock`. It depends on the workspace crates by path.
- `fuzz/rust-toolchain.toml` pins a dated nightly. CI uses the same pin, and a maintainer updates it by hand.
- The fuzz workspace is never published and nothing in a release depends on it.
- CI runs `cargo deny` on the fuzz workspace as well as the main one (M1.19). Its policy allows NCSA for `libfuzzer-sys` only, as a license exception.

## Consequences

The main workspace keeps building on the pinned stable toolchain. Fuzzing needs a nightly install, which `fuzz/rust-toolchain.toml` selects inside `fuzz/`.

The fuzz workspace has a second lock file. Dependabot and `cargo deny` each cover it as a separate entry.

## Alternatives considered

- Keep `fuzz/` in the main workspace and pass `+nightly` only to `cargo fuzz`. Rejected, because every workspace command on stable would then build the fuzz targets, and `libfuzzer-sys` would enter the main `Cargo.lock`.
