# Origin of the vendored libyaml-safer

This directory is a fork of the `libyaml-safer` crate, version 0.3.0, under the MIT license in `LICENSE-MIT`.

- Repository: https://github.com/simonask/libyaml-safer
- Commit, from the published `.cargo_vcs_info.json`: `7fe984df4b2c9003cb319002f0e2669c753eea09`. The file marks the tree as dirty. The published sources differ from that commit only in their CRLF line endings.
- Copied from the crates.io package, SHA-256 `d325b93a27b93b93b7cf4a57d681c5b6a4d8f910e695db989c055eb1e4f657df`, which matches the checksum in the crates.io index. Line endings were converted to LF.

## Kept

`error.rs`, `event.rs`, `macros.rs`, `parser.rs`, `scanner.rs`, and `token.rs`, and the parts of upstream's `lib.rs` they use, now `mod.rs`. `reader.rs` was rewritten.

## Dropped

- The emitter (`emitter.rs`) and the document loader (`document.rs`). workflint builds its own tree from events (ADR 0005).
- The test-suite binaries in `src/bin/`, the benchmarks, and the emitter tests.
- The `Break` enum, the tag constants, `Event` constructors, the parser's alias table, and the composer, emitter, and I/O error kinds. Only the dropped code used them.
- `ScalarStyle::Any`, `SequenceStyle::Any`, and `MappingStyle::Any`, which the parser never produces, and two parser states that nothing entered.

## Changed

- The input is a `&str`. `reader.rs` moves characters from the string into the scanner's buffer and keeps upstream's check for control characters. This removed the one `unsafe` block, UTF-16 decoding, and partial UTF-8 reads. The scanner and parser lost their reader type parameter. `Parser::new` takes the input, and the `set_input*`, `set_encoding`, and `reset` methods are gone.
- A leading byte order mark counts in `Mark::index`, which is now a byte offset into the source. Its column stays 0, as before.
- `read_line_break` does nothing at the end of the input, like libyaml's `READ_LINE` macro. Upstream panicked when a `|` or `>` block scalar ended the input without a final line break.
- In a flow collection, a tag may be followed by `,`, as in libyaml. Upstream panicked on input such as `[!!str, x]`. The yaml-test-suite case WZ62 now passes and left the ignore list.
- A `\` at the end of the input inside a double-quoted scalar reports `found unknown escape character`, as in libyaml. Upstream panicked.
- Every other `panic!`, `unwrap`, `expect`, and `assert!` on a scanner or parser invariant now returns an error with the problem `found an internal parser error`. No input is known to reach one. `read_char` and `skip_char` do nothing at the end of the input.
- Paths changed from `crate::` to `super::`, items not re-exported became `pub(crate)`, and public items gained rustdoc. Two `TODO` comments were removed.
