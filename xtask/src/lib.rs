//! Code shared by the xtask commands and the conformance test target in
//! `xtask/tests/conformance.rs`.

// ADR 0018 lets xtask panic: it runs only on developer machines and in CI,
// never on user input.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod conformance;
pub mod process;
