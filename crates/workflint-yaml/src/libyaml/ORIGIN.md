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
- `Encoding::Any`, the `Scanner::scan` method, and the `Iterator` implementation of `Scanner`. Only a unit test used the last two.
- `stale_simple_keys`, which walked the whole simple key stack for each token. go-yaml checks one key at a time instead, as described below.

## Changed

- The input is a `&str`. `reader.rs` moves characters from the string into the scanner's buffer and keeps upstream's check for control characters. This removed the one `unsafe` block, UTF-16 decoding, and partial UTF-8 reads. The scanner and parser lost their reader type parameter. `Parser::new` takes the input, and the `set_input*`, `set_encoding`, and `reset` methods are gone. `crate::decode` turns UTF-8 and UTF-16 bytes into the string, as go-yaml decodes them.
- A leading byte order mark counts in `Mark::index`, which is now a byte offset into the source. Its column stays 0, as before.
- `read_line_break` does nothing at the end of the input, like libyaml's `READ_LINE` macro. Upstream panicked when a `|` or `>` block scalar ended the input without a final line break.
- In a flow collection, a tag may be followed by `,`, as in libyaml. Upstream panicked on input such as `[!!str, x]`. The yaml-test-suite case WZ62 now passes and left the ignore list. Outside flow collections, a tag takes `,`, `[`, and `]`, as in go-yaml.
- A `\` at the end of the input inside a double-quoted scalar reports `found unknown escape character`, as in libyaml. Upstream panicked.
- Every other `panic!`, `unwrap`, `expect`, and `assert!` on a scanner or parser invariant now returns an error. `Error::internal` takes the stage that found the problem, which `Error::kind` returns and the problem names, as in `found an internal scanner error`. No input is known to reach one. `read_char` and `skip_char` do nothing at the end of the input. `VecDeque::insert` gets its position from `insert_token`, which returns an error for a position past the end of the queue.
- Paths changed from `crate::` to `super::`, items not re-exported became `pub(crate)`, and public items gained rustdoc. Two `TODO` comments and commented-out code in `scan_flow_scalar` were removed.
- The scanner records comments instead of skipping them, and the parser reports each one as an `EventData::Comment` event with a `CommentKind`. The scanner records a comment wherever go-yaml v4.0.0-rc.3 reads one in `internal/libyaml/scanner.go`, as described below. It also records the comment after a directive, which go-yaml drops. go-yaml attaches comments to nodes as head, line, and foot comments. workflint does not, and leaves attachment to its tree (ADR 0009). The parser returns each comment before the first other event that starts after it.
- `Event` has a `node_marks` field of the new type `NodeMarks`. `parse_node` fills it with the marks of the `&anchor` token and the mark where the node's content starts after its anchor and tag. Upstream dropped both, so the tree could not give the anchor its own span. Every other event gets the default value.
- `Error::problem_offset` returns the byte offset of the problem. Reader errors have no problem mark, so this is the only position they report.
- The parser's iterator returns `None` after the first error, as `FusedIterator` promises.

## Changed to match go-yaml

ADR 0022 makes go-yaml v4.0.0-rc.3, the YAML library of actionlint, the reference for this parser, except where go-yaml rejects valid YAML. go-yaml's `internal/libyaml` is a port of libyaml with changes of its own. These changes bring them over. The exceptions are in `docs/differences.md`: `tag-before-flow-comma`, `yaml-solidus-escape`, and `yaml-1-2-directive`.

- The scanner keeps at least three tokens queued ahead of the parser, as `fetchMoreTokens` does, and fetches more while the first queued token can still become a simple key. An error in a queued token comes before the event of an earlier token. `simple_keys_by_token` finds the key of the first queued token, and a key's staleness is checked only there, in `fetch_value`, and when the key is removed, as in `simpleKeyIsValid`.
- The 1,024-character limit of a simple key counts characters, not bytes. A key is valid only on its own line, so the scanner compares columns.
- More than 10,000 flow levels or block indentation levels fail with `exceeded max depth of 10000`, as `max_flow_level` and `max_indents` do.
- `reader.rs` reads the input in go-yaml's 512-byte chunks and decodes a whole chunk at a time, so a control character fails the scan when go-yaml's reader decodes its chunk. A control character after a syntax error in the same chunk wins, and one in a later chunk is not reached.
- At the start of a line, the scanner skips a character only while its buffer starts with a byte order mark, because go-yaml's `isBOM` looks at the start of its buffer, not at the current character. The same check makes every anchor name invalid in that state. A byte order mark later in the stream stays in the content.
- After each token except `-`, the scanner reads a comment on the rest of the line, skipping tabs, as `scanLineComment` does. Between tokens, it reads every comment that follows over blank lines and indentation, tabs included, as `scanComments` does. So `a:\t# c` and a tab before a comment after another comment line parse.
- A block end is where scanning for the next token started, not where the next token starts. If a comment block that `scanComments` records starts at the closing block's indentation, the block end moves one column after its `#`, with go-yaml's line count. This changes the marks of `MappingEnd` and `SequenceEnd` events and of the empty value of an explicit key at the end of a block.
- Anchor and alias names take every printable ASCII character except blanks, flow indicators, and `:`.
- A double-quoted scalar accepts the escape `\'`, and the message for a bad `\x`, `\u`, or `\U` escape says `hexadecimal`.
- In a flow collection, a `:` that no blank follows is part of a plain scalar, `?` ends a plain scalar only before a blank, and `?` starts a key only before a blank. Upstream failed `[b:,c]` with `found unexpected ':'` and read `[?x]` as a mapping.
- A `%YAML` version number has at most two digits.
- Errors in a tag handle have go-yaml's contexts, `while parsing a tag` and `while parsing a %TAG directive`.
- The URI escapes of a tag decode as one UTF-8 character. Upstream added each octet as a character.
- An indentless sequence's end event and the end event of a single-pair mapping in a flow sequence end where they start. The empty value of such a pair starts at its `:`.

- When an empty flow collection starts a simple key, the parser can take its first token before the scanner finds the `:`. go-yaml's `insertToken` then gets a negative position and appends the KEY and BLOCK-MAPPING-START tokens to the queue, and `insert_token` does the same. So `[] x: y` and `{} a: x` are the documents `[]` and `{}`, and `- [] b: 1` fails with `did not find expected '-' indicator`.

One case still differs from go-yaml, which go-yaml handles by accident of its memory layout. go-yaml's `parseFlowSequenceEntryMappingValue` reads the `:` token through a pointer into its token queue. When fetching the next token moves the queue in memory, the pointer sees another token, and the empty value gets that token's position. The parser always uses the `:`.
